import fs from "node:fs";
import path from "node:path";
import { validateHumanDocumentation } from "./check-docs-human.mjs";
import {
  containsMachineSpecificWorkspacePath,
  resolveDocumentReference,
} from "./check-docs-references.mjs";
import {
  ACTIVITY_KINDS,
  PLAN_KINDS,
  allowsMissingActivityDirectory,
  parseTaskControlRows,
  parseWorkCandidateRows,
  validateActivityPhaseMetadata,
  validateTaskCandidateSeparation,
  validateTaskExecutionPolicy,
  validateTaskPlanBinding,
} from "./check-docs-phases.mjs";

const root = process.cwd();
const docsRoot = path.join(root, "文档");
const structurePath = path.join(docsRoot, "WORKSPACE_STRUCTURE.md");
const ignored = new Set([
  ".git",
  ".reports",
  ".codex-build",
  ".venv",
  ".explore-output",
  "bin",
  "dist",
  "logs",
  "node_modules",
  "obj",
  "target",
]);

function walk(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isDirectory() && ignored.has(entry.name)) return [];
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) return walk(full);
    return /\.mdx?$/i.test(entry.name) ? [full] : [];
  });
}

function walkAll(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isDirectory() && ignored.has(entry.name)) return [];
    const full = path.join(directory, entry.name);
    return entry.isDirectory() ? walkAll(full) : [full];
  });
}

function relative(file, base = root) {
  return path.relative(base, file).split(path.sep).join("/");
}

function resolveReference(source, raw) {
  return resolveDocumentReference({ root, docsRoot, source, raw });
}

const files = walk(root);
const errors = [];
const texts = new Map(files.map((file) => [file, fs.readFileSync(file, "utf8")]));
const formal = files.filter((file) => file.startsWith(`${docsRoot}${path.sep}`));
const governed = [path.join(root, "AGENTS.md"), ...formal];
errors.push(...validateHumanDocumentation({ root, files, resolveReference }));

if (!fs.existsSync(structurePath)) {
  console.error("Documentation check failed (1):");
  console.error("- 文档/WORKSPACE_STRUCTURE.md: missing structure contract");
  process.exit(1);
}

const structureText = fs.readFileSync(structurePath, "utf8");
const placements = [...structureText.matchAll(/^- `([^`]+)` -> `([^`]+)`\s*$/gm)].map((match) => {
  const declaredPath = match[1].replaceAll("\\", "/");
  const isDirectory = declaredPath.endsWith("/");
  const target = path.resolve(docsRoot, ...declaredPath.replace(/\/$/, "").split("/"));
  return {
    declaredPath,
    isDirectory,
    target,
    kinds: match[2].split(",").map((kind) => kind.trim()).filter(Boolean),
  };
});
const statusRules = new Map([...structureText.matchAll(/^- `([^`]+)`: `([^`]+)`\s*$/gm)].map((match) => [
  match[1].trim(),
  match[2].split(",").map((status) => status.trim()).filter(Boolean),
]));

if (placements.length === 0) {
  errors.push("文档/WORKSPACE_STRUCTURE.md: no governed document locations declared");
}
if (statusRules.size === 0) {
  errors.push("文档/WORKSPACE_STRUCTURE.md: no document status rules declared");
}

for (const placement of placements) {
  if (placement.target !== docsRoot && !placement.target.startsWith(`${docsRoot}${path.sep}`)) {
    errors.push(`文档/WORKSPACE_STRUCTURE.md: governed path escapes 文档/ (${placement.declaredPath})`);
  } else if (!fs.existsSync(placement.target) &&
      !allowsMissingActivityDirectory(placement.isDirectory, placement.kinds)) {
    errors.push(`文档/WORKSPACE_STRUCTURE.md: declared path does not exist (${placement.declaredPath})`);
  }
  if (placement.kinds.length === 0) {
    errors.push(`文档/WORKSPACE_STRUCTURE.md: declared path has no Kind (${placement.declaredPath})`);
  }
  for (const kind of placement.kinds) {
    if (!statusRules.has(kind)) {
      errors.push(`文档/WORKSPACE_STRUCTURE.md: Kind ${kind} has no declared status lifecycle`);
    }
  }
}

const expectedDocsDirectories = new Set(placements
  .map((placement) => placement.declaredPath.split("/")[0])
  .filter((entry) => entry && !/\.mdx?$/i.test(entry)));
const actualDocsDirectories = new Set(fs.readdirSync(docsRoot, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name));
for (const directory of actualDocsDirectories) {
  if (!expectedDocsDirectories.has(directory)) {
    errors.push(`文档/${directory}: undeclared documentation root directory`);
  }
}
for (const directory of expectedDocsDirectories) {
  if (!actualDocsDirectories.has(directory)) {
    errors.push(`文档/WORKSPACE_STRUCTURE.md: declared documentation root directory is missing (${directory})`);
  }
}

function placementFor(file) {
  const candidates = placements.filter((placement) => placement.isDirectory
    ? file.startsWith(`${placement.target}${path.sep}`)
    : file === placement.target);
  return candidates.sort((left, right) => right.target.length - left.target.length)[0];
}

for (const file of formal) {
  const label = relative(file);
  if (path.basename(file).toLowerCase() === "readme.md") {
    errors.push(`${label}: README.md is not allowed in the governed documentation tree`);
  }

  const placement = placementFor(file);
  if (!placement) {
    errors.push(`${label}: file is outside every governed location in WORKSPACE_STRUCTURE.md`);
    continue;
  }

  const head = texts.get(file).split(/\r?\n/).slice(0, 24).join("\n");
  const kind = head.match(/^Kind:\s*(\S+)\s*$/m)?.[1];
  const status = head.match(/^Status:\s*(\S+)\s*$/m)?.[1];
  if (!kind) {
    errors.push(`${label}: missing Kind metadata in first 24 lines`);
  } else if (!placement.kinds.includes(kind)) {
    errors.push(`${label}: Kind ${kind} is not allowed by ${placement.declaredPath} (${placement.kinds.join(", ")})`);
  } else if (status && !statusRules.get(kind)?.includes(status)) {
    errors.push(`${label}: Status ${status} is not allowed for Kind ${kind} (${statusRules.get(kind)?.join(", ")})`);
  }
}

const materialPlacements = placements.filter((placement) =>
  placement.isDirectory && placement.kinds.includes("Material"));
for (const materialPlacement of materialPlacements) {
  if (!fs.existsSync(materialPlacement.target)) continue;
  const localLinkSources = governed.map((source) => ({
    source,
    targets: [...texts.get(source).matchAll(/\[[^\]]*\]\(([^)]+)\)/g)]
      .map((match) => resolveReference(source, match[1]))
      .filter(Boolean)
      .map((target) => path.normalize(target)),
  }));
  for (const file of walkAll(materialPlacement.target)) {
    const normalized = path.normalize(file);
    const consumed = localLinkSources.some(({ source, targets }) =>
      path.normalize(source) !== normalized && targets.includes(normalized));
    if (!consumed) {
      errors.push(`${relative(file)}: material is not linked by a current governed document`);
    }
  }
}

function metadata(file, key) {
  const head = texts.get(file).split(/\r?\n/).slice(0, 24).join("\n");
  return head.match(new RegExp(`^${key}:\\s*(\\S+)\\s*$`, "m"))?.[1];
}

const workflowContracts = formal.filter((file) => metadata(file, "Kind") === "WorkflowContract");
if (workflowContracts.length !== 1) {
  errors.push(`文档/工作流/: expected exactly one WorkflowContract, found ${workflowContracts.length}`);
}
const workflowContractText = workflowContracts.length === 1 ? texts.get(workflowContracts[0]) : "";
const declaredActionList = [...workflowContractText.matchAll(/^\| `(ACT-[A-Z0-9-]+)` \|/gm)]
  .map((match) => match[1]);
const declaredActions = new Set(declaredActionList);
if (declaredActions.size === 0) {
  errors.push("文档/工作流/WORKFLOW_CONTRACT.md: no Action declarations found");
}
if (declaredActions.size !== declaredActionList.length) {
  errors.push("文档/工作流/WORKFLOW_CONTRACT.md: duplicate Action ID");
}

const workflowFiles = formal.filter((file) => metadata(file, "Kind") === "Workflow");
const workflows = new Map();
for (const file of workflowFiles) {
  const label = relative(file);
  const workflowId = metadata(file, "Workflow ID");
  if (!workflowId || !/^WF-\d{4}$/.test(workflowId)) {
    errors.push(`${label}: missing or invalid Workflow ID`);
    continue;
  }
  if (workflows.has(workflowId)) {
    errors.push(`${label}: duplicate Workflow ID ${workflowId}`);
  } else {
    workflows.set(workflowId, file);
  }
  const text = texts.get(file);
  for (const section of ["Trigger", "Input", "Output", "Actions", "Key Constraints", "Stop When"]) {
    if (!new RegExp(`^## ${section}\\s*$`, "m").test(text)) {
      errors.push(`${label}: missing workflow section ${section}`);
    }
  }
  const actionReferences = new Set(text.match(/\bACT-[A-Z0-9-]+\b/g) ?? []);
  if (actionReferences.size === 0) {
    errors.push(`${label}: workflow does not reference any Action`);
  }
  for (const actionId of actionReferences) {
    if (!declaredActions.has(actionId)) {
      errors.push(`${label}: references undeclared Action ${actionId}`);
    }
  }
  if (workflowContractText && !workflowContractText.includes(path.basename(file))) {
    errors.push(`${label}: workflow is not routed by WORKFLOW_CONTRACT.md`);
  }
}

const taskControlPath = path.join(docsRoot, "TASK_CONTROL.md");
const taskControlText = texts.get(taskControlPath) ?? "";
const { rows: taskRows, errors: taskPhaseErrors } = parseTaskControlRows(taskControlText);
errors.push(...taskPhaseErrors.map((error) => `文档/TASK_CONTROL.md: ${error}`));
const workCandidatesPath = path.join(docsRoot, "WORK_CANDIDATES.md");
const workCandidatesText = texts.get(workCandidatesPath) ?? "";
const { rows: workCandidateRows, errors: workCandidateErrors } = parseWorkCandidateRows(workCandidatesText);
errors.push(...workCandidateErrors.map((error) => `文档/WORK_CANDIDATES.md: ${error}`));
errors.push(...validateTaskCandidateSeparation(taskRows, workCandidateRows));
for (const error of validateTaskExecutionPolicy(workflowContractText)) {
  errors.push(`task execution semantics: ${error}`);
}

for (const match of taskControlText.matchAll(/`([^`\r\n]+)`/g)) {
  const reference = match[1].replaceAll("\\", "/");
  if (/[<>{}*]/.test(reference)) continue;
  if (!reference.includes("/")) continue;
  const target = resolveReference(taskControlPath, reference);
  if (!target || (target !== root && !target.startsWith(`${root}${path.sep}`))) continue;
  if (!fs.existsSync(target)) {
    errors.push(`文档/TASK_CONTROL.md: broken task entry ${reference}`);
  }
}

for (const match of taskControlText.matchAll(/`(文档\/(?:工作空间|项目\/项目_[^/]+)\/推进中\/[^`\r\n]+\.md)`/g)) {
  const target = path.resolve(root, ...match[1].split("/"));
  if (!fs.existsSync(target)) {
    errors.push(`文档/TASK_CONTROL.md: missing active document ${match[1]}`);
  }
}
for (const row of workCandidateRows) {
  const target = row.ownerReference ? resolveReference(workCandidatesPath, row.ownerReference) : null;
  if (target && !fs.existsSync(target)) {
    errors.push(`文档/WORK_CANDIDATES.md: broken work candidate owner ${row.ownerReference}`);
  }
}
const formalKinds = new Map(formal.map((file) => [path.normalize(file), metadata(file, "Kind")]));

for (const row of taskRows) {
  if (row.section !== "current" || !["SystemTest", "Deployment"].includes(row.phase)) continue;
  const target = row.entryReference ? resolveReference(taskControlPath, row.entryReference) : null;
  const activity = target && texts.has(target) ? {
    kind: formalKinds.get(path.normalize(target)),
    status: metadata(target, "Status"),
    candidate: metadata(target, "Candidate"),
    artifact: metadata(target, "Artifact"),
  } : {};
  for (const error of validateTaskPlanBinding(row, activity)) {
    errors.push(`文档/TASK_CONTROL.md: ${error}`);
  }
}

for (const file of formal) {
  const kind = metadata(file, "Kind");
  if (!ACTIVITY_KINDS.has(kind)) continue;
  const name = path.basename(file);
  if (!taskControlText.includes(name)) {
    errors.push(`${relative(file)}: active document is not referenced by 文档/TASK_CONTROL.md`);
  }
  const activityMetadata = {
    kind,
    status: metadata(file, "Status"),
    workflow: metadata(file, "Workflow"),
    phase: metadata(file, "Phase"),
    requestedBy: metadata(file, "Requested By"),
    candidate: metadata(file, "Candidate"),
    environment: metadata(file, "Environment"),
    artifact: metadata(file, "Artifact"),
    target: metadata(file, "Target"),
    authorization: metadata(file, "Authorization"),
    admissionEvidence: metadata(file, "Admission Evidence"),
    rollback: metadata(file, "Rollback"),
  };
  for (const error of validateActivityPhaseMetadata(activityMetadata)) {
    errors.push(`${relative(file)}: ${error}`);
  }
  if (activityMetadata.workflow && !workflows.has(activityMetadata.workflow)) {
    errors.push(`${relative(file)}: references undeclared Workflow ${activityMetadata.workflow}`);
  }
}

const planOwnerKinds = new Set([
  "AgentEntry",
  "CurrentDesign",
  "Decision",
  "ProductContract",
  "Runbook",
  "StructureContract",
  "Workflow",
  "WorkflowContract",
]);
const durableKinds = new Set([...planOwnerKinds, "WorkInventory"]);
for (const row of workCandidateRows) {
  const target = row.ownerReference ? resolveReference(workCandidatesPath, row.ownerReference) : null;
  const kind = target ? formalKinds.get(path.normalize(target)) : undefined;
  if (kind && !planOwnerKinds.has(kind)) {
    errors.push(`文档/WORK_CANDIDATES.md: work candidate owner ${row.ownerReference} is ${kind}, not a durable fact owner`);
  }
}
for (const file of formal.filter((candidate) => PLAN_KINDS.has(metadata(candidate, "Kind")))) {
  const lines = texts.get(file).split(/\r?\n/);
  const dependsIndex = lines.findIndex((line) => line.startsWith("Depends On:"));
  let hasDurableOwner = false;
  if (dependsIndex >= 0) {
    for (let index = dependsIndex + 1; index < lines.length; index += 1) {
      const match = lines[index].match(/^-\s+(.+)$/);
      if (!match) break;
      const target = resolveReference(file, match[1]);
      if (target && planOwnerKinds.has(formalKinds.get(path.normalize(target)))) {
        hasDurableOwner = true;
        break;
      }
    }
  }
  if (!hasDurableOwner) {
    errors.push(`${relative(file)}: ${metadata(file, "Kind")} must depend on at least one durable formal owner`);
  }
}
for (const file of formal) {
  const kind = metadata(file, "Kind");
  if (!durableKinds.has(kind)) continue;
  const lines = texts.get(file).split(/\r?\n/);
  const dependsIndex = lines.findIndex((line) => line.startsWith("Depends On:"));
  if (dependsIndex < 0) continue;
  for (let index = dependsIndex + 1; index < lines.length; index += 1) {
    const match = lines[index].match(/^-\s+(.+)$/);
    if (!match) break;
    if (/^(?:none|无)$/i.test(match[1].trim())) continue;
    const target = resolveReference(file, match[1]);
    const targetKind = target ? formalKinds.get(path.normalize(target)) : undefined;
    if (ACTIVITY_KINDS.has(targetKind)) {
      errors.push(`${relative(file)}: durable ${kind} must not depend on temporary ${targetKind} ${relative(target)}`);
    }
  }
}

const projectAgentPlacements = placements.filter((placement) =>
  !placement.isDirectory && placement.kinds.includes("AgentEntry"));
for (const projectAgentPlacement of projectAgentPlacements) {
  const projectRoot = path.dirname(projectAgentPlacement.target);
  const expectedDirectories = new Set(placements
    .filter((placement) => placement.isDirectory && path.dirname(placement.target) === projectRoot)
    .map((placement) => path.basename(placement.target)));
  const optionalDirectories = new Set(placements
    .filter((placement) => placement.isDirectory && path.dirname(placement.target) === projectRoot &&
      allowsMissingActivityDirectory(placement.isDirectory, placement.kinds))
    .map((placement) => path.basename(placement.target)));
  const actualDirectories = new Set(fs.readdirSync(projectRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name));
  for (const directory of actualDirectories) {
    if (!expectedDirectories.has(directory)) {
      errors.push(`${relative(projectRoot)}/${directory}: undeclared project documentation directory`);
    }
  }
  for (const directory of expectedDirectories) {
    if (!actualDirectories.has(directory) && !optionalDirectories.has(directory)) {
      errors.push(`文档/WORKSPACE_STRUCTURE.md: declared project directory is missing (${directory})`);
    }
  }
}

for (const file of governed) {
  const text = texts.get(file);
  const lines = text.split(/\r?\n/);
  const head = lines.slice(0, 24).join("\n");
  const label = relative(file);

  if (!lines.find((line) => line.trim())?.startsWith("# ")) {
    errors.push(`${label}: first content line must be an H1`);
  }
  for (const key of ["Status", "Scope", "Owner", "Updated", "Depends On"]) {
    if (!new RegExp(`^${key}:`, "m").test(head)) {
      errors.push(`${label}: missing ${key} metadata in first 24 lines`);
    }
  }
  if (file !== path.join(root, "AGENTS.md") && !/^Kind:\s*\S+/m.test(head)) {
    errors.push(`${label}: missing Kind metadata in first 24 lines`);
  }
  for (const key of ["Status", "Kind", "Workflow", "Workflow ID", "Phase", "Requested By", "Candidate", "Environment", "Artifact", "Target", "Authorization", "Admission Evidence", "Rollback", "Scope", "Owner", "Updated", "Depends On"]) {
    const matches = head.match(new RegExp(`^${key}:`, "gm")) ?? [];
    if (matches.length > 1) errors.push(`${label}: duplicate ${key} metadata`);
  }
  if (/^Status:\s*(?:Done|Completed)\s*$/im.test(head)) {
    errors.push(`${label}: completed documents must be removed or merged, not kept as Status Done`);
  }

  const dependsIndex = lines.findIndex((line) => line.startsWith("Depends On:"));
  if (dependsIndex >= 0) {
    for (let index = dependsIndex + 1; index < lines.length; index += 1) {
      const match = lines[index].match(/^-\s+(.+)$/);
      if (!match) break;
      if (/^(?:none|无)$/i.test(match[1].trim())) continue;
      const target = resolveReference(file, match[1]);
      if (target && !fs.existsSync(target)) {
        errors.push(`${label}: missing dependency ${match[1]}`);
      }
    }
  }

  for (const match of text.matchAll(/\[[^\]]*\]\(([^)]+\.md(?:#[^)]+)?)\)/gi)) {
    const target = resolveReference(file, match[1]);
    if (target && !fs.existsSync(target)) {
      errors.push(`${label}: broken Markdown link ${match[1]}`);
    }
  }
  if (text.includes("\uFFFD")) {
    errors.push(`${label}: contains Unicode replacement character`);
  }
}

for (const file of files) {
  const text = texts.get(file);
  const references = new Set();
  for (const match of text.matchAll(/`([^`\r\n]+?\.(?:md|html?))(?:#[^`\r\n]+)?`/gi)) {
    const reference = match[1];
    if (/[\\/]/.test(reference)) references.add(reference);
  }
  for (const reference of references) {
    if (/[<>{}*]/.test(reference) || /^[A-Za-z]:[\\/]/.test(reference)) continue;
    const target = resolveReference(file, reference);
    if (target && !fs.existsSync(target)) {
      errors.push(`${relative(file)}: broken document reference ${reference}`);
    }
  }
}

const governedSet = new Set(governed.map((file) => path.normalize(file)));
const dependencyGraph = new Map();
for (const file of governed) {
  const lines = fs.readFileSync(file, "utf8").split(/\r?\n/);
  const dependsIndex = lines.findIndex((line) => line.startsWith("Depends On:"));
  const targets = [];
  if (dependsIndex >= 0) {
    for (let index = dependsIndex + 1; index < lines.length; index += 1) {
      const match = lines[index].match(/^-\s+(.+)$/);
      if (!match) break;
      if (/^(?:none|无)$/i.test(match[1].trim())) continue;
      const target = resolveReference(file, match[1]);
      if (target && governedSet.has(path.normalize(target))) targets.push(path.normalize(target));
    }
  }
  dependencyGraph.set(path.normalize(file), targets);
}

const state = new Map();
function visit(file, stack) {
  if (state.get(file) === 2) return;
  if (state.get(file) === 1) {
    const start = stack.indexOf(file);
    const cycle = [...stack.slice(start), file].map((item) => relative(item)).join(" -> ");
    errors.push(`Depends On cycle: ${cycle}`);
    return;
  }
  state.set(file, 1);
  for (const target of dependencyGraph.get(file) ?? []) visit(target, [...stack, file]);
  state.set(file, 2);
}
for (const file of dependencyGraph.keys()) visit(file, []);

const budgets = new Map([
  ["AGENTS.md", 120],
  ["文档/WORKSPACE_STRUCTURE.md", 180],
  ["文档/TASK_CONTROL.md", 120],
  ["文档/WORK_CANDIDATES.md", 120],
]);
for (const placement of projectAgentPlacements) budgets.set(relative(placement.target), 120);
for (const file of formal) {
  const label = relative(file);
  if (!budgets.has(label)) budgets.set(label, 200);
}
for (const file of files.filter((item) => /\/skills\/[^/]+\/SKILL\.md$/.test(`/${relative(item)}`))) {
  budgets.set(relative(file), 120);
}
for (const file of files.filter((item) => relative(item).startsWith(".agents/skills/") && /\.mdx?$/i.test(item))) {
  const label = relative(file);
  if (!budgets.has(label)) budgets.set(label, 180);
}
for (const [file, limit] of budgets) {
  const text = texts.get(path.join(root, ...file.split("/")));
  if (!text) continue;
  const count = text.split(/\r?\n/).length;
  if (count > limit) errors.push(`${file}: ${count} lines exceeds ${limit}-line hot-path budget`);
}

for (const file of files.filter((item) => {
  const label = relative(item);
  return label === "AGENTS.md"
    || label === "README.md"
    || label.startsWith("人类-文档/")
    || label.startsWith("文档/")
    || label.startsWith(".agents/skills/");
})) {
  const text = texts.get(file);
  if (containsMachineSpecificWorkspacePath(text)) {
    errors.push(`${relative(file)}: contains a machine-specific workspace path`);
  }
  if (/Password=(?!<|\$\{|%|\$env:|__)[^;\s"'`]+/i.test(text)) {
    errors.push(`${relative(file)}: contains a literal password in a connection string`);
  }
  if (/POSTGRES_PASSWORD=(?!<|\$\{|%|\$env:)[^\s`]+/i.test(text)) {
    errors.push(`${relative(file)}: contains a literal PostgreSQL password`);
  }
}

const lineCount = [...texts.values()].reduce((sum, text) => sum + text.split(/\r?\n/).length, 0);
const charCount = [...texts.values()].reduce((sum, text) => sum + text.length, 0);

if (errors.length) {
  console.error(`Documentation check failed (${errors.length}):`);
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

console.log(`Documentation check passed: ${files.length} files, ${lineCount} lines, ${charCount} characters.`);
