import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

export function Dashboard() {
  return (
    <div className="mx-auto max-w-3xl">
      <Card>
        <CardHeader>
          <CardTitle>Engram knowledge platform</CardTitle>
          <CardDescription>
            Contract-first agentic memory — durable knowledge, taxonomy, beliefs,
            and Q&amp;A over the Rust core.
          </CardDescription>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          The shadcn-admin shell is up. Capability routes (ingest, index,
          knowledge, belief, memory, chat) are ported in the following commits —
          one route per capability, no duplication. The 3D knowledge graph returns
          to the dashboard once the routes land.
        </CardContent>
      </Card>
    </div>
  );
}
