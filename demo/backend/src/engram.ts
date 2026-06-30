import {
  createNativeMemoryTransport,
  type NativeMemoryTransport,
} from "@engram/node";

// One Rust-backed engine is held for the process lifetime so write, retrieve,
// and forget observe the same in-memory SQLite state.
let transport: NativeMemoryTransport | null = null;

export function getTransport(): NativeMemoryTransport {
  if (transport === null) {
    transport = createNativeMemoryTransport();
  }
  return transport;
}
