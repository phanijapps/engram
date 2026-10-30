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
import { copyFileSync, existsSync, mkdirSync, rmSync, utimesSync } from "node:fs";
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

function artifactPath(recipe) {
  return join(artifactsRoot, recipe, "engram_node.node");
}

function buildRecipe(recipe) {
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
  const dest = artifactPath(recipe);
  mkdirSync(dirname(dest), { recursive: true });
  copyFileSync(src, dest);
  // cargo keeps the cdylib mtime from its last link; stamp the copy AFTER the
  // sources so freshness math stays monotonic.
  utimesSync(dest, new Date(), new Date());
  console.log(`[task_020 helper] placed ${recipe} artifact: ${dest}`);
}

/** Validates and rebuilds both recipe artifacts through Cargo. Returns the
 *  recipe-specific copies after each validated feature build. */
export function ensureRecipeArtifacts() {
  const paths = {};
  for (const recipe of RECIPES) {
    buildRecipe(recipe);
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
