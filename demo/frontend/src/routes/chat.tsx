import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Badge } from "@/components/ui/badge";
import { SCOPE } from "@/lib/constants";

type QaSource = { kind: "memory" | "belief"; id: string; text: string; source: string };
type QaResult = { answer: string; sources: QaSource[]; llm: "ok" | "unavailable" | "error" };
type RetrievalHit = { id: string; text: string; score: number };

const DEFAULT_CORPUS = `Engram is a contract-first agentic memory layer with a Rust core and TypeScript bindings.
Memory records carry content, scope, provenance, and policy. SQLite persists memories
and knowledge durably so the demo survives restarts. Taxonomy concept schemes organize
controlled vocabularies.`;

function Ask() {
  const [question, setQuestion] = useState("");
  const [result, setResult] = useState<QaResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const ask = async () => {
    if (!question.trim()) return;
    setBusy(true);
    setError("");
    setResult(null);
    try {
      const res = await fetch("/qa/ask", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ question: question.trim(), scope: SCOPE }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setResult((await res.json()) as QaResult);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex gap-2">
        <Input
          placeholder="Ask over knowledge + memory…"
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") ask();
          }}
        />
        <Button onClick={ask} disabled={busy || !question.trim()}>
          {busy ? "Answering…" : "Ask"}
        </Button>
      </div>
      {error && <p className="text-sm text-destructive">{error}</p>}
      {result?.llm === "unavailable" && (
        <p className="text-sm text-destructive">LLM unavailable (set .env) — evidence only</p>
      )}
      {result?.llm === "error" && (
        <p className="text-sm text-destructive">LLM synthesis failed — evidence only</p>
      )}
      {result && (
        <>
          <div className="rounded-md border-l-2 border-primary bg-primary/5 p-4">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              components={{
                h1: ({ children }) => <h1 className="mb-2 text-lg font-semibold">{children}</h1>,
                h2: ({ children }) => <h2 className="mb-2 text-base font-semibold">{children}</h2>,
                h3: ({ children }) => <h3 className="mb-1.5 text-sm font-semibold">{children}</h3>,
                p: ({ children }) => <p className="mb-2 text-sm leading-relaxed">{children}</p>,
                ul: ({ children }) => <ul className="mb-2 ml-4 list-disc space-y-1 text-sm">{children}</ul>,
                ol: ({ children }) => <ol className="mb-2 ml-4 list-decimal space-y-1 text-sm">{children}</ol>,
                li: ({ children }) => <li className="leading-relaxed">{children}</li>,
                strong: ({ children }) => <strong className="font-semibold">{children}</strong>,
                code: ({ children, className }) =>
                  className ? (
                    <pre className="my-2 overflow-x-auto rounded bg-muted p-2 text-xs"><code>{children}</code></pre>
                  ) : (
                    <code className="rounded bg-muted px-1 py-0.5 text-xs">{children}</code>
                  ),
                a: ({ href, children }) => (
                  <a href={href} className="text-primary underline" target="_blank" rel="noreferrer">{children}</a>
                ),
              }}
            >
              {result.answer}
            </ReactMarkdown>
          </div>
          {result.sources.length > 0 && (
            <div>
              <h4 className="mb-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Grounding ({result.sources.length})
              </h4>
              <ul className="space-y-1 text-sm">
                {result.sources.map((s, i) => (
                  <li key={`${s.kind}-${s.id}-${i}`} className="flex items-start gap-2">
                    <Badge variant="outline">{s.kind}</Badge>
                    <span>
                      <code className="text-xs text-muted-foreground">{s.source}</code> — {s.text}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </>
      )}
    </div>
  );
}

function ContextComposer() {
  const [corpus, setCorpus] = useState(DEFAULT_CORPUS);
  const [query, setQuery] = useState("");
  const [indexed, setIndexed] = useState<number | null>(null);
  const [hits, setHits] = useState<RetrievalHit[]>([]);
  const [error, setError] = useState("");

  const index = async () => {
    try {
      const res = await fetch("/retrieval/index", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ text: corpus }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setIndexed(((await res.json()) as { indexed: number }).indexed);
      setError("");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  const search = async () => {
    if (!query.trim()) return;
    try {
      const res = await fetch("/retrieval/search", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ query, topK: 5 }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setHits((await res.json()) as RetrievalHit[]);
      setError("");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  return (
    <div className="space-y-3">
      <p className="text-sm text-muted-foreground">
        Index a corpus, then retrieve passages to compose context for the answer.
      </p>
      <Textarea value={corpus} onChange={(e) => setCorpus(e.target.value)} rows={4} />
      <Button variant="secondary" onClick={index}>Index corpus</Button>
      {indexed !== null && (
        <span className="ml-2 text-sm text-muted-foreground">{indexed} passage(s) indexed</span>
      )}
      <div className="flex gap-2">
        <Input
          placeholder="Query passages…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") search();
          }}
        />
        <Button variant="secondary" onClick={search} disabled={indexed === null || !query.trim()}>
          Retrieve
        </Button>
      </div>
      {error && <p className="text-sm text-destructive">{error}</p>}
      <ul className="space-y-1 text-sm">
        {hits.map((h) => (
          <li key={h.id} className="flex items-start justify-between gap-2">
            <span className="flex-1">{h.text}</span>
            <Badge variant="outline">{h.score.toFixed(2)}</Badge>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function Chat() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Chat &amp; context composer</CardTitle>
      </CardHeader>
      <CardContent>
        <Tabs defaultValue="ask">
          <TabsList>
            <TabsTrigger value="ask">Ask</TabsTrigger>
            <TabsTrigger value="context">Context composer</TabsTrigger>
          </TabsList>
          <TabsContent value="ask" className="mt-4">
            <Ask />
          </TabsContent>
          <TabsContent value="context" className="mt-4">
            <ContextComposer />
          </TabsContent>
        </Tabs>
      </CardContent>
    </Card>
  );
}
