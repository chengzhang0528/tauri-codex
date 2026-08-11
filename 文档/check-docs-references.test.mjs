import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import {
  containsMachineSpecificWorkspacePath,
  resolveDocumentReference,
} from "./check-docs-references.mjs";

test("distinguishes Windows workspace paths from HTTPS links", () => {
  assert.equal(containsMachineSpecificWorkspacePath("Use D:\\github.com\\project"), true);
  assert.equal(containsMachineSpecificWorkspacePath("See https://github.com/org/project"), false);
});

async function createFixture(t) {
  const root = await mkdtemp(path.join(tmpdir(), "check-docs-references-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const docsRoot = path.join(root, "文档");
  const projectRoot = path.join(docsRoot, "项目", "项目_demo");
  const source = path.join(projectRoot, "AGENTS.md");
  await mkdir(path.join(root, "app", "src"), { recursive: true });
  await mkdir(path.join(root, ".agents", "skills", "example"), { recursive: true });
  await mkdir(path.join(projectRoot, "技术设计"), { recursive: true });
  await mkdir(path.join(docsRoot, "工作流"), { recursive: true });
  await writeFile(source, "# demo\n", "utf8");
  await writeFile(path.join(root, "app", "src", "main.ts"), "export {};\n", "utf8");
  await writeFile(path.join(root, ".agents", "skills", "example", "SKILL.md"), "# example\n", "utf8");
  await writeFile(path.join(projectRoot, "技术设计", "DOC-0001.md"), "# design\n", "utf8");
  await writeFile(path.join(docsRoot, "工作流", "WF-0001.md"), "# workflow\n", "utf8");
  return { root, docsRoot, source, projectRoot };
}

test("resolves an existing arbitrary source root from workspace documents", async (t) => {
  const fixture = await createFixture(t);
  assert.equal(
    resolveDocumentReference({ ...fixture, raw: "app/src/main.ts" }),
    path.join(fixture.root, "app", "src", "main.ts"),
  );
});

test("prefers an existing document-local reference", async (t) => {
  const fixture = await createFixture(t);
  assert.equal(
    resolveDocumentReference({ ...fixture, raw: "技术设计/DOC-0001.md" }),
    path.join(fixture.projectRoot, "技术设计", "DOC-0001.md"),
  );
});

test("resolves governed document roots without product-specific names", async (t) => {
  const fixture = await createFixture(t);
  assert.equal(
    resolveDocumentReference({ ...fixture, raw: "工作流/WF-0001.md" }),
    path.join(fixture.docsRoot, "工作流", "WF-0001.md"),
  );
  assert.equal(
    resolveDocumentReference({ ...fixture, raw: "文档/TASK_CONTROL.md" }),
    path.join(fixture.docsRoot, "TASK_CONTROL.md"),
  );
  assert.equal(
    resolveDocumentReference({ ...fixture, raw: ".agents/skills/example/SKILL.md" }),
    path.join(fixture.root, ".agents", "skills", "example", "SKILL.md"),
  );
});
