export const DELIVERY_PHASES = new Set(["Development", "SystemTest", "Deployment"]);
export const ACTIVITY_KINDS = new Set(["ChangePlan", "SystemTestPlan", "DeploymentPlan", "Issue"]);
export const PLAN_KINDS = new Set(["ChangePlan", "SystemTestPlan", "DeploymentPlan"]);

const allowedStatuses = new Set(["InProgress", "Review", "Blocked"]);
const nonUserAuthorityMarkers = new Set(["pending", "ci", "release-gate", "release gate", "automation", "tests-passed"]);
export const WORK_CANDIDATE_BASES = new Set(["user-deferred", "verified-gap", "contract-obligation"]);

export const TASK_EXECUTION_POLICY = new Map([
  ["TASK_SCOPE", "per-task"],
  ["TASK_AUTHORITY", "explicit-user-goal"],
  ["DEVELOPMENT_ATTACHED_TESTS", "remain-development"],
  ["SYSTEM_TEST_CONTROL", "immediate-or-controlled"],
  ["SYSTEM_TEST_AUTOCREATE", "forbidden"],
  ["DEPLOYMENT_CONTROL", "controlled-only"],
  ["DEPLOYMENT_MISSING_EVIDENCE", "block"],
  ["REPORTING_SCOPE", "established-tasks-only"],
  ["PERSISTENCE_CONTROL", "independent"],
  ["TASK_CONTROL_SCOPE", "recoverable-active-work"],
  ["WORK_CANDIDATE_SCOPE", "known-uncommitted-outcomes"],
  ["WORK_CANDIDATE_PROMOTION", "explicit-user-task"],
  ["WORK_CANDIDATE_EXECUTION_TYPE", "classify-on-promotion"],
  ["WORK_QUERY_COMPLETENESS", "known-unless-audited"],
]);

export function parseTaskExecutionPolicy(text) {
  const policy = new Map();
  const errors = [];
  let inPolicySection = false;
  for (const line of text.split(/\r?\n/)) {
    if (line === "## 机器可校验策略") {
      inPolicySection = true;
      continue;
    }
    if (inPolicySection && line.startsWith("## ")) break;
    if (!inPolicySection) continue;
    const match = line.match(/^\|\s*`?([A-Z][A-Z0-9_]*)`?\s*\|\s*`?([a-z][a-z0-9-]*)`?\s*\|$/);
    if (!match) continue;
    const [, key, value] = match;
    if (policy.has(key)) errors.push(`duplicate task execution policy ${key}`);
    policy.set(key, value);
  }
  return { policy, errors };
}

export function validateTaskExecutionPolicy(text) {
  const { policy, errors } = parseTaskExecutionPolicy(text);
  for (const [key, expected] of TASK_EXECUTION_POLICY) {
    const actual = policy.get(key);
    if (!actual) errors.push(`missing task execution policy ${key}`);
    else if (actual !== expected) errors.push(`invalid task execution policy ${key} -> ${actual}; expected ${expected}`);
  }
  for (const key of policy.keys()) {
    if (!TASK_EXECUTION_POLICY.has(key)) errors.push(`unexpected task execution policy ${key}`);
  }
  return errors;
}

export function parseTaskControlRows(text) {
  const errors = [];
  const rows = [];
  let section = "";

  for (const line of text.split(/\r?\n/)) {
    if (line === "## 当前队列") {
      section = "current";
      continue;
    }
    if (line === "## 候选积压") {
      errors.push("obsolete task section 候选积压");
      section = "obsolete";
      continue;
    }
    if (line === "## 长期触发") {
      errors.push("obsolete task section 长期触发");
      section = "obsolete";
      continue;
    }
    if (line.startsWith("## ")) {
      section = "";
      continue;
    }
    if (!/^\|\s*[A-Z][A-Z0-9-]+\s*\|/.test(line)) continue;

    const cells = line.split("|").slice(1, -1).map((cell) => cell.trim());
    const [taskId, status, phase, scope, candidateOrArtifact, nextResult, entry] = cells;
    if (taskId === "ID") continue;
    if (cells.length !== 7) {
      errors.push(`invalid task row shape ${taskId}`);
      continue;
    }

    const entryReference = entry.match(/`([^`\r\n]+)`/)?.[1];
    const row = {
      taskId,
      status,
      phase,
      scope,
      candidateOrArtifact,
      nextResult,
      entry,
      entryReference,
      section,
    };
    rows.push(row);

    if (!allowedStatuses.has(status)) errors.push(`invalid task status ${taskId} -> ${status}`);
    if (!DELIVERY_PHASES.has(phase)) errors.push(`invalid task phase ${taskId} -> ${phase}`);
    if (section !== "current") errors.push(`task outside current queue ${taskId}`);

    if (phase === "Development" && candidateOrArtifact !== "-") {
      errors.push(`Development task must use - candidate/artifact ${taskId}`);
    }
    if (["SystemTest", "Deployment"].includes(phase) && candidateOrArtifact === "-") {
      errors.push(`${phase} task requires candidate/artifact ${taskId}`);
    }
    if (/^(?:待|pending$)/i.test(candidateOrArtifact)) {
      errors.push(`pending candidate/artifact is forbidden ${taskId}`);
    }
    if (nextResult.includes("[混合]") || nextResult.includes("混合阶段")) {
      errors.push(`mixed phase task is forbidden ${taskId}`);
    }
    if (!entryReference) errors.push(`task entry must contain one backticked reference ${taskId}`);
  }

  const counts = new Map();
  for (const row of rows) counts.set(row.taskId, (counts.get(row.taskId) ?? 0) + 1);
  for (const [taskId, count] of counts) {
    if (count > 1) errors.push(`duplicate task ID ${taskId}`);
  }

  return { rows, errors };
}

export function parseWorkCandidateRows(text) {
  const errors = [];
  const rows = [];
  let inCandidateSection = false;

  for (const line of text.split(/\r?\n/)) {
    if (line === "## 候选结果") {
      inCandidateSection = true;
      continue;
    }
    if (line.startsWith("## ")) {
      inCandidateSection = false;
      continue;
    }
    if (!inCandidateSection || !/^\|\s*[A-Z][A-Z0-9-]+\s*\|/.test(line)) continue;

    const cells = line.split("|").slice(1, -1).map((cell) => cell.trim());
    const [outcomeKey, scope, basis, remainingOutcome, owner, trigger, dependencies] = cells;
    if (outcomeKey === "Outcome Key") continue;
    if (cells.length !== 7) {
      errors.push(`invalid work candidate row shape ${outcomeKey}`);
      continue;
    }

    const ownerReference = owner.match(/^`([^`\r\n]+)`$/)?.[1];
    rows.push({ outcomeKey, scope, basis, remainingOutcome, owner, ownerReference, trigger, dependencies });

    if (!WORK_CANDIDATE_BASES.has(basis)) errors.push(`invalid work candidate basis ${outcomeKey} -> ${basis}`);
    if (!scope || !remainingOutcome || !trigger) errors.push(`incomplete work candidate ${outcomeKey}`);
    if (!ownerReference) errors.push(`work candidate owner must be one backticked reference ${outcomeKey}`);
    if (!dependencies) errors.push(`work candidate dependencies are missing ${outcomeKey}`);
    if (/\b(?:Development|SystemTest|Deployment)\b/.test(cells.join(" "))) {
      errors.push(`work candidate must not declare execution type ${outcomeKey}`);
    }
    if (/\bpending\b|待候选|待授权|待环境|待目标/i.test(cells.join(" "))) {
      errors.push(`work candidate must not contain pending metadata ${outcomeKey}`);
    }
  }

  const counts = new Map();
  for (const row of rows) counts.set(row.outcomeKey, (counts.get(row.outcomeKey) ?? 0) + 1);
  for (const [outcomeKey, count] of counts) {
    if (count > 1) errors.push(`duplicate work candidate ${outcomeKey}`);
  }
  return { rows, errors };
}

export function validateTaskCandidateSeparation(taskRows, candidateRows) {
  const taskIds = new Set(taskRows.map((row) => row.taskId));
  return candidateRows
    .filter((row) => taskIds.has(row.outcomeKey))
    .map((row) => `active task overlaps work candidate ${row.outcomeKey}`);
}

export function validateActivityPhaseMetadata({ kind, status, workflow, phase, requestedBy, candidate, environment, artifact, target, authorization, admissionEvidence, rollback }) {
  const errors = [];
  const expected = {
    ChangePlan: { workflow: "WF-0002", phase: "Development" },
    SystemTestPlan: { workflow: "WF-0005", phase: "SystemTest" },
    DeploymentPlan: { workflow: "WF-0006", phase: "Deployment" },
    Issue: { workflow: "WF-0003" },
  }[kind];
  if (!expected) return errors;

  if (workflow !== expected.workflow) errors.push(`${kind} must bind ${expected.workflow}`);
  if (expected.phase && phase !== expected.phase) errors.push(`${kind} must declare Phase ${expected.phase}`);

  if (kind === "SystemTestPlan") {
    if (!requestedBy) errors.push("SystemTestPlan is missing Requested By");
    if (!candidate) errors.push("SystemTestPlan is missing Candidate");
    if (!environment) errors.push("SystemTestPlan is missing Environment");
    if (requestedBy && nonUserAuthorityMarkers.has(requestedBy.toLowerCase())) {
      errors.push("SystemTestPlan requires an explicit user request source before creation");
    }
    if ([candidate, environment].some((value) => !value || value === "pending")) {
      errors.push("SystemTestPlan requires fixed Candidate and Environment before creation");
    }
    if (status === "Active" && [candidate, environment].some((value) => !value || value === "pending")) {
      errors.push("Active SystemTestPlan requires fixed Candidate and Environment");
    }
  }

  if (kind === "DeploymentPlan") {
    const fields = { Artifact: artifact, Target: target, Authorization: authorization, "Admission Evidence": admissionEvidence, Rollback: rollback };
    for (const [name, value] of Object.entries(fields)) {
      if (!value) errors.push(`DeploymentPlan is missing ${name}`);
    }
    for (const [name, value] of Object.entries({ Artifact: artifact, Target: target, Authorization: authorization })) {
      if (value === "pending") errors.push(`DeploymentPlan requires fixed ${name} before creation`);
    }
    if (authorization && nonUserAuthorityMarkers.has(authorization.toLowerCase())) {
      errors.push("DeploymentPlan requires current user authorization before creation");
    }
    if (status === "Active" && Object.values(fields).some((value) => !value || value === "pending")) {
      errors.push("Active DeploymentPlan requires fixed Artifact, Target, Authorization, Admission Evidence, and Rollback");
    }
  }

  return errors;
}

export function validateTaskPlanBinding(row, activity) {
  if (row.section !== "current" || !["SystemTest", "Deployment"].includes(row.phase)) return [];
  const errors = [];
  const expectedKind = row.phase === "SystemTest" ? "SystemTestPlan" : "DeploymentPlan";
  if (activity.kind !== expectedKind) {
    errors.push(`current ${row.phase} task ${row.taskId} must enter through ${expectedKind}`);
    return errors;
  }
  if (row.status !== "Blocked" && activity.status !== "Active") {
    errors.push(`${row.taskId} cannot be ${row.status} while ${expectedKind} is ${activity.status ?? "missing"}`);
  }
  const planCandidate = row.phase === "SystemTest" ? activity.candidate : activity.artifact;
  if (row.candidateOrArtifact.startsWith("待")) {
    if (planCandidate !== "pending") errors.push(`${row.taskId} pending candidate/artifact does not match plan metadata`);
  } else if (planCandidate !== row.candidateOrArtifact) {
    errors.push(`${row.taskId} candidate/artifact does not match plan metadata`);
  }
  return errors;
}
