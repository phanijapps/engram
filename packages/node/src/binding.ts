import { createRequire } from "node:module";

/** Native class shape exported by the Rust Node-API addon. */
export interface NativeMemoryEngineBinding {
  writeMemoryJson(requestJson: string): string;
  retrieveJson(requestJson: string): string;
  forgetJson(requestJson: string): string;
}

/** Constructor shape for the Rust-backed local memory engine. */
export interface NativeMemoryEngineConstructor {
  new (): NativeMemoryEngineBinding;
}

/** Native addon surface consumed by `@engram/node`. */
export interface NativeBinding {
  NativeMemoryEngine: NativeMemoryEngineConstructor;
}

/** Function used to load a native addon, injectable for deterministic tests. */
export type NativeBindingLoader = () => NativeBinding;

/** Loads the compiled Engram Node-API addon from known package locations. */
export function loadNativeBinding(loader: NativeBindingLoader = loadCompiledBinding): NativeBinding {
  return loader();
}

function loadCompiledBinding(): NativeBinding {
  const require = createRequire(import.meta.url);
  const candidates = [
    "../engram_node.node",
    "../engram-node.node",
    "../index.node",
    "../../engram_node.node",
    "../../engram-node.node"
  ];

  for (const candidate of candidates) {
    try {
      return require(candidate) as NativeBinding;
    } catch (error) {
      if (!isModuleNotFound(error)) {
        throw error;
      }
    }
  }

  throw new Error(
    "Unable to load @engram/node native addon. Build crates/engram-node and place the .node artifact in the package root."
  );
}

function isModuleNotFound(error: unknown): boolean {
  return (
    error instanceof Error &&
    "code" in error &&
    (error as NodeJS.ErrnoException).code === "MODULE_NOT_FOUND"
  );
}
