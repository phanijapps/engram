import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { buildGraphData, type RawEntity, type RawRelationship } from "@/Graph3D";
import { Graph3DCard } from "@/components/graph-3d-card";
import { ACTOR, POLICY, SCOPE } from "@/lib/constants";

type LLMStatus = { entities: number; relationships: number } | "unavailable" | "error";
type IngestResult = {
  entities: RawEntity[];
  relationships: RawRelationship[];
  chunkCount: number;
  llm?: LLMStatus;
};

const DEFAULT_CODE =
  "fn write() { read(); flush(); }\nfn read() {}\nfn flush() {}\nstruct Store;\n";

function llmHint(llm: LLMStatus | undefined): string {
  if (!llm) return "";
  if (llm === "unavailable") return " · LLM unavailable (set .env)";
  if (llm === "error") return " · LLM failed — deterministic only";
  return ` · LLM +${llm.entities} entities · +${llm.relationships} edges`;
}

export function Ingest() {
  const [text, setText] = useState(DEFAULT_CODE);
  const [isCode, setIsCode] = useState(true);
  const [enhance, setEnhance] = useState(false);
  const [result, setResult] = useState<IngestResult | null>(null);
  const [error, setError] = useState("");

  const graphData = result
    ? buildGraphData(result.entities, result.relationships)
    : null;

  const ingest = async () => {
    try {
      const documentKind = isCode ? "code" : "text";
      const response = enhance
        ? await fetch("/llm/extract", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              text,
              documentKind,
              scope: SCOPE,
              policy: POLICY,
              sourceName: "demo-ingest",
              actor: ACTOR,
            }),
          })
        : await fetch("/ingest/extract", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              sourceKind: "filesystem",
              sourceName: "demo-ingest",
              scope: SCOPE,
              documentKind,
              document: { path: "snippet" },
              text,
              policy: POLICY,
              actor: ACTOR,
            }),
          });
      if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
      setResult((await response.json()) as IngestResult);
      setError("");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Ingest &amp; extract</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <Textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          rows={6}
          className="font-mono text-sm"
        />
        <div className="flex flex-wrap items-center gap-6">
          <div className="flex items-center gap-2">
            <Switch checked={isCode} onCheckedChange={setIsCode} id="is-code" />
            <Label htmlFor="is-code">code document</Label>
          </div>
          <div className="flex items-center gap-2">
            <Switch checked={enhance} onCheckedChange={setEnhance} id="enhance" />
            <Label htmlFor="enhance">LLM enhance</Label>
          </div>
          <Button onClick={ingest} disabled={!text.trim()}>
            Ingest &amp; extract
          </Button>
        </div>
        {error && <p className="text-sm text-destructive">{error}</p>}
        {result && (
          <p className="text-sm text-muted-foreground">
            {result.entities.length} entities · {result.relationships.length} edges ·{" "}
            {result.chunkCount} chunks{llmHint(result.llm)}
          </p>
        )}
        <Graph3DCard data={graphData} />
      </CardContent>
    </Card>
  );
}
