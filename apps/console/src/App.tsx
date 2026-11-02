import { useEffect, useState } from "react";
import { fetchInventory, type InventoryResponse } from "./api";
import { AliasHeader, InventoryView } from "./views/Inventory";

/**
 * Console shell (job 012 STEP-007 scaffold): the org/project alias header
 * (DEC-002) and the view switch. Views land with their owning tasks; the
 * scaffold ships the shell plus the live inventory fetch so the zero-CLI
 * journey has a pulse from the first start.
 */
export function App() {
  const [inventory, setInventory] = useState<InventoryResponse | null>(null);
  const [view, setView] = useState<"inventory" | "partition">("inventory");

  useEffect(() => {
    let cancelled = false;
    fetchInventory()
      .then((response) => {
        if (!cancelled) setInventory(response);
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setInventory({ state: "engine-error", partitions: [], message: String(error) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100">
      <header className="border-b border-slate-800 px-6 py-4">
        <h1 className="text-lg font-semibold tracking-tight">engram console</h1>
        <AliasHeader />
      </header>
      <nav className="flex gap-1 px-6 pt-4 text-sm">
        <button
          className={`rounded-t px-3 py-1.5 ${view === "inventory" ? "bg-slate-800" : "hover:bg-slate-900"}`}
          onClick={() => setView("inventory")}
        >
          Inventory
        </button>
      </nav>
      <main className="px-6 py-4">
        {view === "inventory" ? (
          <InventoryView inventory={inventory} onSelect={() => setView("partition")} />
        ) : (
          <p className="text-slate-400">Partition view lands with STEP-009.</p>
        )}
      </main>
      <footer className="px-6 py-4 text-xs text-slate-500">
        read-only by construction · loopback only · no authentication by design (single trusted
        operator)
      </footer>
    </div>
  );
}
