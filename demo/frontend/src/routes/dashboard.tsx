import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Graph3DCard } from "@/components/graph-3d-card";
import { SCOPE } from "@/lib/constants";
import { buildGraphData, type RawEntity, type RawRelationship } from "@/Graph3D";

type Repo = {
  id: string;
  name: string;
  gitRemote: string | null;
  gitBranch: string | null;
  gitSha: string | null;
  entityCount: number;
  relationshipCount: number;
  lastUpdated: string | null;
};

type Stats = {
  tenant: { tenant: string; workspace: string; environment: string };
  repos: Repo[];
  totalEntities: number;
  totalRelationships: number;
  totalChunks: number;
  totalRepos: number;
};

function GitBadge({ label, value }: { label: string; value: string | null }) {
  if (!value) return null;
  return (
    <span className="text-xs text-muted-foreground">
      {label}: <code className="text-foreground">{value.length > 20 ? value.slice(0, 8) + "…" + value.slice(-8) : value}</code>
    </span>
  );
}

export function Dashboard() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    const load = async () => {
      try {
        const res = await fetch("/knowledge/stats", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ scope: SCOPE }),
        });
        if (!res.ok) throw new Error(`${res.status}`);
        setStats((await res.json()) as Stats);
      } catch (e) {
        setError(String(e instanceof Error ? e.message : e));
      }
    };
    void load();
  }, []);

  if (error) return <div className="text-sm text-destructive">{error}</div>;
  if (!stats) return <div className="text-sm text-muted-foreground">Loading…</div>;

  return (
    <div className="space-y-4">
      <div className="grid gap-3 sm:grid-cols-4">
        <Card>
          <CardContent className="pt-4 text-center">
            <div className="text-2xl font-bold">{stats.totalRepos}</div>
            <div className="text-xs text-muted-foreground">Repositories</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-4 text-center">
            <div className="text-2xl font-bold">{stats.totalEntities.toLocaleString()}</div>
            <div className="text-xs text-muted-foreground">Entities</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-4 text-center">
            <div className="text-2xl font-bold">{stats.totalRelationships.toLocaleString()}</div>
            <div className="text-xs text-muted-foreground">Relationships</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-4 text-center">
            <div className="text-2xl font-bold">{stats.totalChunks.toLocaleString()}</div>
            <div className="text-xs text-muted-foreground">Chunks</div>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Indexed Repositories</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-2">
            {stats.repos.map((repo) => (
              <div
                key={repo.id}
                className="flex flex-wrap items-center gap-3 rounded-md border p-3"
              >
                <div className="flex-1 min-w-[12rem]">
                  <div className="font-medium">{repo.name}</div>
                  <div className="flex gap-3 mt-1">
                    <GitBadge label="branch" value={repo.gitBranch} />
                    <GitBadge label="sha" value={repo.gitSha} />
                    {repo.gitRemote && (
                      <a
                        href={repo.gitRemote.startsWith("http") ? repo.gitRemote : "#"}
                        target="_blank"
                        rel="noreferrer"
                        className="text-xs text-primary underline"
                      >
                        {repo.gitRemote.replace(/^git@/, "").replace(/^https?:\/\//, "")}
                      </a>
                    )}
                  </div>
                </div>
                <Badge variant="outline">{repo.entityCount} entities</Badge>
                <Badge variant="outline">{repo.relationshipCount} edges</Badge>
                {repo.lastUpdated && (
                  <span className="text-xs text-muted-foreground">
                    {new Date(repo.lastUpdated).toLocaleDateString()}
                  </span>
                )}
              </div>
            ))}
            {stats.repos.length === 0 && (
              <div className="text-sm text-muted-foreground">
                No repositories indexed yet. Open the Index dialog to scan a repo.
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
