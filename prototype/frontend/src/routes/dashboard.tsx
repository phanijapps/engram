import { useEffect, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Link } from "@tanstack/react-router";
import { Network, FolderTree, Database } from "lucide-react";
import { SCOPE } from "@/lib/constants";

type Repo = {
  id: string;
  name: string;
  gitRemote: string | null;
  gitBranch: string | null;
  gitSha: string | null;
  lastUpdated: string | null;
};
type Stats = {
  tenant: { tenant: string; workspace: string; environment: string };
  repos: Repo[];
  totalRepos: number;
};

function IndexDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (v: boolean) => void }) {
  const [root, setRoot] = useState("");
  const [force, setForce] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [status, setStatus] = useState("");

  useEffect(() => {
    if (!jobId) return;
    const id = setInterval(async () => {
      try {
        const res = await fetch(`/ingest/jobs/${jobId}`);
        const s = (await res.json()) as { status: string; summary?: Record<string, number> };
        setStatus(s.status === "done" || s.status === "error"
          ? `${s.status}${s.summary ? ` — ${s.summary.ingested} files · ${s.summary.entities} entities` : ""}`
          : `${s.status}…`);
        if (s.status === "done" || s.status === "error") clearInterval(id);
      } catch { /* ignore */ }
    }, 1500);
    return () => clearInterval(id);
  }, [jobId]);

  const start = async () => {
    if (!root.trim()) return;
    setStatus("starting…");
    try {
      const res = await fetch("/ingest/jobs", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ root: root.trim(), force }),
      });
      setJobId(((await res.json()) as { jobId: string }).jobId);
    } catch (e) { setStatus(String(e)); }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader><DialogTitle>Index a repository</DialogTitle></DialogHeader>
        <div className="space-y-3">
          <Input placeholder="/absolute/path/to/repo" value={root} onChange={(e) => setRoot(e.target.value)} />
          <div className="flex items-center gap-2">
            <Switch checked={force} onCheckedChange={setForce} id="idx-f" />
            <Label htmlFor="idx-f" className="text-xs text-muted-foreground">Force re-index</Label>
            <Button onClick={start} disabled={!root.trim() || !!jobId}>Index</Button>
          </div>
          {status && <p className="text-sm text-muted-foreground">{status}</p>}
          {status.startsWith("done") && (
            <Button variant="secondary" onClick={() => { onOpenChange(false); window.location.reload(); }}>
              Done — refresh
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function Dashboard() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [error, setError] = useState("");
  const [showIndex, setShowIndex] = useState(false);

  useEffect(() => {
    fetch("/knowledge/stats", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ scope: SCOPE }),
    })
      .then((r) => r.json() as Promise<Stats>)
      .then(setStats)
      .catch((e) => setError(String(e)));
  }, []);

  if (error) return <div className="p-4 text-sm text-destructive">{error}</div>;
  if (!stats) return <div className="p-4 text-sm text-muted-foreground">Loading…</div>;

  return (
    <div className="space-y-4 p-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">
          {stats.tenant.workspace} <span className="text-muted-foreground">/ {stats.tenant.tenant}</span>
        </h1>
        <Button onClick={() => setShowIndex(true)}>
          <FolderTree className="mr-2 size-4" /> Index repo
        </Button>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        <Card>
          <CardContent className="flex items-center gap-3 pt-4">
            <FolderTree className="size-8 text-primary" />
            <div>
              <div className="text-2xl font-bold">{stats.totalRepos}</div>
              <div className="text-xs text-muted-foreground">Repositories</div>
            </div>
          </CardContent>
        </Card>
        <Link to="/graph">
          <Card className="cursor-pointer transition-colors hover:border-primary">
            <CardContent className="flex items-center gap-3 pt-4">
              <Network className="size-8 text-primary" />
              <div>
                <div className="text-sm font-medium">Knowledge Graph →</div>
                <div className="text-xs text-muted-foreground">Visualize all entities + edges</div>
              </div>
            </CardContent>
          </Card>
        </Link>
        <Link to="/chat">
          <Card className="cursor-pointer transition-colors hover:border-primary">
            <CardContent className="flex items-center gap-3 pt-4">
              <Database className="size-8 text-primary" />
              <div>
                <div className="text-sm font-medium">Ask →</div>
                <div className="text-xs text-muted-foreground">Agentic Q&A over the graph</div>
              </div>
            </CardContent>
          </Card>
        </Link>
      </div>

      <Card>
        <CardHeader><CardTitle>Indexed Repositories</CardTitle></CardHeader>
        <CardContent>
          <div className="space-y-2">
            {stats.repos.map((repo) => (
              <div key={repo.id} className="flex flex-wrap items-center gap-3 rounded-md border p-3">
                <div className="flex-1 min-w-[12rem]">
                  <div className="font-medium">{repo.name}</div>
                  <div className="flex gap-3 mt-1">
                    {repo.gitBranch && <span className="text-xs text-muted-foreground">branch: <code className="text-foreground">{repo.gitBranch}</code></span>}
                    {repo.gitSha && <span className="text-xs text-muted-foreground">sha: <code className="text-foreground">{repo.gitSha.slice(0, 10)}</code></span>}
                    {repo.gitRemote && (
                      <a href="#" className="text-xs text-primary underline">{repo.gitRemote.replace(/^git@/, "").replace(/^https?:\/\//, "")}</a>
                    )}
                  </div>
                </div>
                {repo.lastUpdated && <span className="text-xs text-muted-foreground">{new Date(repo.lastUpdated).toLocaleDateString()}</span>}
              </div>
            ))}
            {stats.repos.length === 0 && (
              <div className="text-sm text-muted-foreground">No repos indexed. Click "Index repo" to scan a codebase.</div>
            )}
          </div>
        </CardContent>
      </Card>

      <IndexDialog open={showIndex} onOpenChange={setShowIndex} />
    </div>
  );
}
