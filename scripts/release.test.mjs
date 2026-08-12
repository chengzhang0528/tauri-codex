import assert from "node:assert/strict";
import test from "node:test";
import { nextPatchVersion, replaceVersion } from "./release.mjs";

test("increments a stable patch version", () => {
  assert.equal(nextPatchVersion("0.1.0"), "0.1.1");
  assert.equal(nextPatchVersion("1.9.9"), "1.9.10");
});

test("updates one version marker without changing surrounding text", () => {
  assert.equal(replaceVersion('{"version":"0.1.0"}', "0.1.0", "0.1.1", "fixture"), '{"version":"0.1.1"}');
  assert.throws(() => replaceVersion('{"version":"0.1.0","other":"0.1.0"}', "0.1.0", "0.1.1", "fixture"));
});
