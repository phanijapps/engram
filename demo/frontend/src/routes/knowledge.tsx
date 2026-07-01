import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { SCOPE } from "@/lib/constants";

type Klass = {
  id: string;
  label: string;
  description?: string;
  parentClassIds?: string[];
};
type Property = { id: string; label: string; domainClassId?: string; rangeClassId?: string };
type Sample = {
  ontology: { id: string; name: string };
  classes: Klass[];
  properties: Property[];
  axioms: unknown[];
  scheme: { id: string; name: string };
  concepts: { id: string; prefLabel?: { value?: string } }[];
};
type Finding = { id: string; severity: string; code: string; message: string };

export function Knowledge() {
  const [sample, setSample] = useState<Sample | null>(null);
  const [graphId, setGraphId] = useState("");
  const [findings, setFindings] = useState<Finding[] | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const load = async () => {
    setBusy(true);
    setError("");
    setFindings(null);
    try {
      const res = await fetch("/ontology/it-org", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ scope: SCOPE }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setSample(((await res.json()) as { sample: Sample }).sample);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const validate = async () => {
    if (!graphId.trim() || !sample) return;
    setBusy(true);
    setError("");
    try {
      const res = await fetch("/ontology/validate", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          graphId: graphId.trim(),
          ontologyId: sample.ontology.id,
          scope: SCOPE,
        }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setFindings(((await res.json()) as Finding[]) ?? []);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const classLabel = (id?: string) =>
    id ? sample?.classes.find((c) => c.id === id)?.label ?? id : "—";

  return (
    <Card>
      <CardHeader>
        <CardTitle>Ontology &amp; taxonomy</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={load} disabled={busy}>
            {sample ? "Reload IT-org ontology" : "Load IT-org ontology"}
          </Button>
          {sample && (
            <>
              <Input
                placeholder="graph id to validate"
                value={graphId}
                onChange={(e) => setGraphId(e.target.value)}
                className="min-w-[18rem]"
              />
              <Button onClick={validate} variant="secondary" disabled={busy || !graphId.trim()}>
                Validate graph
              </Button>
            </>
          )}
        </div>
        {error && <p className="text-sm text-destructive">{error}</p>}
        {sample && (
          <p className="text-sm text-muted-foreground">
            ontology <code>{sample.ontology.id}</code> · {sample.classes.length} classes ·{" "}
            {sample.properties.length} properties · {sample.axioms.length} axioms · taxonomy{" "}
            <code>{sample.scheme.id}</code> ({sample.concepts.length} concepts)
          </p>
        )}
        {sample && (
          <div className="grid gap-6 md:grid-cols-2">
            <div>
              <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Classes
              </h3>
              <ul className="space-y-1 text-sm">
                {sample.classes.map((c) => (
                  <li key={c.id}>
                    <span className="font-medium">{c.label}</span>
                    {c.parentClassIds && c.parentClassIds.length > 0 && (
                      <span className="text-muted-foreground">
                        {" "}extends {c.parentClassIds.map(classLabel).join(", ")}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
            <div>
              <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Properties
              </h3>
              <ul className="space-y-1 text-sm">
                {sample.properties.map((p) => (
                  <li key={p.id}>
                    <span className="font-medium">{p.label}</span>{" "}
                    <span className="text-muted-foreground">
                      {classLabel(p.domainClassId)} → {classLabel(p.rangeClassId)}
                    </span>
                  </li>
                ))}
              </ul>
              <h3 className="mb-2 mt-4 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Taxonomy
              </h3>
              <ul className="space-y-1 text-sm">
                {sample.concepts.map((c) => (
                  <li key={c.id}>{c.prefLabel?.value ?? c.id}</li>
                ))}
              </ul>
            </div>
          </div>
        )}
        {findings && (
          <div className="rounded-md border p-3">
            <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Validation findings ({findings.length})
            </h3>
            {findings.length === 0 ? (
              <p className="text-sm text-muted-foreground">No undeclared predicates — graph conforms.</p>
            ) : (
              <ul className="space-y-1 text-sm">
                {findings.map((f) => (
                  <li key={f.id} className="flex items-start gap-2">
                    <Badge variant="outline">{f.severity}</Badge>
                    <span>{f.message}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
