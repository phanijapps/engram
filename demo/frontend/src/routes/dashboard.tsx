import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Graph3DCard } from "@/components/graph-3d-card";

export function Dashboard() {
  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Engram knowledge platform</CardTitle>
          <CardDescription>
            Contract-first agentic memory — durable knowledge, taxonomy, beliefs,
            and Q&amp;A over the Rust core.
          </CardDescription>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          Ingest a document or index a repo to populate the graph, then ask over
          knowledge + memory. Navigate via the sidebar or the command palette
          (Ctrl/⌘K).
        </CardContent>
      </Card>
      <Graph3DCard data={null} />
    </div>
  );
}
