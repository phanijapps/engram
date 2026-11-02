import type { InventoryResponse, PartitionSummary } from "../api";

/** Display label for one scope dimension: unset renders "(unset)". */
export function dimensionLabel(value: string | null | undefined): string {
  return value === null || value === undefined || value === "" ? "(unset)" : value;
}

/** The org/project alias note (DEC-002): engine vocabulary with the owner's
 * launch-flag aliases stated once, visibly, in the inventory header. */
export function AliasHeader() {
  return (
    <p className="text-sm text-slate-400">
      Engine vocabulary: partitions are distinct tenant/subject/workspace/session/environment
      tuples. Launch aliases map <code>--org</code> → tenant and <code>--project</code> →
      workspace.
    </p>
  );
}

/** The bounded summary line over the whole inventory (NFR-006): totals only,
 * never a full record dump. */
export function InventorySummary({
  partitions
}: {
  partitions: PartitionSummary[];
}) {
  const memories = partitions.reduce((sum, row) => sum + row.memoryCount, 0);
  const entities = partitions.reduce((sum, row) => sum + row.entityCount, 0);
  const beliefs = partitions.reduce((sum, row) => sum + row.beliefCount, 0);
  return (
    <p className="mb-2 text-sm text-slate-400">
      {partitions.length} partition{partitions.length === 1 ? "" : "s"} · {memories} memories ·{" "}
      {entities} entities · {beliefs} beliefs
    </p>
  );
}

/** One inventory row: the partition tuple with counts (REQ-001). */
export function PartitionRow({ partition }: { partition: PartitionSummary }) {
  const s = partition.scope;
  return (
    <tr className="border-b border-slate-800/60">
      <td className="px-3 py-1.5 font-mono text-xs">{s.tenant}</td>
      <td className="px-3 py-1.5 font-mono text-xs">{dimensionLabel(s.subject)}</td>
      <td className="px-3 py-1.5 font-mono text-xs">{dimensionLabel(s.workspace)}</td>
      <td className="px-3 py-1.5 font-mono text-xs">{dimensionLabel(s.session)}</td>
      <td className="px-3 py-1.5 font-mono text-xs">{dimensionLabel(s.environment)}</td>
      <td className="px-3 py-1.5 text-right tabular-nums">{partition.memoryCount}</td>
      <td className="px-3 py-1.5 text-right tabular-nums">{partition.entityCount}</td>
      <td className="px-3 py-1.5 text-right tabular-nums">{partition.beliefCount}</td>
    </tr>
  );
}

/**
 * The inventory view (full build lands with STEP-008/TASK-009): one row per
 * partition with memory/entity/belief counts, "(unset)" labels, and the named
 * empty/degraded states. Scaffold state: live fetch + table + states.
 */
export function InventoryView({
  inventory,
  onSelect
}: {
  inventory: InventoryResponse | null;
  onSelect: (partition: PartitionSummary) => void;
}) {
  if (inventory === null) {
    return <p className="text-slate-400">Loading inventory…</p>;
  }
  if (inventory.state !== "ready") {
    return (
      <section className="rounded border border-amber-700/50 bg-amber-950/30 p-4">
        <h2 className="font-semibold text-amber-300">State: {inventory.state}</h2>
        <p className="text-sm text-amber-200/80">{inventory.message ?? ""}</p>
      </section>
    );
  }
  if (inventory.partitions.length === 0) {
    return (
      <section className="rounded border border-slate-700 bg-slate-900/60 p-4">
        <h2 className="font-semibold">Empty store</h2>
        <p className="text-sm text-slate-400">
          No partitions are present in this store's data yet. Memories, knowledge entities, or
          beliefs written through the engine appear here automatically.
        </p>
      </section>
    );
  }
  return (
    <section>
      <InventorySummary partitions={inventory.partitions} />
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-slate-700 text-left text-xs uppercase tracking-wide text-slate-400">
            <th className="px-3 py-2">tenant (org)</th>
            <th className="px-3 py-2">subject</th>
            <th className="px-3 py-2">workspace (project)</th>
            <th className="px-3 py-2">session</th>
            <th className="px-3 py-2">environment</th>
            <th className="px-3 py-2 text-right">memories</th>
            <th className="px-3 py-2 text-right">entities</th>
            <th className="px-3 py-2 text-right">beliefs</th>
          </tr>
        </thead>
        <tbody>
          {inventory.partitions.map((partition) => (
            <tr
              key={JSON.stringify(partition.scope)}
              role="link"
              tabIndex={0}
              aria-label={`open partition ${partition.scope.tenant}${partition.scope.workspace ? ` / ${partition.scope.workspace}` : ""}`}
              className="cursor-pointer hover:bg-slate-900 focus-visible:bg-slate-900 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-500 focus-visible:-outline-offset-2"
              onClick={() => onSelect(partition)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  onSelect(partition);
                }
              }}
            >
              <PartitionRow partition={partition} />
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
