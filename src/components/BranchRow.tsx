import type { ReactNode } from "react";
import {
  GitBranch,
  ArrowUp,
  ArrowDown,
  Plus,
  GitFork,
  Link,
  Unlink,
  Layers,
  UploadCloud,
  ArrowRightToLine,
} from "lucide-react";
import type { Branch, BranchActionKind } from "../lib/types";
import { PrBadge } from "./PrBadge";

function IconBtn({
  title,
  onClick,
  children,
}: {
  title: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      title={title}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className="rounded p-1 text-neutral-500 hover:bg-neutral-700 hover:text-neutral-100"
    >
      {children}
    </button>
  );
}

export function BranchRow({
  branch,
  onAction,
  onSelect,
  isSelected,
}: {
  branch: Branch;
  onAction?: (kind: BranchActionKind, branch: Branch) => void;
  onSelect?: (branch: Branch) => void;
  isSelected?: boolean;
}) {
  return (
    <div
      onClick={() => onSelect?.(branch)}
      className={`group flex items-center gap-2 rounded-md px-2 py-1.5 ${
        onSelect ? "cursor-pointer" : ""
      } ${
        isSelected
          ? "bg-indigo-950/60 ring-1 ring-indigo-600/70"
          : branch.isCurrent
          ? "bg-indigo-950/30 ring-1 ring-indigo-800/50"
          : "hover:bg-neutral-900"
      }`}
    >
      <GitBranch
        className={`h-4 w-4 shrink-0 ${
          branch.isTrunk ? "text-amber-400" : "text-neutral-500"
        }`}
      />
      <span
        className={`font-mono text-sm ${
          branch.isCurrent ? "text-indigo-200" : "text-neutral-200"
        }`}
      >
        {branch.name}
      </span>
      {branch.isCurrent && (
        <span className="rounded bg-indigo-900/70 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-indigo-300">
          HEAD
        </span>
      )}
      {branch.dirty && (
        <span title="uncommitted changes" className="text-[10px] text-rose-400">
          ● uncommitted
        </span>
      )}

      <div className="ml-auto flex items-center gap-2">
        {onAction && (
          <div className="hidden items-center gap-0.5 group-hover:flex">
            {!branch.isCurrent && (
              <IconBtn title="Checkout this branch" onClick={() => onAction("checkout", branch)}>
                <ArrowRightToLine className="h-3.5 w-3.5" />
              </IconBtn>
            )}
            {!branch.isTrunk && branch.needsPush && (
              <IconBtn title="Publish (push to origin)" onClick={() => onAction("publish", branch)}>
                <UploadCloud className="h-3.5 w-3.5" />
              </IconBtn>
            )}
            <IconBtn title="New branch on top" onClick={() => onAction("new-child", branch)}>
              <Plus className="h-3.5 w-3.5" />
            </IconBtn>
            {!branch.isTrunk &&
              (branch.tracked ? (
                <>
                  <IconBtn title="Restack from here" onClick={() => onAction("restack", branch)}>
                    <Layers className="h-3.5 w-3.5" />
                  </IconBtn>
                  <IconBtn title="Set parent" onClick={() => onAction("set-parent", branch)}>
                    <GitFork className="h-3.5 w-3.5" />
                  </IconBtn>
                  <IconBtn title="Untrack" onClick={() => onAction("untrack", branch)}>
                    <Unlink className="h-3.5 w-3.5" />
                  </IconBtn>
                </>
              ) : (
                <IconBtn title="Track in stack" onClick={() => onAction("track", branch)}>
                  <Link className="h-3.5 w-3.5" />
                </IconBtn>
              ))}
          </div>
        )}

        {branch.needsPush && (
          <span
            title="local commits not pushed to origin"
            className="inline-flex items-center gap-0.5 rounded bg-sky-950/50 px-1.5 py-0.5 text-xs text-sky-300"
          >
            <UploadCloud className="h-3 w-3" />
            push
          </span>
        )}
        {!branch.isTrunk && branch.behind > 0 && (
          <span
            title="commits behind parent — needs restack"
            className="inline-flex items-center gap-0.5 rounded bg-amber-950/50 px-1.5 py-0.5 text-xs text-amber-300"
          >
            <ArrowDown className="h-3 w-3" />
            {branch.behind}
          </span>
        )}
        {!branch.isTrunk && branch.ahead > 0 && (
          <span
            title="commits ahead of parent"
            className="inline-flex items-center gap-0.5 rounded bg-emerald-950/40 px-1.5 py-0.5 text-xs text-emerald-300"
          >
            <ArrowUp className="h-3 w-3" />
            {branch.ahead}
          </span>
        )}
        {branch.pr && <PrBadge pr={branch.pr} />}
      </div>
    </div>
  );
}
