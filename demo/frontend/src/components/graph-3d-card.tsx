import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Graph3D, type GraphData } from "@/Graph3D";

export function Graph3DCard({
  data,
  title = "Knowledge graph",
}: {
  data: GraphData | null;
  title?: string;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      <CardContent>
        <Graph3D data={data} />
      </CardContent>
    </Card>
  );
}
