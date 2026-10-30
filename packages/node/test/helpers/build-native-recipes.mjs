/**
 * TASK-020 task-owned native build/load helper (see tasks.md §Focused proof
 * ownership: "TASK-020 owns its native build/load helper in
 * packages/node/test/helpers … using existing cargo feature build interfaces
 * in isolated artifact directories before load").
 *
 * Builds BOTH recipe artifacts of the engram-node addon in isolated artifact
 * directories, copying each cargo cdylib product to
 * `packages/node/test/artifacts/<recipe>/engram_node.node`.
 *
 * Freshness: every invocation asks Cargo to validate the complete workspace
 * dependency graph for each recipe before copying its artifact. Cargo shares
 * `target/debug` between the feature invocations, so the helper removes the
 * shared cdylib before each recipe build — cargo then ALWAYS relinks that
 * recipe's variant (cached objects, fresh link) and the copy below is
 * guaranteed to be THIS recipe's binary. Without the removal, a no-op build
 * after the OTHER recipe's link would copy the wrong variant into this
 * recipe's slot (two dlopen copies of the same featured binary then
 * segfault on napi static-state interposition — observed at r6a-fix 57bde08).
 *
 * Selection honesty: tests assert the SELECTED recipe through the addon's own
 * behavior (the sqlite artifact refuses a pgvector config with the exact
 * "requires the `pgvector` feature" message; the pgvector artifact gets PAST
 * that gate), never by trusting the directory name. One recipe addon loads
 * per test FILE: vitest isolates files in separate processes, so the two
 * copies are never dlopen'd into one process.
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { closeSync, copyFileSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, statSync, utimesSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
// helpers/ -> test/ -> node/ -> packages/ -> repo root
const repoRoot = join(here, "..", "..", "..", "..");
const pkgRoot = join(here, "..", "..");
const sharedArtifactsRoot = join(pkgRoot, "test", "artifacts");
const LOCK_RETRY_MS = 50;
const LOCK_TIMEOUT_MS = 15 * 60 * 1000;

const artifactByPlatform = {
  linux: "libengram_node.so",
  darwin: "libengram_node.dylib",
  win32: "engram_node.dll",
};
const cdylib = artifactByPlatform[process.platform];
if (!cdylib) {
  throw new Error(`unsupported platform: ${process.platform}`);
}

/** The two recipe builds this helper owns. */
export const RECIPES = /** @type {const} */ (["sqlite", "pgvector"]);

function featuresFor(recipe) {
  switch (recipe) {
    case "sqlite":
      return ["--no-default-features", "--features", "http-embeddings"];
    case "pgvector":
      return ["--no-default-features", "--features", "http-embeddings,pgvector"];
    default:
      throw new Error(`unknown recipe: ${recipe}`);
  }
}

function artifactPath(artifactsRoot, recipe) {
  return join(artifactsRoot, recipe, "engram_node.node");
}

function artifactLockPath(artifactsRoot) {
  return join(artifactsRoot, ".build-native-recipes.lock");
}

function sleep(milliseconds) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

/** Acquires the cross-process artifact-set lock with bounded retry. */
function acquireArtifactLock(artifactsRoot) {
  mkdirSync(artifactsRoot, { recursive: true });
  const deadline = Date.now() + LOCK_TIMEOUT_MS;
  const lockPath = artifactLockPath(artifactsRoot);
  for (;;) {
    try {
      closeSync(openSync(lockPath, "wx"));
      return;
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }
      if (Date.now() >= deadline) {
        throw new Error(`timed out after ${LOCK_TIMEOUT_MS}ms waiting for recipe artifact lock`);
      }
      sleep(LOCK_RETRY_MS);
    }
  }
}

function validateArtifact(artifactsRoot, recipe) {
  const dest = artifactPath(artifactsRoot, recipe);
  if (!existsSync(dest) || statSync(dest).size === 0) {
    throw new Error(`invalid ${recipe} artifact: ${dest}`);
  }
}

function digestArtifact(artifactsRoot, recipe) {
  return createHash("sha256").update(readFileSync(artifactPath(artifactsRoot, recipe))).digest("hex");
}

function buildRecipe(artifactsRoot, recipe) {
  // Remove the SHARED cdylib first so cargo must relink THIS recipe's
  // variant even when every object is cached — the copied artifact can then
  // never alias the other recipe's last link (r6a-fix2 finding 7 repair).
  const sharedCdylib = join(repoRoot, "target", "debug", cdylib);
  if (existsSync(sharedCdylib)) {
    rmSync(sharedCdylib, { force: true });
  }
  const argv = ["build", "-p", "engram-node", ...featuresFor(recipe)];
  console.log(`[task_020 helper] cargo ${argv.join(" ")}`);
  execFileSync("cargo", argv, { stdio: "inherit", cwd: repoRoot });
  const src = join(repoRoot, "target", "debug", cdylib);
  if (!existsSync(src)) {
    throw new Error(`build artifact not found: ${src}`);
  }
  const dest = artifactPath(artifactsRoot, recipe);
  mkdirSync(dirname(dest), { recursive: true });
  copyFileSync(src, dest);
  // cargo keeps the cdylib mtime from its last link; stamp the copy AFTER the
  // sources so freshness math stays monotonic.
  utimesSync(dest, new Date(), new Date());
  validateArtifact(artifactsRoot, recipe);
  console.log(`[task_020 helper] placed ${recipe} artifact: ${dest}`);
}

/** Builds, validates, and hashes an artifact set while its lock is held. */
function rebuildRecipeArtifacts(artifactsRoot) {
  const paths = {};
  for (const recipe of RECIPES) {
    buildRecipe(artifactsRoot, recipe);
    paths[recipe] = artifactPath(artifactsRoot, recipe);
  }
  const digests = {};
  for (const recipe of RECIPES) {
    validateArtifact(artifactsRoot, recipe);
    digests[recipe] = digestArtifact(artifactsRoot, recipe);
  }
  return { paths, digests };
}

/** Runs a callback inside the artifact-set transaction. */
export function withRecipeArtifacts(callback, { artifactsRoot = sharedArtifactsRoot } = {}) {
  acquireArtifactLock(artifactsRoot);
  try {
    const artifacts = rebuildRecipeArtifacts(artifactsRoot);
    return callback({
      ...artifacts,
      rebuild: () => rebuildRecipeArtifacts(artifactsRoot),
    });
  } finally {
    rmSync(artifactLockPath(artifactsRoot), { force: true });
  }
}

/** Validates and rebuilds both recipe artifacts through Cargo. Returns paths
 * and SHA-256 digests calculated while the artifact-set lock is held. */
export function ensureRecipeArtifacts(options) {
  return withRecipeArtifacts(({ paths, digests }) => ({ ...paths, digests }), options);
}

/** Loads one recipe's addon (isolated path → isolated dlopen instance). */
export function loadRecipe(recipe) {
  const paths = ensureRecipeArtifacts();
  const require = createRequire(import.meta.url);
  return require(paths[recipe]);
}
