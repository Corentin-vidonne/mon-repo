import { useMemo } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  type Edge,
  type Node,
  type NodeTypes,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import * as dagre from "dagre";
import type { Branch, StackNode } from "../lib/types";
import { GraphNode, type GraphNodeData } from "./GraphNode";

const NODE_W = 208;
const NODE_H = 60;
const nodeTypes: NodeTypes = { branch: GraphNode };

export function StackGraph({
  roots,
  untracked,
  selected,
  onSelect,
}: {
  roots: StackNode[];
  untracked: Branch[];
  selected: string | null;
  onSelect: (name: string) => void;
}) {
  const { nodes, edges } = useMemo(() => {
    const data: GraphNodeData[] = [];
    const links: { source: string; target: string }[] = [];
    const walk = (n: StackNode) => {
      data.push({ branch: n.branch, selected: n.branch.name === selected });
      n.children.forEach((c) => {
        links.push({ source: n.branch.name, target: c.branch.name });
        walk(c);
      });
    };
    roots.forEach(walk);
    // Untracked branches: standalone nodes (no parent edge) so they're all visible.
    untracked.forEach((b) => data.push({ branch: b, selected: b.name === selected }));

    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: "TB", nodesep: 28, ranksep: 48 });
    g.setDefaultEdgeLabel(() => ({}));
    data.forEach((d) => g.setNode(d.branch.name, { width: NODE_W, height: NODE_H }));
    links.forEach((l) => g.setEdge(l.source, l.target));
    dagre.layout(g);

    const nodes: Node[] = data.map((d) => {
      const p = g.node(d.branch.name);
      return {
        id: d.branch.name,
        type: "branch",
        position: { x: p.x - NODE_W / 2, y: p.y - NODE_H / 2 },
        data: d as unknown as Record<string, unknown>,
      };
    });
    const edges: Edge[] = links.map((l) => ({
      id: `${l.source}->${l.target}`,
      source: l.source,
      target: l.target,
      type: "smoothstep",
      style: { stroke: "#3f3f46" },
    }));
    return { nodes, edges };
  }, [roots, untracked, selected]);

  return (
    <div className="h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodeClick={(_, n) => onSelect(n.id)}
        fitView
        fitViewOptions={{ padding: 0.25 }}
        nodesDraggable={false}
        nodesConnectable={false}
        minZoom={0.2}
      >
        <Background color="#27272a" gap={20} />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  );
}
