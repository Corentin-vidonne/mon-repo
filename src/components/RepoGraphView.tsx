import { useEffect, useMemo, useState } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  Handle,
  Position,
  type Edge,
  type Node,
  type NodeProps,
  type NodeTypes,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import * as dagre from "dagre";
import { Boxes } from "lucide-react";
import { api, errorText } from "../lib/api";
import type { RepoGraph } from "../lib/types";

const NODE_W = 192;
const NODE_H = 56;

function RepoNodeCard({ data }: NodeProps) {
  const d = data as unknown as { name: string; remote: string | null };
  return (
    <div className="w-48 rounded-lg border border-neutral-700 bg-neutral-900 px-3 py-2 hover:border-indigo-600">
      <Handle type="target" position={Position.Left} className="!h-1.5 !w-1.5 !border-0 !bg-neutral-700" />
      <div className="flex items-center gap-1.5">
        <Boxes className="h-3.5 w-3.5 shrink-0 text-indigo-400" />
        <span className="truncate text-xs font-medium text-neutral-100">{d.name}</span>
      </div>
      {d.remote && (
        <div className="mt-0.5 truncate text-[10px] text-neutral-500">{d.remote}</div>
      )}
      <Handle type="source" position={Position.Right} className="!h-1.5 !w-1.5 !border-0 !bg-neutral-700" />
    </div>
  );
}

const nodeTypes: NodeTypes = { repo: RepoNodeCard };

export function RepoGraphView({
  repos,
  onOpenRepo,
}: {
  repos: string[];
  onOpenRepo: (path: string) => void;
}) {
  const [graph, setGraph] = useState<RepoGraph | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setGraph(null);
    setError(null);
    api
      .repoGraph(repos)
      .then((g) => alive && setGraph(g))
      .catch((e) => alive && setError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [repos]);

  const { nodes, edges } = useMemo(() => {
    if (!graph) return { nodes: [] as Node[], edges: [] as Edge[] };
    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: "LR", nodesep: 30, ranksep: 90 });
    g.setDefaultEdgeLabel(() => ({}));
    graph.nodes.forEach((n) => g.setNode(n.id, { width: NODE_W, height: NODE_H }));
    graph.edges.forEach((e) => g.setEdge(e.from, e.to));
    dagre.layout(g);

    const nodes: Node[] = graph.nodes.map((n) => {
      const p = g.node(n.id);
      return {
        id: n.id,
        type: "repo",
        position: { x: (p?.x ?? 0) - NODE_W / 2, y: (p?.y ?? 0) - NODE_H / 2 },
        data: { name: n.name, remote: n.remoteUrl } as unknown as Record<string, unknown>,
      };
    });
    const edges: Edge[] = graph.edges.map((e, i) => ({
      id: `${e.from}->${e.to}-${i}`,
      source: e.from,
      target: e.to,
      label: e.via,
      type: "smoothstep",
      animated: true,
      style: { stroke: "#4f46e5" },
      labelStyle: { fill: "#a3a3a3", fontSize: 10 },
      labelBgStyle: { fill: "#0a0a0a" },
    }));
    return { nodes, edges };
  }, [graph]);

  if (error) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-red-400">{error}</div>
    );
  }
  if (!graph) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-neutral-600">
        Analyzing repositories…
      </div>
    );
  }
  if (graph.nodes.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-neutral-600">
        Add repositories to see the links between them.
      </div>
    );
  }

  return (
    <div className="h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodeClick={(_, n) => onOpenRepo(n.id)}
        fitView
        fitViewOptions={{ padding: 0.25 }}
        nodesDraggable={false}
        nodesConnectable={false}
        minZoom={0.1}
      >
        <Background color="#27272a" gap={20} />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  );
}
