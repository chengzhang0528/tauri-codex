import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const runner = path.join(path.dirname(fileURLToPath(import.meta.url)), "run-git-closeout.mjs");

function git(cwd, args) {
  const result = spawnSync("git", args, { cwd, encoding: "utf8", windowsHide: true });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result.stdout.trim();
}

function write(file, contents) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, contents, "utf8");
}

function createRepository() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "git-closeout-"));
  const remote = path.join(root, "remote.git");
  const repo = path.join(root, "repo");
  fs.mkdirSync(repo);
  git(root, ["init", "--bare", remote]);
  git(repo, ["init", "-b", "main"]);
  git(repo, ["config", "user.name", "Git Closeout Test"]);
  git(repo, ["config", "user.email", "git-closeout@example.invalid"]);
  write(path.join(repo, "owned.txt"), "base\n");
  write(path.join(repo, "other.txt"), "base\n");
  git(repo, ["add", "owned.txt", "other.txt"]);
  git(repo, ["commit", "-m", "initial"]);
  git(repo, ["remote", "add", "origin", remote]);
  git(repo, ["push", "-u", "origin", "main"]);
  return { root, remote, repo };
}

function createEmptyRepository({ configureIdentity = true } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "git-closeout-empty-"));
  const remote = path.join(root, "remote.git");
  const repo = path.join(root, "repo");
  fs.mkdirSync(repo);
  git(root, ["init", "--bare", remote]);
  git(repo, ["init", "-b", "main"]);
  if (configureIdentity) {
    git(repo, ["config", "user.name", "Git Closeout Test"]);
    git(repo, ["config", "user.email", "git-closeout@example.invalid"]);
  }
  git(repo, ["remote", "add", "origin", remote]);
  write(path.join(repo, "owned.txt"), "initial content\n");
  return { root, remote, repo };
}

function run(repo, extra = []) {
  const result = spawnSync(process.execPath, [
    runner,
    "--repo", repo,
    "--verification", "focused tests passed",
    ...extra,
  ], { encoding: "utf8", windowsHide: true });
  const output = result.stdout.trim();
  return { ...result, json: output ? JSON.parse(output) : null };
}

test("commits and pushes only owned files while preserving unrelated unstaged changes", (t) => {
  const fixture = createRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));
  write(path.join(fixture.repo, "owned.txt"), "owned change\n");
  write(path.join(fixture.repo, "other.txt"), "unrelated change\n");

  const result = run(fixture.repo, [
    "--mode", "commit-push",
    "--message", "test: exact closeout",
    "--path", "owned.txt",
  ]);

  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.equal(result.json.status, "pushed");
  assert.deepEqual(git(fixture.repo, ["show", "--pretty=", "--name-only", "HEAD"]).split(/\r?\n/).filter(Boolean), ["owned.txt"]);
  assert.equal(git(fixture.repo, ["status", "--short"]), "M other.txt");
  assert.equal(git(fixture.repo, ["rev-parse", "HEAD"]), git(fixture.repo, ["rev-parse", "origin/main"]));
});

test("initializes an entirely empty origin main with one exact commit", (t) => {
  const fixture = createEmptyRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));

  const result = run(fixture.repo, [
    "--mode", "commit-push",
    "--message", "test: initialize empty remote",
    "--path", "owned.txt",
  ]);

  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.equal(result.json.status, "pushed");
  assert.equal(git(fixture.repo, ["rev-parse", "HEAD"]), git(fixture.repo, ["rev-parse", "origin/main"]));
  assert.equal(git(fixture.repo, ["status", "--short"]), "");
});

test("rejects remote initialization when another remote branch already exists", (t) => {
  const fixture = createEmptyRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));
  const peer = path.join(fixture.root, "peer");
  fs.mkdirSync(peer);
  git(peer, ["init", "-b", "other"]);
  git(peer, ["config", "user.name", "Peer"]);
  git(peer, ["config", "user.email", "peer@example.invalid"]);
  write(path.join(peer, "peer.txt"), "remote content\n");
  git(peer, ["add", "peer.txt"]);
  git(peer, ["commit", "-m", "peer initial"]);
  git(peer, ["remote", "add", "origin", fixture.remote]);
  git(peer, ["push", "origin", "other"]);

  const result = run(fixture.repo, [
    "--mode", "commit-push",
    "--message", "test: blocked initialization",
    "--path", "owned.txt",
  ]);

  assert.equal(result.status, 2);
  assert.equal(result.json.code, "remote_branch_not_found");
  assert.equal(git(fixture.repo, ["status", "--short"]), "?? owned.txt");
});

test("unstages an unborn repository when the initial commit fails", (t) => {
  const fixture = createEmptyRepository({ configureIdentity: false });
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));

  const result = run(fixture.repo, [
    "--mode", "commit-push",
    "--message", "test: missing identity",
    "--path", "owned.txt",
  ]);

  assert.equal(result.status, 2);
  assert.equal(result.json.code, "commit_failed");
  assert.equal(git(fixture.repo, ["diff", "--cached", "--name-only"]), "");
  assert.equal(git(fixture.repo, ["status", "--short"]), "?? owned.txt");
});

test("returns skip_no_changes without creating a commit", (t) => {
  const fixture = createRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));
  const before = git(fixture.repo, ["rev-parse", "HEAD"]);

  const result = run(fixture.repo, ["--mode", "commit-push", "--message", "test: no-op", "--path", "owned.txt"]);

  assert.equal(result.status, 0);
  assert.equal(result.json.status, "skip_no_changes");
  assert.equal(git(fixture.repo, ["rev-parse", "HEAD"]), before);
});

test("rejects missing verification evidence", (t) => {
  const fixture = createRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));
  write(path.join(fixture.repo, "owned.txt"), "changed\n");

  const result = spawnSync(process.execPath, [runner, "--repo", fixture.repo, "--path", "owned.txt", "--message", "test: blocked"], {
    encoding: "utf8",
    windowsHide: true,
  });

  assert.equal(result.status, 2);
  assert.equal(JSON.parse(result.stdout).code, "verification_required");
  assert.equal(git(fixture.repo, ["status", "--short"]), "M owned.txt");
});

test("rejects any pre-existing staged content", (t) => {
  const fixture = createRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));
  write(path.join(fixture.repo, "owned.txt"), "owned change\n");
  write(path.join(fixture.repo, "other.txt"), "staged elsewhere\n");
  git(fixture.repo, ["add", "other.txt"]);

  const result = run(fixture.repo, ["--mode", "commit-push", "--message", "test: blocked", "--path", "owned.txt"]);

  assert.equal(result.status, 2);
  assert.equal(result.json.code, "preexisting_staged_changes");
  assert.equal(git(fixture.repo, ["diff", "--cached", "--name-only"]), "other.txt");
});

test("rejects a non-default branch without explicit authorization", (t) => {
  const fixture = createRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));
  git(fixture.repo, ["switch", "-c", "topic"]);
  write(path.join(fixture.repo, "owned.txt"), "changed\n");

  const result = run(fixture.repo, ["--mode", "commit-push", "--message", "test: blocked", "--path", "owned.txt"]);

  assert.equal(result.status, 2);
  assert.equal(result.json.code, "branch_not_allowed");
  assert.equal(git(fixture.repo, ["log", "-1", "--pretty=%s"]), "initial");
});

test("rejects remote-ahead history before staging", (t) => {
  const fixture = createRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));
  const peer = path.join(fixture.root, "peer");
  git(fixture.root, ["clone", "-b", "main", fixture.remote, peer]);
  git(peer, ["config", "user.name", "Peer"]);
  git(peer, ["config", "user.email", "peer@example.invalid"]);
  write(path.join(peer, "peer.txt"), "remote change\n");
  git(peer, ["add", "peer.txt"]);
  git(peer, ["commit", "-m", "peer change"]);
  git(peer, ["push", "origin", "main"]);
  write(path.join(fixture.repo, "owned.txt"), "local change\n");

  const result = run(fixture.repo, ["--mode", "commit-push", "--message", "test: blocked", "--path", "owned.txt"]);

  assert.equal(result.status, 2);
  assert.equal(result.json.code, "history_not_synchronized");
  assert.equal(result.json.behind, 1);
  assert.equal(git(fixture.repo, ["diff", "--cached", "--name-only"]), "");
});

test("rejects directories and common secret paths", (t) => {
  const fixture = createRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));
  write(path.join(fixture.repo, ".env"), "TOKEN=secret\n");

  const directory = run(fixture.repo, ["--mode", "check", "--path", "."]);
  const secret = run(fixture.repo, ["--mode", "check", "--path", ".env"]);

  assert.equal(directory.status, 2);
  assert.equal(directory.json.code, "unsafe_path");
  assert.equal(secret.status, 2);
  assert.equal(secret.json.code, "unsafe_path");
});

test("redacts credentials and token query values from Git failures", (t) => {
  const fixture = createRepository();
  t.after(() => fs.rmSync(fixture.root, { recursive: true, force: true }));
  write(path.join(fixture.repo, "owned.txt"), "changed\n");
  git(fixture.repo, ["remote", "set-url", "origin", "https://user:secret@127.0.0.1:1/repo.git?token=dummy-token"]);

  const result = run(fixture.repo, ["--mode", "check", "--path", "owned.txt"]);

  assert.equal(result.status, 2);
  assert.equal(result.json.code, "git_command_failed");
  assert.doesNotMatch(result.stdout, /secret|dummy-token/);
  assert.match(result.stdout, /\[redacted\]/);
});
