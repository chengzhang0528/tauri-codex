import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

class CloseoutError extends Error {
  constructor(code, message, details = {}, exitCode = 2) {
    super(message);
    this.code = code;
    this.details = details;
    this.exitCode = exitCode;
  }
}

function parseArgs(argv) {
  const options = {
    mode: "commit-push",
    repo: ".",
    remote: "origin",
    branch: "main",
    paths: [],
    allowLinkedWorktree: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (key === "--allow-linked-worktree") {
      options.allowLinkedWorktree = true;
      continue;
    }

    const value = argv[index + 1];
    if (!value) {
      throw new CloseoutError("invalid_arguments", `Missing value for ${key}.`, {}, 1);
    }
    index += 1;

    switch (key) {
      case "--mode":
        options.mode = value;
        break;
      case "--repo":
        options.repo = value;
        break;
      case "--remote":
        options.remote = value;
        break;
      case "--branch":
        options.branch = value;
        break;
      case "--message":
        options.message = value;
        break;
      case "--verification":
        options.verification = value;
        break;
      case "--path":
        options.paths.push(value);
        break;
      default:
        throw new CloseoutError("invalid_arguments", `Unknown argument: ${key}.`, {}, 1);
    }
  }

  if (!new Set(["check", "commit-push"]).has(options.mode)) {
    throw new CloseoutError("invalid_mode", "--mode must be check or commit-push.", {}, 1);
  }
  if (!options.verification?.trim()) {
    throw new CloseoutError("verification_required", "Non-empty --verification evidence is required.");
  }
  if (options.mode === "commit-push" && !options.message?.trim()) {
    throw new CloseoutError("message_required", "A non-empty --message is required for commit-push.");
  }
  if (options.paths.length === 0) {
    throw new CloseoutError("paths_required", "At least one explicit --path is required.");
  }
  if (!options.remote.trim() || !options.branch.trim()) {
    throw new CloseoutError("invalid_target", "Remote and branch must be non-empty.", {}, 1);
  }
  if (options.remote.startsWith("-") || options.branch.startsWith("-")) {
    throw new CloseoutError("invalid_target", "Remote and branch cannot start with a dash.", {}, 1);
  }

  return options;
}

function sanitizeGitOutput(value) {
  return value
    .replace(/(https?:\/\/)[^\s/@]+(?::[^\s/@]*)?@/gi, "$1[redacted]@")
    .replace(/([?&](?:access_token|api_key|key|password|sig|signature|token)=)[^&\s]+/gi, "$1[redacted]")
    .slice(0, 2000);
}

function runGit(repo, args, { allowFailure = false } = {}) {
  const result = spawnSync("git", args, {
    cwd: repo,
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 8 * 1024 * 1024,
  });

  if (result.error) {
    throw new CloseoutError("git_unavailable", result.error.message, {}, 1);
  }
  if (result.status !== 0 && !allowFailure) {
    throw new CloseoutError("git_command_failed", `git ${args[0]} failed.`, {
      command: ["git", ...args],
      stderr: sanitizeGitOutput(result.stderr.trim()),
      stdout: sanitizeGitOutput(result.stdout.trim()),
    });
  }
  return result;
}

function gitText(repo, args) {
  return runGit(repo, args).stdout.trim();
}

function splitNull(value) {
  return value.split("\0").filter(Boolean);
}

function resolveGitPath(repo, value) {
  return path.resolve(repo, value.trim());
}

function normalizeOwnedPath(repo, value) {
  if (!value?.trim() || path.isAbsolute(value)) {
    throw new CloseoutError("unsafe_path", "Owned paths must be non-empty repository-relative file paths.", { path: value });
  }

  const normalized = value.replaceAll("\\", "/").replace(/^\.\//, "");
  const resolved = path.resolve(repo, normalized);
  const relative = path.relative(repo, resolved).replaceAll("\\", "/");
  if (!relative || relative === "." || relative.startsWith("../") || path.isAbsolute(relative)) {
    throw new CloseoutError("unsafe_path", "Owned path escapes the repository or names the repository root.", { path: value });
  }

  const lower = `/${relative.toLowerCase()}`;
  const base = path.posix.basename(lower);
  const blockedSegments = ["/node_modules/", "/dist/", "/bin/", "/obj/", "/.venv/", "/target/", "/.git/"];
  const blockedNames = new Set([".env", "id_rsa", "id_ed25519"]);
  const blockedExtensions = [".log", ".pem", ".key", ".pfx", ".p12"];
  if (
    blockedSegments.some((segment) => lower.includes(segment)) ||
    blockedNames.has(base) ||
    base.startsWith(".env.") ||
    blockedExtensions.some((extension) => base.endsWith(extension))
  ) {
    throw new CloseoutError("unsafe_path", "Owned path matches a blocked secret or generated-output class.", { path: relative });
  }

  if (fs.existsSync(resolved) && fs.statSync(resolved).isDirectory()) {
    throw new CloseoutError("directory_path_rejected", "Pass changed files individually; directories are not accepted.", { path: relative });
  }

  const tracked = runGit(repo, ["ls-files", "--error-unmatch", "--", relative], { allowFailure: true }).status === 0;
  if (!fs.existsSync(resolved) && !tracked) {
    throw new CloseoutError("path_not_found", "Owned path is neither an existing file nor a tracked deletion.", { path: relative });
  }
  return relative;
}

function assertCheckout(repo, options) {
  const prefix = gitText(repo, ["rev-parse", "--show-prefix"]);
  if (prefix) {
    throw new CloseoutError("repo_not_top_level", "--repo must name the current Git top-level directory.", { prefix });
  }

  const remotes = gitText(repo, ["remote"]).split(/\r?\n/).filter(Boolean);
  if (!remotes.includes(options.remote)) {
    throw new CloseoutError("remote_not_found", `Remote ${options.remote} does not exist.`, { remotes });
  }
  const branchCheck = runGit(repo, ["check-ref-format", "--branch", options.branch], { allowFailure: true });
  if (branchCheck.status !== 0) {
    throw new CloseoutError("invalid_target", `Branch ${options.branch} is not a valid branch name.`, {}, 1);
  }

  const branch = gitText(repo, ["branch", "--show-current"]);
  if (branch !== options.branch) {
    throw new CloseoutError("branch_not_allowed", `Current branch is ${branch || "detached HEAD"}; expected ${options.branch}.`, {
      currentBranch: branch,
      expectedBranch: options.branch,
    });
  }

  const gitDir = resolveGitPath(repo, gitText(repo, ["rev-parse", "--git-dir"]));
  const commonDir = resolveGitPath(repo, gitText(repo, ["rev-parse", "--git-common-dir"]));
  if (!options.allowLinkedWorktree && path.normalize(gitDir) !== path.normalize(commonDir)) {
    throw new CloseoutError("linked_worktree_not_allowed", "The current checkout is a linked worktree.");
  }
}

function assertCleanIndex(repo) {
  const staged = splitNull(runGit(repo, ["diff", "--cached", "--name-only", "-z"]).stdout);
  if (staged.length > 0) {
    throw new CloseoutError("preexisting_staged_changes", "The index already contains staged changes.", { staged });
  }
}

function selectedChanges(repo, ownedPaths) {
  const unstaged = splitNull(runGit(repo, ["diff", "--name-only", "-z", "--", ...ownedPaths]).stdout);
  const untracked = splitNull(runGit(repo, ["ls-files", "--others", "--exclude-standard", "-z", "--", ...ownedPaths]).stdout);
  return [...new Set([...unstaged, ...untracked])].sort();
}

function hasLocalHead(repo) {
  return runGit(repo, ["rev-parse", "--verify", "HEAD"], { allowFailure: true }).status === 0;
}

function remoteHeads(repo, remote) {
  const output = gitText(repo, ["ls-remote", "--heads", remote]);
  return output.split(/\r?\n/).filter(Boolean).map((line) => line.split(/\s+/)[1]).filter(Boolean);
}

function syncCounts(repo, remote, branch) {
  runGit(repo, ["fetch", "--no-tags", remote, branch]);
  const output = gitText(repo, ["rev-list", "--left-right", "--count", `HEAD...${remote}/${branch}`]);
  const [ahead, behind] = output.split(/\s+/).map(Number);
  if (!Number.isInteger(ahead) || !Number.isInteger(behind)) {
    throw new CloseoutError("sync_state_unreadable", "Could not read local/remote synchronization state.", { output });
  }
  return { ahead, behind };
}

function assertInitiallySynchronized(repo, options) {
  const heads = remoteHeads(repo, options.remote);
  const expectedRef = `refs/heads/${options.branch}`;
  if (!heads.includes(expectedRef)) {
    if (!hasLocalHead(repo) && heads.length === 0) {
      return { initializeRemote: true };
    }
    throw new CloseoutError("remote_branch_not_found", `Remote ${options.remote}/${options.branch} does not exist.`, {
      remoteHeads: heads,
    });
  }
  if (!hasLocalHead(repo)) {
    throw new CloseoutError("history_not_synchronized", "Local and remote history must match before closeout.", {
      ahead: 0,
      behind: 1,
    });
  }
  const counts = syncCounts(repo, options.remote, options.branch);
  if (counts.ahead !== 0 || counts.behind !== 0) {
    throw new CloseoutError("history_not_synchronized", "Local and remote history must match before closeout.", counts);
  }
  return { initializeRemote: false };
}

function unstageOwned(repo, ownedPaths) {
  if (hasLocalHead(repo)) {
    runGit(repo, ["restore", "--staged", "--", ...ownedPaths], { allowFailure: true });
  } else {
    runGit(repo, ["rm", "--cached", "--ignore-unmatch", "--", ...ownedPaths], { allowFailure: true });
  }
}

function commitAndPush(repo, options, ownedPaths, changedFiles, syncState) {
  runGit(repo, ["add", "--", ...ownedPaths]);
  const staged = splitNull(runGit(repo, ["diff", "--cached", "--name-only", "-z"]).stdout).sort();
  if (staged.length === 0 || staged.some((file) => !ownedPaths.includes(file))) {
    unstageOwned(repo, ownedPaths);
    throw new CloseoutError("staged_scope_mismatch", "Staged files do not exactly fit the declared owned path set.", {
      ownedPaths,
      staged,
    });
  }

  const commit = runGit(repo, ["commit", "-m", options.message], { allowFailure: true });
  if (commit.status !== 0) {
    unstageOwned(repo, ownedPaths);
    throw new CloseoutError("commit_failed", "Git commit failed; owned paths were unstaged.", {
      stderr: sanitizeGitOutput(commit.stderr.trim()),
      stdout: sanitizeGitOutput(commit.stdout.trim()),
    });
  }

  const sha = gitText(repo, ["rev-parse", "HEAD"]);
  try {
    assertCheckout(repo, options);
    const currentHead = gitText(repo, ["rev-parse", "HEAD"]);
    if (syncState.initializeRemote) {
      const heads = remoteHeads(repo, options.remote);
      if (currentHead !== sha || heads.length !== 0) {
        throw new CloseoutError("remote_changed_after_commit", "The empty remote changed after the local commit; push was not attempted.", {
          sha,
          currentHead,
          remoteHeads: heads,
        }, 3);
      }
    } else {
      const counts = syncCounts(repo, options.remote, options.branch);
      if (currentHead !== sha || counts.ahead !== 1 || counts.behind !== 0) {
        throw new CloseoutError("remote_changed_after_commit", "History changed after the local commit; push was not attempted.", {
          sha,
          currentHead,
          ...counts,
        }, 3);
      }
    }

    const push = runGit(repo, ["push", options.remote, `HEAD:${options.branch}`], { allowFailure: true });
    if (push.status !== 0) {
      throw new CloseoutError("push_failed", "The local commit exists but push failed.", {
        sha,
        stderr: sanitizeGitOutput(push.stderr.trim()),
        stdout: sanitizeGitOutput(push.stdout.trim()),
      }, 3);
    }

    runGit(repo, ["fetch", "--no-tags", options.remote, options.branch]);
    const remoteHead = gitText(repo, ["rev-parse", `${options.remote}/${options.branch}`]);
    if (remoteHead !== sha) {
      throw new CloseoutError("push_not_verified", "Push returned successfully but the remote-tracking branch does not match the commit.", {
        sha,
        remoteHead,
      }, 3);
    }
    return { status: "pushed", sha, changedFiles, remote: options.remote, branch: options.branch };
  } catch (error) {
    if (error instanceof CloseoutError) {
      error.exitCode = 3;
      error.details = { sha, ...error.details };
    }
    throw error;
  }
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const repo = fs.realpathSync(path.resolve(options.repo));
  assertCheckout(repo, options);
  assertCleanIndex(repo);

  const ownedPaths = [...new Set(options.paths.map((value) => normalizeOwnedPath(repo, value)))].sort();
  const changedFiles = selectedChanges(repo, ownedPaths);
  if (changedFiles.length === 0) {
    return { status: "skip_no_changes", ownedPaths };
  }

  const syncState = assertInitiallySynchronized(repo, options);
  if (options.mode === "check") {
    return {
      status: "ready",
      changedFiles,
      remote: options.remote,
      branch: options.branch,
      initializeRemote: syncState.initializeRemote,
      verification: options.verification.trim(),
    };
  }

  return commitAndPush(repo, options, ownedPaths, changedFiles, syncState);
}

try {
  const result = main();
  process.stdout.write(`${JSON.stringify(result)}\n`);
} catch (error) {
  const controlled = error instanceof CloseoutError
    ? error
    : new CloseoutError("unexpected_error", error instanceof Error ? error.message : String(error), {}, 1);
  process.stdout.write(`${JSON.stringify({
    status: "stopped",
    code: controlled.code,
    message: controlled.message,
    ...controlled.details,
  })}\n`);
  process.exitCode = controlled.exitCode;
}
