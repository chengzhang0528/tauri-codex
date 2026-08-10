import assert from "node:assert/strict";
import { mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import { validateHumanDocumentation } from "./check-docs-human.mjs";

async function createFixture(t, overrides = {}) {
  const root = await mkdtemp(path.join(tmpdir(), "check-docs-human-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(path.join(root, "人类-文档", "开发"), { recursive: true });
  await writeFile(path.join(root, "package.json"), JSON.stringify({ scripts: { "known:script": "node -e 0" } }), "utf8");

  const content = {
    "README.md": "# Workspace\n\n[全部人类文档](人类-文档/README.md)\n",
    "人类-文档/README.md": "# 人类文档\n\n[执行任务](开发/执行任务.md)\n",
    "人类-文档/开发/执行任务.md": "# 执行任务\n\n`npm run known:script`\n",
    ...overrides,
  };
  for (const [relative, text] of Object.entries(content)) {
    if (text === null) continue;
    const target = path.join(root, relative);
    await mkdir(path.dirname(target), { recursive: true });
    await writeFile(target, text, "utf8");
  }
  return root;
}

async function markdownFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(entries.map(async (entry) => {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) return markdownFiles(target);
    return /\.mdx?$/i.test(entry.name) ? [target] : [];
  }));
  return nested.flat();
}

function resolveReference(source, raw) {
  const value = decodeURIComponent(raw.split("#", 1)[0].trim());
  if (!value || /^(?:https?:|mailto:|data:)/i.test(value)) return null;
  return path.resolve(path.dirname(source), value);
}

async function validate(root) {
  return validateHumanDocumentation({
    root,
    files: await markdownFiles(root),
    resolveReference,
  });
}

test("accepts a connected root entry, human entry, and task page", async (t) => {
  const root = await createFixture(t);
  assert.deepEqual(await validate(root), []);
});

test("requires the root README", async (t) => {
  const root = await createFixture(t, { "README.md": null });
  assert.ok((await validate(root)).includes("README.md: missing root human documentation entry"));
});

test("requires the complete human documentation entry", async (t) => {
  const root = await createFixture(t, { "人类-文档/README.md": null });
  assert.ok((await validate(root)).includes("人类-文档/README.md: missing human documentation entry"));
});

test("requires the root README to link the complete human entry", async (t) => {
  const root = await createFixture(t, { "README.md": "# Workspace\n" });
  assert.ok((await validate(root)).includes("README.md: root human entry must link to 人类-文档/README.md"));
});

test("requires the complete human entry to link a task page", async (t) => {
  const root = await createFixture(t, { "人类-文档/README.md": "# 人类文档\n" });
  assert.ok((await validate(root)).includes("人类-文档/README.md: human entry must link to at least one task page"));
});

test("rejects unknown root npm scripts", async (t) => {
  const root = await createFixture(t, { "人类-文档/开发/执行任务.md": "# 执行任务\n\n`npm run missing:script`\n" });
  assert.ok((await validate(root)).some((error) => error.endsWith("unknown package script missing:script")));
});

test("rejects broken human documentation links", async (t) => {
  const root = await createFixture(t, { "人类-文档/开发/执行任务.md": "# 执行任务\n\n[缺失页面](缺失页面.md)\n" });
  assert.ok((await validate(root)).some((error) => error.endsWith("broken human documentation link 缺失页面.md")));
});

test("requires task page filenames to match H1", async (t) => {
  const root = await createFixture(t, { "人类-文档/开发/执行任务.md": "# 另一个标题\n" });
  assert.ok((await validate(root)).some((error) => error.includes("filename does not match H1")));
});
