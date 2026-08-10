import assert from "node:assert/strict";
import test from "node:test";
import {
  parseTaskControlRows,
  parseTaskExecutionPolicy,
  parseWorkCandidateRows,
  validateActivityPhaseMetadata,
  validateTaskCandidateSeparation,
  validateTaskExecutionPolicy,
  validateTaskPlanBinding,
} from "./check-docs-phases.mjs";

const validPolicy = `
## 机器可校验策略

| Policy | Value |
|---|---|
| \`TASK_SCOPE\` | \`per-task\` |
| \`TASK_AUTHORITY\` | \`explicit-user-goal\` |
| \`DEVELOPMENT_ATTACHED_TESTS\` | \`remain-development\` |
| \`SYSTEM_TEST_CONTROL\` | \`immediate-or-controlled\` |
| \`SYSTEM_TEST_AUTOCREATE\` | \`forbidden\` |
| \`DEPLOYMENT_CONTROL\` | \`controlled-only\` |
| \`DEPLOYMENT_MISSING_EVIDENCE\` | \`block\` |
| \`REPORTING_SCOPE\` | \`established-tasks-only\` |
| \`PERSISTENCE_CONTROL\` | \`independent\` |
| \`TASK_CONTROL_SCOPE\` | \`recoverable-active-work\` |
| \`WORK_CANDIDATE_SCOPE\` | \`known-uncommitted-outcomes\` |
| \`WORK_CANDIDATE_PROMOTION\` | \`explicit-user-task\` |
| \`WORK_CANDIDATE_EXECUTION_TYPE\` | \`classify-on-promotion\` |
| \`WORK_QUERY_COMPLETENESS\` | \`known-unless-audited\` |
`;

test("parses and accepts the structured task execution policy", () => {
  assert.equal(parseTaskExecutionPolicy(validPolicy).policy.get("TASK_SCOPE"), "per-task");
  assert.deepEqual(validateTaskExecutionPolicy(validPolicy), []);
});

test("rejects global scope, controlled-only system tests, automatic follow-ons, and broad reporting", () => {
  const invalid = validPolicy
    .replace("per-task", "project-global")
    .replace("remain-development", "split-system-test")
    .replace("immediate-or-controlled", "controlled-only")
    .replace("SYSTEM_TEST_AUTOCREATE\` | \`forbidden", "SYSTEM_TEST_AUTOCREATE\` | \`allowed")
    .replace("DEPLOYMENT_MISSING_EVIDENCE\` | \`block", "DEPLOYMENT_MISSING_EVIDENCE\` | \`start-system-test")
    .replace("established-tasks-only", "all-types");
  const errors = validateTaskExecutionPolicy(invalid);
  assert.ok(errors.some((error) => error.includes("TASK_SCOPE")));
  assert.ok(errors.some((error) => error.includes("DEVELOPMENT_ATTACHED_TESTS")));
  assert.ok(errors.some((error) => error.includes("SYSTEM_TEST_CONTROL")));
  assert.ok(errors.some((error) => error.includes("SYSTEM_TEST_AUTOCREATE")));
  assert.ok(errors.some((error) => error.includes("DEPLOYMENT_MISSING_EVIDENCE")));
  assert.ok(errors.some((error) => error.includes("REPORTING_SCOPE")));
});

test("accepts only recoverable active task statuses and fixed controlled targets", () => {
  const text = `
## 当前队列
| DEV-001 | InProgress | Development | X | - | scoped change | \`src/file.ts\` |
| SYS-001 | Blocked | SystemTest | X | candidate-42 | wait for environment | \`文档/工作空间/推进中/TEST.md\` |
| DEP-001 | Review | Deployment | X | 1.2.3 | verify deployed artifact | \`文档/工作空间/推进中/DEPLOY.md\` |
`;
  assert.deepEqual(parseTaskControlRows(text).errors, []);
});

test("rejects inactive task statuses, obsolete sections, mixed types, and pending targets", () => {
  const text = `
## 当前队列
| DEV-001 | InProgress | Development | X | 1.2.3 | [混合] build and deploy | \`src/file.ts\` |
| SYS-001 | Ready | SystemTest | X | 待候选 | test later | \`test.md\` |
## 候选积压
| DEP-001 | Backlog | Deployment | X | - | deploy | \`deploy.md\` |
`;
  const { errors } = parseTaskControlRows(text);
  assert.ok(errors.some((error) => error.includes("Development task must use -")));
  assert.ok(errors.some((error) => error.includes("mixed phase task")));
  assert.ok(errors.some((error) => error.includes("invalid task status SYS-001")));
  assert.ok(errors.some((error) => error.includes("pending candidate/artifact is forbidden")));
  assert.ok(errors.some((error) => error.includes("obsolete task section")));
  assert.ok(errors.some((error) => error.includes("task outside current queue")));
  assert.ok(errors.some((error) => error.includes("Deployment task requires")));
});

test("accepts evidence-based candidates without execution metadata", () => {
  const text = `
## 候选结果
| OUTCOME-001 | X | verified-gap | close a verified gap | \`src/file.ts\` | user requests the outcome | - |
| OUTCOME-002 | Y | contract-obligation | fulfill the contract | \`文档/contract.md\` | user selects this outcome | OUTCOME-001 |
`;
  assert.deepEqual(parseWorkCandidateRows(text).errors, []);
});

test("rejects invalid candidate basis, execution type, pending metadata, and active overlap", () => {
  const text = `
## 候选结果
| SAME-001 | X | guess | run a SystemTest later | \`src/file.ts\` | 待授权 | - |
`;
  const { rows, errors } = parseWorkCandidateRows(text);
  assert.ok(errors.some((error) => error.includes("invalid work candidate basis")));
  assert.ok(errors.some((error) => error.includes("must not declare execution type")));
  assert.ok(errors.some((error) => error.includes("must not contain pending metadata")));
  assert.deepEqual(validateTaskCandidateSeparation([{ taskId: "SAME-001" }], rows), ["active task overlaps work candidate SAME-001"]);
});

test("requires user authority before controlled plans and fixed metadata before Active", () => {
  assert.deepEqual(validateActivityPhaseMetadata({
    kind: "SystemTestPlan",
    status: "Draft",
    workflow: "WF-0005",
    phase: "SystemTest",
    requestedBy: "user-request-42",
    candidate: "candidate-42",
    environment: "qa",
  }), []);

  const pendingSystemTestErrors = validateActivityPhaseMetadata({
    kind: "SystemTestPlan",
    status: "Draft",
    workflow: "WF-0005",
    phase: "SystemTest",
    requestedBy: "user-request-42",
    candidate: "pending",
    environment: "qa",
  });
  assert.ok(pendingSystemTestErrors.includes("SystemTestPlan requires fixed Candidate and Environment before creation"));

  assert.deepEqual(validateActivityPhaseMetadata({
    kind: "DeploymentPlan",
    status: "Active",
    workflow: "WF-0006",
    phase: "Deployment",
    artifact: "1.2.3",
    target: "production-a",
    authorization: "user-request-42",
    admissionEvidence: "TEST-42",
    rollback: "RUN-0004",
  }), []);

  assert.ok(validateActivityPhaseMetadata({
    kind: "SystemTestPlan",
    status: "Draft",
    workflow: "WF-0005",
    phase: "SystemTest",
    requestedBy: "pending",
    candidate: "pending",
    environment: "pending",
  }).includes("SystemTestPlan requires an explicit user request source before creation"));

  assert.ok(validateActivityPhaseMetadata({
    kind: "SystemTestPlan",
    status: "Draft",
    workflow: "WF-0005",
    phase: "SystemTest",
    candidate: "pending",
    environment: "pending",
  }).includes("SystemTestPlan is missing Requested By"));

  assert.ok(validateActivityPhaseMetadata({
    kind: "SystemTestPlan",
    status: "Draft",
    workflow: "WF-0005",
    phase: "SystemTest",
    requestedBy: "CI",
    candidate: "pending",
    environment: "pending",
  }).includes("SystemTestPlan requires an explicit user request source before creation"));

  assert.ok(validateActivityPhaseMetadata({
    kind: "DeploymentPlan",
    status: "Draft",
    workflow: "WF-0006",
    phase: "Deployment",
    artifact: "1.2.3",
    target: "pending",
    authorization: "pending",
    admissionEvidence: "pending",
    rollback: "pending",
  }).some((error) => error.includes("before creation")));

  assert.ok(validateActivityPhaseMetadata({
    kind: "DeploymentPlan",
    status: "Draft",
    workflow: "WF-0006",
    phase: "Deployment",
    artifact: "1.2.3",
    target: "production-a",
    authorization: "tests-passed",
    admissionEvidence: "TEST-42",
    rollback: "pending",
  }).includes("DeploymentPlan requires current user authorization before creation"));
});

test("binds current system test and deployment tasks to matching plans", () => {
  const row = {
    taskId: "SYS-001",
    status: "Blocked",
    phase: "SystemTest",
    candidateOrArtifact: "待候选",
    section: "current",
  };
  assert.deepEqual(validateTaskPlanBinding(row, {
    kind: "SystemTestPlan",
    status: "Draft",
    candidate: "pending",
  }), []);
  assert.ok(validateTaskPlanBinding({ ...row, status: "Review" }, {
    kind: "SystemTestPlan",
    status: "Draft",
    candidate: "pending",
  }).some((error) => error.includes("cannot be Review")));
  assert.ok(validateTaskPlanBinding(row, {
    kind: "DeploymentPlan",
    status: "Draft",
    artifact: "pending",
  }).some((error) => error.includes("must enter through SystemTestPlan")));
});
