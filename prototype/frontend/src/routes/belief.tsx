import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { ACTOR, POLICY, SCOPE } from "@/lib/constants";

type Belief = {
  id: string;
  subject: { key: string };
  content: string;
  status: string;
  confidence: number;
  validFrom?: string;
  validUntil?: string;
};
type Contradiction = {
  id: string;
  kind: string;
  severity: number;
  status: string;
  targets: { targetId: string }[];
  reasoning?: string;
};

function fmtTime(iso?: string): string {
  return iso ? new Date(iso).toLocaleString() : "—";
}

export function Belief() {
  const [beliefs, setBeliefs] = useState<Belief[]>([]);
  const [contradictions, setContradictions] = useState<Contradiction[]>([]);
  const [key, setKey] = useState("");
  const [content, setContent] = useState("");
  const [confidence, setConfidence] = useState(0.8);
  const [validFrom, setValidFrom] = useState("");
  const [validUntil, setValidUntil] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = async () => {
    try {
      const [b, c] = await Promise.all([
        fetch("/belief/list", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ scope: SCOPE }),
        }).then((r) => r.json() as Promise<Belief[]>),
        fetch("/belief/contradictions", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ scope: SCOPE }),
        }).then((r) => r.json() as Promise<Contradiction[]>),
      ]);
      setBeliefs(b);
      setContradictions(c);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  const add = async () => {
    if (!key.trim() || !content.trim()) return;
    setBusy(true);
    setError("");
    try {
      const now = new Date().toISOString();
      await fetch("/belief/put", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          id: `belief-${Date.now()}`,
          scope: SCOPE,
          subject: { key: key.trim(), aliases: [] },
          content: content.trim(),
          status: "active",
          confidence,
          sources: [],
          policy: POLICY,
          provenance: { source: "demo:belief", actor: ACTOR, observedAt: now, confidence: 1, method: "manual" },
          createdAt: now,
          validFrom: validFrom ? new Date(validFrom).toISOString() : undefined,
          validUntil: validUntil ? new Date(validUntil).toISOString() : undefined,
        }),
      });
      setKey("");
      setContent("");
      await refresh();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const detect = async () => {
    setBusy(true);
    setError("");
    try {
      await fetch("/belief/detect", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ scope: SCOPE }),
      });
      await refresh();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const resolve = async (id: string, kind: "manual_ignore" | "target_won") => {
    setBusy(true);
    setError("");
    try {
      await fetch("/belief/resolve", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          id,
          scope: SCOPE,
          resolution: { kind, actor: ACTOR, resolvedAt: new Date().toISOString() },
        }),
      });
      await refresh();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Beliefs &amp; contradictions</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex gap-2">
          <Button variant="secondary" onClick={refresh}>Refresh</Button>
          <Button onClick={detect} disabled={busy || beliefs.length === 0}>Detect contradictions</Button>
        </div>
        {error && <p className="text-sm text-destructive">{error}</p>}

        <div className="grid gap-2 rounded-md border p-3 md:grid-cols-[1fr_1fr_auto_auto_auto]">
          <Input placeholder="subject key (e.g. svc-a)" value={key} onChange={(e) => setKey(e.target.value)} />
          <Input placeholder="belief content" value={content} onChange={(e) => setContent(e.target.value)} />
          <div className="flex flex-col gap-1">
            <Label className="text-xs text-muted-foreground">confidence {confidence.toFixed(2)}</Label>
            <input type="range" min={0} max={1} step={0.05} value={confidence} onChange={(e) => setConfidence(Number(e.target.value))} className="w-28" />
          </div>
          <Input type="datetime-local" value={validFrom} onChange={(e) => setValidFrom(e.target.value)} title="valid from" />
          <Input type="datetime-local" value={validUntil} onChange={(e) => setValidUntil(e.target.value)} title="valid until" />
          <Button onClick={add} disabled={busy || !key.trim() || !content.trim()}>Add</Button>
        </div>

        <div className="grid gap-6 md:grid-cols-2">
          <div>
            <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Beliefs ({beliefs.length})
            </h3>
            <ul className="space-y-2 text-sm">
              {beliefs.map((b) => (
                <li key={b.id} className="space-y-1">
                  <div>
                    <code>{b.subject.key}</code> · {b.content} <Badge variant="outline">{b.status}</Badge>
                  </div>
                  <Progress value={Math.round(b.confidence * 100)} className="h-1.5" />
                  <p className="text-xs text-muted-foreground">
                    valid {fmtTime(b.validFrom)} → {fmtTime(b.validUntil)} (display-only)
                  </p>
                </li>
              ))}
            </ul>
          </div>
          <div>
            <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Contradictions ({contradictions.length})
            </h3>
            <ul className="space-y-2 text-sm">
              {contradictions.map((c) => (
                <li key={c.id} className="space-y-1">
                  <div>
                    <code>{c.kind}</code> · severity {c.severity.toFixed(2)}{" "}
                    <Badge variant={c.status === "open" ? "default" : "outline"}>{c.status}</Badge>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {c.targets.length} target(s) · {c.reasoning ?? "—"}
                  </p>
                  {c.status === "open" && (
                    <div className="flex gap-2">
                      <Button size="sm" variant="secondary" onClick={() => resolve(c.id, "target_won")}>target won</Button>
                      <Button size="sm" variant="ghost" onClick={() => resolve(c.id, "manual_ignore")}>ignore</Button>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
