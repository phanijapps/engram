/**
 * Type surface for the TASK-020 native build/load helper (implementation in
 * build-native-recipes.mjs; this declaration keeps `tsc --noEmit` honest
 * without a build step for the helper itself).
 */

/** The recipe names this helper builds. */
export declare const RECIPES: readonly ["sqlite", "pgvector"];

/** Ensures both recipe artifacts exist and are fresh; returns their paths. */
export declare function ensureRecipeArtifacts(): Record<"sqlite" | "pgvector", string>;

/** Loads one recipe's addon (isolated artifact directory → isolated dlopen). */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export declare function loadRecipe(recipe: "sqlite" | "pgvector"): any;
