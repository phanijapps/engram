/**
 * Type surface for the TASK-020 native build/load helper (implementation in
 * build-native-recipes.mjs; this declaration keeps `tsc --noEmit` honest
 * without a build step for the helper itself).
 */

/** The recipe names this helper builds. */
export declare const RECIPES: readonly ["sqlite", "pgvector"];

type Recipe = (typeof RECIPES)[number];
type RecipePaths = Record<Recipe, string>;
type RecipeDigests = Record<Recipe, string>;
type ArtifactOptions = { artifactsRoot?: string };
type ArtifactTransaction = {
  paths: RecipePaths;
  digests: RecipeDigests;
  rebuild: () => { paths: RecipePaths; digests: RecipeDigests };
};

/** Runs a callback while the selected artifact set remains locked. */
export declare function withRecipeArtifacts<T>(
  callback: (transaction: ArtifactTransaction) => T,
  options?: ArtifactOptions,
): T;

/** Ensures both recipe artifacts exist and returns paths and in-lock digests. */
export declare function ensureRecipeArtifacts(options?: ArtifactOptions): RecipePaths & { digests: RecipeDigests };

/** Loads one recipe's addon (isolated artifact directory → isolated dlopen). */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export declare function loadRecipe(recipe: Recipe): any;
