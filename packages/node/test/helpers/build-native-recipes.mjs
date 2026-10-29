/**
 * TASK-020 task-owned native build/load helper (see tasks.md §Focused proof
 * ownership: "TASK-020 owns its native build/load helper in
 * packages/node/test/helpers … using existing cargo feature build interfaces
 * in isolated artifact directories before load").
 *
 * Builds BOTH recipe artifacts of the engram-node addon in isolated artifact
 * directories, copying each cargo cdylib product to
 * `packages/node/test/artifacts/<recipe>/engram_node.node`:
 *   - sqlite   : `cargo build -p engram-node --no-default-features
 *                --features http-embeddings` (the SQLite recipe — engram-node
 *                always compiles the sqlite wiring).
 *   - pgvector : the same plus `,pgvector` (routes pgvector configs to the
 *                backends/pgvector recipe).
 *
 * Freshness: an artifact is rebuilt when missing, when any Rust source under
 * `bindings/node/` (or its Cargo.toml / the workspace Cargo.lock) is newer
 * than the artifact, or when TASK020_FORCE_BUILD=1. Stale artifacts are never
 * silently reused.
 *
 * Selection honesty: tests assert the SELECTED recipe through the addon's own
 * behavior (the sqlite artifact refuses a pgvector config with the exact
 * "requires the `pgvector` feature" message; the pgvector artifact gets PAST
 * that gate), never by trusting the directory name.
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync, utimesSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
// helpers/ -> test/ -> node/ -> packages/ -> repo root
const repoRoot = join(here, "..", "..", "..", "..");
const pkgRoot = join(here, "..", "..");
const artifactsRoot = join(pkgRoot, "test", "artifacts");

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

/** Newest mtime across the binding's Rust sources + manifests (freshness
 *  fingerprint for stale-artifact rejection). */
function newestSourceMtime() {
  let newest = 0;
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(path);
      } else if (/\.(rs|toml)$/.test(entry.name) || entry.name === "Cargo.lock") {
        newest = Math.max(newest, statSync(path).mtimeMs);
      }
    }
  };
  walk(join(repoRoot, "bindings", "node"));
  newest = Math.max(newest, statSync(join(repoRoot, "Cargo.lock")).mtimeMs);
  return newest;
}

function artifactPath(recipe) {
  return join(artifactsRoot, recipe, "engram_node.node");
}

function isFresh(recipe) {
  const artifact = artifactPath(recipe);
  if (!existsSync(artifact)) return false;
  if (process.env.TASK020_FORCE_BUILD === "1") return false;
  return statSync(artifact).mtimeMs >= newestSourceMtime();
}

function buildRecipe(recipe) {
  const argv = ["build", "-p", "engram-node", ...featuresFor(recipe)];
  console.log(`[task_020 helper] cargo ${argv.join(" ")}`);
  execFileSync("cargo", argv, { stdio: "inherit", cwd: repoRoot });
  const src = join(repoRoot, "target", "debug", cdylib);
  if (!existsSync(src)) {
    throw new Error(`build artifact not found: ${src}`);
  }
  const dest = artifactPath(recipe);
  mkdirSync(dirname(dest), { recursive: true });
  copyFileSync(src, dest);
  // cargo keeps the cdylib mtime from its last link; stamp the copy AFTER the
  // sources so freshness math stays monotonic.
  utimesSync(dest, new Date(), new Date());
  console.log(`[task_020 helper] placed ${recipe} artifact: ${dest}`);
}

/** Ensures both recipe artifacts exist and are fresh (building what is
 *  missing/stale). Returns the artifact paths keyed by recipe. */
export function ensureRecipeArtifacts() {
  const paths = {};
  for (const recipe of RECIPES) {
    if (!isFresh(recipe)) {
      buildRecipe(recipe);
    }
    paths[recipe] = artifactPath(recipe);
  }
  return paths;
}

/** Loads one recipe's addon (isolated path → isolated dlopen instance). */
export function loadRecipe(recipe) {
  const paths = ensureRecipeArtifacts();
  const require = createRequire(import.meta.url);
  return require(paths[recipe]);
}
