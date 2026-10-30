/** Shared TASK-020 case battery (per recipe, one addon per file). */
export declare const scope: { tenant: string; workspace: string };
export declare const foreignScope: { tenant: string; workspace: string };
export declare function configJson(base: string, extra?: Record<string, unknown>): string;
export declare function authorizedConfig(
  base: string,
  extra?: Record<string, unknown>,
  of?: { tenant: string; workspace: string }
): string;
export declare function gitFixture(tag: string): { path: string; oid: string };
export declare function assertPgDurableBinding(repo: string): void;
export declare function describeCodeBindingCases(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  getAddon: () => any,
  recipe: string
): void;
