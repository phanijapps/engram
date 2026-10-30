/**
 * TASK-020 task-owned native build/load helper (see tasks.md §Focused proof
 * ownership: "TASK-020 owns its native build/load helper in
 * packages/node/test/helpers … using existing cargo feature build interfaces
 * in isolated artifact directories before load").
 *
 * Builds BOTH recipe artifacts of the engram-node addon in artifact
 * directories. Every published artifact has a fresh versioned filename that
 * includes its content digest. Published files are immutable and are retained;
 * the helper never replaces or removes a published artifact. A consumer can
 * therefore keep mapping its loaded inode while later transactions build and
 * publish newer artifacts.
 *
 * Freshness: every invocation asks Cargo to validate the complete workspace
 * dependency graph for each recipe before publishing its artifact. Cargo uses
 * one shared `target/debug` directory, so ONE repository-wide lock covers the
 * delete, build, and copy pipeline for every artifact root. This serializes an
 * isolated-root transaction with a shared-root transaction. It prevents either
 * transaction from deleting or copying the shared cdylib while the other uses
 * it. Artifact roots isolate destinations only; they do not isolate Cargo.
 *
 * Selection honesty: tests assert the SELECTED recipe through the addon's own
 * behavior (the sqlite artifact refuses a pgvector config with the exact
 * "requires the `pgvector` feature" message; the pgvector artifact gets PAST
 * that gate), never by trusting the directory name. One recipe addon loads
 * per test FILE: vitest isolates files in separate processes, so the two
 * copies are never dlopen'd into one process.
 */
import { execFileSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { closeSync, constants, copyFileSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, rmSync, statSync, utimesSync, writeSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
// helpers/ -> test/ -> node/ -> packages/ -> repo root
const repoRoot = join(here, "..", "..", "..", "..");
const pkgRoot = join(here, "..", "..");
const sharedArtifactsRoot = join(pkgRoot, "test", "artifacts");
const LOCK_RETRY_MS = 50;
const LOCK_TIMEOUT_MS = 30 * 60 * 1000;

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

function sharedTargetLockPath() {
  return join(repoRoot, "target", ".engram-node-recipe-build.lock");
}

function sleep(milliseconds) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

/** Returns whether the recorded lock holder has exited. */
function holderHasExited(lockPath) {
  const pid = Number.parseInt(readFileSync(lockPath, "utf8"), 10);
  if (!Number.isSafeInteger(pid) || pid <= 0) {
    return false;
  }
  try {
    process.kill(pid, 0);
    return false;
  } catch (error) {
    return error?.code === "ESRCH";
  }
}

/** Acquires the one lock that protects every shared-target build pipeline. */
function acquireSharedTargetLock() {
  const lockPath = sharedTargetLockPath();
  mkdirSync(dirname(lockPath), { recursive: true });
  let deadline = Date.now() + LOCK_TIMEOUT_MS;
  let reclaimed = false;
  for (;;) {
    try {
      const descriptor = openSync(lockPath, "wx");
      try {
        writeSync(descriptor, `${process.pid}\n`);
      } finally {
        closeSync(descriptor);
      }
      return () => rmSync(lockPath, { force: true });
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }
      if (Date.now() >= deadline) {
        // A waiter never steals a live build. It may reclaim only after its
        // wait window when the lockfile PID is conclusively absent (ESRCH).
        if (!reclaimed && holderHasExited(lockPath)) {
          rmSync(lockPath, { force: true });
          reclaimed = true;
          deadline = Date.now() + LOCK_TIMEOUT_MS;
          continue;
        }
        throw new Error(`timed out after ${LOCK_TIMEOUT_MS}ms waiting for shared recipe build lock`);
      }
      sleep(LOCK_RETRY_MS);
    }
  }
}

function validateArtifact(path, recipe) {
  if (!existsSync(path) || statSync(path).size === 0) {
    throw new Error(`invalid ${recipe} artifact: ${path}`);
  }
}

function digestArtifact(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/** Publishes a fresh immutable copy without replacing any published inode. */
function publishArtifact(artifactsRoot, recipe, src) {
  const digest = digestArtifact(src);
  const destinationDirectory = join(artifactsRoot, recipe);
  mkdirSync(destinationDirectory, { recursive: true });
  const dest = join(destinationDirectory, `engram_node.${recipe}.${digest.slice(0, 16)}.${randomUUID()}.node`);
  copyFileSync(src, dest, constants.COPYFILE_EXCL);
  // cargo keeps the cdylib mtime from its last link; stamp the new copy AFTER
  // the sources so freshness math stays monotonic without changing its bytes.
  utimesSync(dest, new Date(), new Date());
  validateArtifact(dest, recipe);
  if (digestArtifact(dest) !== digest) {
    throw new Error(`published ${recipe} artifact digest changed: ${dest}`);
  }
  return { path: dest, digest };
}

function buildRecipe(artifactsRoot, recipe) {
  // The repository-wide lock acquired by withRecipeArtifacts covers this
  // deletion, Cargo invocation, and publish step for every destination root.
  const sharedCdylib = join(repoRoot, "target", "debug", cdylib);
  if (existsSync(sharedCdylib)) {
    rmSync(sharedCdylib, { force: true });
  }
  const argv = ["build", "-p", "engram-node", ...featuresFor(recipe)];
  console.log(`[task_020 helper] cargo ${argv.join(" ")}`);
  execFileSync("cargo", argv, { stdio: "inherit", cwd: repoRoot });
  if (!existsSync(sharedCdylib)) {
    throw new Error(`build artifact not found: ${sharedCdylib}`);
  }
  const artifact = publishArtifact(artifactsRoot, recipe, sharedCdylib);
  console.log(`[task_020 helper] published ${recipe} artifact: ${artifact.path}`);
  return artifact;
}

/** Builds, validates, and hashes an artifact set while the shared lock is held. */
function rebuildRecipeArtifacts(artifactsRoot) {
  const paths = {};
  const digests = {};
  for (const recipe of RECIPES) {
    const artifact = buildRecipe(artifactsRoot, recipe);
    paths[recipe] = artifact.path;
    digests[recipe] = artifact.digest;
  }
  return { paths, digests };
}

/**
 * Selects the requested immutable artifact by content and verifies that the
 * loader reports that same recipe and digest.  `loadRecipe` uses this exact
 * path; injected seams keep the controls Docker-free.
 */
export function resolveRecipeArtifact(recipe, expectedDigest, { inspect, rebuild, load }) {
  let artifact = inspect(recipe);
  if (artifact.recipe !== recipe || artifact.digest !== expectedDigest) {
    artifact = rebuild(recipe);
  }
  if (artifact.recipe !== recipe || artifact.digest !== expectedDigest) {
    throw new Error("native recipe rebuild did not produce the selected identity");
  }
  const loaded = load(artifact);
  if (loaded.recipe !== recipe || loaded.digest !== expectedDigest) {
    throw new Error("loaded native recipe identity does not match selection");
  }
  return loaded;
}

/** Runs a callback inside the shared-target build transaction. */
export function withRecipeArtifacts(callback, { artifactsRoot = sharedArtifactsRoot } = {}) {
  const release = acquireSharedTargetLock();
  try {
    const artifacts = rebuildRecipeArtifacts(artifactsRoot);
    return callback({
      ...artifacts,
      rebuild: () => rebuildRecipeArtifacts(artifactsRoot),
    });
  } finally {
    release();
  }
}

/** Validates and rebuilds both recipe artifacts through Cargo. Returns paths
 * and SHA-256 digests calculated while the shared target lock is held. */
export function ensureRecipeArtifacts(options) {
  return withRecipeArtifacts(({ paths, digests }) => ({ ...paths, digests }), options);
}

/** Returns the newest valid immutable artifact that the warm build published. */
function publishedRecipeArtifact(recipe, artifactsRoot = sharedArtifactsRoot) {
  const directory = join(artifactsRoot, recipe);
  if (!existsSync(directory)) {
    return undefined;
  }
  const candidates = readdirSync(directory)
    .filter((entry) => entry.endsWith(".node"))
    .map((entry) => join(directory, entry))
    .sort((left, right) => statSync(right).mtimeMs - statSync(left).mtimeMs);
  for (const path of candidates) {
    try {
      validateArtifact(path, recipe);
      const digest = digestArtifact(path);
      if (path.includes(`.${digest.slice(0, 16)}.`)) {
        return { path, digest };
      }
    } catch {
      // A concurrent publisher may leave an incomplete candidate visible.
    }
  }
  return undefined;
}

/** Loads the warm-build artifact, or builds one when no valid artifact exists. */
export function loadRecipe(recipe) {
  const require = createRequire(import.meta.url);
  const artifact = publishedRecipeArtifact(recipe);
  if (artifact) {
    return require(artifact.path);
  }
  return withRecipeArtifacts(({ paths }) => require(paths[recipe]));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { digests } = ensureRecipeArtifacts();
  for (const recipe of RECIPES) {
    console.log(`[task_020 helper] built ${recipe} artifact digest: ${digests[recipe]}`);
  }
}
