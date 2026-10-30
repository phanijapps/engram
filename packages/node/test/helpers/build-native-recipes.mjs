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
import { closeSync, constants, copyFileSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, statSync, utimesSync } from "node:fs";
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

function sharedTargetLockPath() {
  return join(repoRoot, "target", ".engram-node-recipe-build.lock");
}

function sleep(milliseconds) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

/** Acquires the one lock that protects every shared-target build pipeline. */
function acquireSharedTargetLock() {
  const lockPath = sharedTargetLockPath();
  mkdirSync(dirname(lockPath), { recursive: true });
  const deadline = Date.now() + LOCK_TIMEOUT_MS;
  for (;;) {
    try {
      closeSync(openSync(lockPath, "wx"));
      return;
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }
      if (Date.now() >= deadline) {
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
  acquireSharedTargetLock();
  try {
    const artifacts = rebuildRecipeArtifacts(artifactsRoot);
    return callback({
      ...artifacts,
      rebuild: () => rebuildRecipeArtifacts(artifactsRoot),
    });
  } finally {
    rmSync(sharedTargetLockPath(), { force: true });
  }
}

/** Validates and rebuilds both recipe artifacts through Cargo. Returns paths
 * and SHA-256 digests calculated while the shared target lock is held. */
export function ensureRecipeArtifacts(options) {
  return withRecipeArtifacts(({ paths, digests }) => ({ ...paths, digests }), options);
}

/** Loads one recipe's addon from the selected immutable artifact path. */
export function loadRecipe(recipe) {
  const require = createRequire(import.meta.url);
  return withRecipeArtifacts(({ paths, digests, rebuild }) => {
    const artifactFor = (artifacts, selectedRecipe) => ({
      recipe: selectedRecipe,
      path: artifacts.paths[selectedRecipe],
      digest: artifacts.digests[selectedRecipe],
    });
    const loaded = resolveRecipeArtifact(recipe, digests[recipe], {
      inspect: (selectedRecipe) => ({
        recipe: selectedRecipe,
        path: paths[selectedRecipe],
        digest: digestArtifact(paths[selectedRecipe]),
      }),
      rebuild: (selectedRecipe) => artifactFor(rebuild(), selectedRecipe),
      load: (artifact) => ({ ...artifact, addon: require(artifact.path) }),
    });
    return loaded.addon;
  });
}
