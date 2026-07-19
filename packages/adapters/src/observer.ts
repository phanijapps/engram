import type { AdapterEvent, AdapterObserver } from "./events.js";

/** Emits an adapter event without letting observer failures affect operations. */
export async function emitSafely(
  observer: AdapterObserver | undefined,
  event: AdapterEvent
): Promise<void> {
  try {
    await observer?.emit(event);
  } catch {
    // Adapter observers must not change Engram operation behavior.
  }
}
