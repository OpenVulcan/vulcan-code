import { expect, it } from "vitest";
import { layoutGraph, type GraphLayoutEdge, type GraphLayoutNode } from "./answer-graph-layout";

// box returns one equally sized node per id.
// box 为每个标识返回一个尺寸相同的节点。
function boxes(ids: string[]): GraphLayoutNode[] {
  return ids.map((id) => ({ id, width: 90, height: 34 }));
}

// Long edges that skip ranks are routed around the other nodes; every routed point stays inside the drawing with
// room for strokes, and nodes stay inside it too.
// 跨层的长连线绕开其他节点；每个连线点都留有描边余量地位于绘图内，节点同样位于绘图内。
it("keeps every edge point and node inside the drawing", () => {
  const ids = ["A", "B", "C", "D", "E", "F"];
  const edges: GraphLayoutEdge[] = [
    { from: "A", to: "B" }, { from: "B", to: "C" }, { from: "C", to: "D" }, { from: "D", to: "E" },
    { from: "A", to: "E", labelWidth: 60, labelHeight: 16 }, { from: "E", to: "A" }, { from: "D", to: "B" },
    { from: "F", to: "E" }, { from: "A", to: "F" }, { from: "C", to: "A" },
  ];
  for (const horizontal of [false, true]) {
    const layout = layoutGraph(boxes(ids), edges, { horizontal, nodeGap: 24, rankGap: 40 });
    for (const point of layout.edges.flatMap((edge) => edge.points)) {
      expect(point.x).toBeGreaterThanOrEqual(5.9);
      expect(point.y).toBeGreaterThanOrEqual(5.9);
      expect(point.x).toBeLessThanOrEqual(layout.width - 5.9);
      expect(point.y).toBeLessThanOrEqual(layout.height - 5.9);
    }
    for (const center of layout.nodes.values()) {
      expect(center.x - 45).toBeGreaterThanOrEqual(0);
      expect(center.y - 17).toBeGreaterThanOrEqual(0);
      expect(center.x + 45).toBeLessThanOrEqual(layout.width);
      expect(center.y + 17).toBeLessThanOrEqual(layout.height);
    }
  }
});
