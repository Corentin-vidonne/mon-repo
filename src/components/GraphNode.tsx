import { Handle, Position, type NodeProps } from "@xyflow/react";
import { GitBranch } from "lucide-react";
import type { Branch } from "../lib/types";

export type GraphNodeData = { branch: Branch; selected: boolean };

function prStateClass(state: string): string {
  if (state === "MERGED") return "text-purple-300";
  if (state === "CLOSED") return "text-red-300";
  return "text-emerald-300";
}

export function GraphNode({ data }: NodeProps) {
  const { branch: b, selected } = data as unknown as GraphNodeData;
  const untracked = !b.isTrunk && !b.tracked;
  return (
    <div
      className={`w-52 rounded-lg border px-3 py-2 shadow-sm ${
        selected
          ? "border-indigo-500 bg-indigo-950/60 ring-2 ring-indigo-500/40"
          : b.isTrunk
          ? "border-amber-700/60 bg-neutral-900"
          : "border-neutral-700 bg-neutral-900 hover:border-neutral-600"
      } ${untracked ? "border-dashed" : ""}`}
    >
      <Handle
        type="target"
        position={Position.Top}
        className="!h-1.5 !w-1.5 !border-0 !bg-neutral-700"
      />
      <div className="flex items-center gap-1.5">
        <GitBranch
          className={`h-3.5 w-3.5 shrink-0 ${b.isTrunk ? "text-amber-400" : "text-neutral-500"}`}
        />
        <span className="truncate font-mono text-xs text-neutral-100">{b.name}</span>
        {b.isCurrent && (
          <span className="ml-auto rounded bg-indigo-900/70 px-1 text-[9px] font-semibold uppercase text-indigo-300">
            HEAD
          </span>
        )}
      </div>
      <div className="mt-1 flex items-center gap-2 text-[10px]">
        {untracked && <span className="text-neutral-500">untracked</span>}
        {!b.isTrunk && b.behind > 0 && <span className="text-amber-300">↓{b.behind}</span>}
        {!b.isTrunk && b.ahead > 0 && <span className="text-emerald-300">↑{b.ahead}</span>}
        {b.dirty && (
          <span className="text-rose-400" title="uncommitted changes">
            ●
          </span>
        )}
        {b.needsPush && (
          <span className="text-sky-300" title="unpushed commits">
            ⇡
          </span>
        )}
        {b.pr && (
          <span className={`ml-auto font-mono ${prStateClass(b.pr.state)}`}>#{b.pr.number}</span>
        )}
      </div>
      <Handle
        type="source"
        position={Position.Bottom}
        className="!h-1.5 !w-1.5 !border-0 !bg-neutral-700"
      />
    </div>
  );
}
