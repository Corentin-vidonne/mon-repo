import { useCallback, useEffect, useMemo, useRef } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  type Edge,
  type Node,
  type NodeTypes,
  type ReactFlowInstance,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import * as dagre from "dagre";
import type { CommitNode } from "../lib/types";
import { CommitNodeCard } from "./CommitNodeCard";

const NODE_W = 224;
const NODE_H = 52;
const nodeTypes: NodeTypes = { commit: CommitNodeCard };

export function CommitGraph({
  nodes: commits,
  selected,
  onSelect,
}: {
  nodes: CommitNode[];
  selected: string | null;
  onSelect: (sha: string) => void;
}) {
  const rf = useRef<ReactFlowInstance | null>(null);

  const { nodes, edges } = useMemo(() => {
    const present = new Set(commits.map((c) => c.sha));
    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: "TB", nodesep: 16, ranksep: 36 });
    g.setDefaultEdgeLabel(() => ({}));
    commits.forEach((c) => g.setNode(c.sha, { width: NODE_W, height: NODE_H }));

    const links: { source: string; target: string }[] = [];
    commits.forEach((c) =>
      c.parents.forEach((p) => {
        if (present.has(p)) {
          g.setEdge(c.sha, p);
          links.push({ source: c.sha, target: p });
        }
      })
    );
    dagre.layout(g);

    const nodes: Node[] = commits.map((c) => {
      const pos = g.node(c.sha);
      return {
        id: c.sha,
        type: "commit",
        position: { x: pos.x - NODE_W / 2, y: pos.y - NODE_H / 2 },
        data: { node: c, selected: c.sha === selected } as unknown as Record<string, unknown>,
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
  }, [commits, selected]);

  // Open zoomed on the most recent commits (readable), not fit-all-zoomed-out.
  const focusRecent = useCallback(
    (inst: ReactFlowInstance) => {
      const recent = commits.slice(0, 12).map((c) => ({ id: c.sha }));
      if (recent.length === 0) return;
      inst.fitView({ nodes: recent, padding: 0.3, maxZoom: 1.15, duration: 250 });
    },
    [commits]
  );

  useEffect(() => {
    if (rf.current) focusRecent(rf.current);
  }, [focusRecent]);

  if (commits.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-neutral-600">
        No commits to display.
      </div>
    );
  }

  return (
    <div className="h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onInit={(inst) => {
          rf.current = inst;
          focusRecent(inst);
        }}
        onNodeClick={(_, n) => onSelect(n.id)}
        nodesDraggable={false}
        nodesConnectable={false}
        minZoom={0.05}
      >
        <Background color="#27272a" gap={20} />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  );
}
