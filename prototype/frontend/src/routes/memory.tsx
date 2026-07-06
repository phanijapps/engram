import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ACTOR, POLICY, SCOPE, requester } from "@/lib/constants";

type MemoryItem = { targetId?: string; content?: { text?: string } };

export function Memory() {
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<MemoryItem[]>([]);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

  const retrieve = async (q: string) => {
    if (!q.trim()) {
      setItems([]);
      return;
    }
    try {
      const res = await fetch("/memory/retrieve", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          query: q,
          scope: SCOPE,
          requester: requester("memory.retrieve"),
          modes: ["keyword"],
          limit: 10,
          budget: { maxItems: 10, maxTokens: 4000 },
          includeExplanations: true,
        }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setItems(((await res.json()) as { items?: MemoryItem[] }).items ?? []);
      setError("");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  const write = async () => {
    if (!text.trim()) return;
    const now = new Date().toISOString();
    try {
      const res = await fetch("/memory/write", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          kind: "observation",
          content: { text, summary: text.slice(0, 80), language: "en", format: "text" },
          scope: SCOPE,
          requester: requester("memory.write"),
          provenance: { source: "demo-ui", actor: ACTOR, observedAt: now, confidence: 1, method: "manual" },
          policy: POLICY,
          idempotencyKey: `ui-${now}`,
        }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      const body = (await res.json()) as { record: { id: string } };
      setMessage(`wrote memory ${body.record.id}`);
      setText("");
      await retrieve(query);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  const forget = async (targetId: string) => {
    try {
      const res = await fetch("/memory/forget", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          targetType: "memory",
          targetId,
          scope: SCOPE,
          requester: requester("memory.forget"),
        }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setMessage(`forgot ${targetId}`);
      await retrieve(query);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Memory</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={3}
            placeholder="Write a memory…"
          />
          <Button onClick={write} disabled={!text.trim()}>Write memory</Button>
        </div>

        <div className="space-y-2">
          <div className="flex gap-2">
            <Input
              placeholder="Search memories…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") retrieve(query);
              }}
            />
            <Button variant="secondary" onClick={() => retrieve(query)} disabled={!query.trim()}>
              Retrieve
            </Button>
          </div>
          <ul className="space-y-1 text-sm">
            {items.map((item, i) => (
              <li key={item.targetId ?? i} className="flex items-start justify-between gap-2">
                <span className="flex-1">{item.content?.text ?? item.targetId ?? JSON.stringify(item).slice(0, 80)}</span>
                {item.targetId && (
                  <div className="flex items-center gap-2">
                    <code className="text-xs text-muted-foreground">{item.targetId}</code>
                    <Button size="sm" variant="ghost" onClick={() => forget(item.targetId!)}>
                      Forget
                    </Button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        </div>

        {error && <p className="text-sm text-destructive">{error}</p>}
        {message && <p className="text-sm text-muted-foreground">{message}</p>}
      </CardContent>
    </Card>
  );
}
