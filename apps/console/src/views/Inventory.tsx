import type { InventoryResponse, PartitionSummary } from "../api";

/** Display label for one scope dimension: unset renders "(unset)". */
export function dimensionLabel(value: string | null | undefined): string {
  return value === null || value === undefined || value === "" ? "(unset)" : value;
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
      <p className="mb-2 text-sm text-slate-400">
        {inventory.partitionCount} partition{inventory.partitions.length === 1 ? "" : "s"} present
        in the data
      </p>
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
              className="cursor-pointer hover:bg-slate-900"
              onClick={() => onSelect(partition)}
            >
              <PartitionRow partition={partition} />
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
