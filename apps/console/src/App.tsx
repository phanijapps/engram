import { useEffect, useState } from "react";
import { fetchInventory, type InventoryResponse } from "./api";
import { AliasHeader, InventoryView } from "./views/Inventory";
import { PartitionView, type PartitionTuple } from "./views/Partition";
import { AnomaliesView } from "./views/Anomalies";
import { fetchAnomalies } from "./api";

/**
 * Console shell (job 012 STEP-007 scaffold): the org/project alias header
 * (DEC-002) and the view switch. Views land with their owning tasks; the
 * scaffold ships the shell plus the live inventory fetch so the zero-CLI
 * journey has a pulse from the first start.
 */
export function App() {
  const [inventory, setInventory] = useState<InventoryResponse | null>(null);
  const [view, setView] = useState<"inventory" | "partition" | "anomalies">("inventory");
  const [selected, setSelected] = useState<PartitionTuple | null>(null);

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
        <button
          className={`rounded-t px-3 py-1.5 ${view === "anomalies" ? "bg-slate-800" : "hover:bg-slate-900"}`}
          onClick={() => setView("anomalies")}
        >
          Anomalies
        </button>
      </nav>
      <main className="px-6 py-4">
        {view === "inventory" ? (
          <InventoryView
            inventory={inventory}
            onSelect={(partition) => {
              setSelected(partition.scope);
              setView("partition");
            }}
          />
        ) : view === "anomalies" ? (
          <AnomaliesView
            onLoad={fetchAnomalies}
            onOpenPartition={(scope) => {
              setSelected(scope);
              setView("partition");
            }}
          />
        ) : selected ? (
          <PartitionView scope={selected} onBack={() => setView("inventory")} />
        ) : (
          <p className="text-slate-400">Select a partition from the inventory.</p>
        )}
      </main>
      <footer className="px-6 py-4 text-xs text-slate-500">
        read-only by construction · loopback only · no authentication by design (single trusted
        operator)
      </footer>
    </div>
  );
}
