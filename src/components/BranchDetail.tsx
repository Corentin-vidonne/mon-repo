import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import {
  X,
  ExternalLink,
  Plus,
  Layers,
  GitFork,
  Unlink,
  Link as LinkIcon,
  ArrowRightToLine,
  UploadCloud,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Branch, BranchActionKind, CommitInfo } from "../lib/types";
import { api } from "../lib/api";

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <span className="text-xs text-neutral-500">{label}</span>
      <span className={`text-xs text-neutral-300 ${mono ? "font-mono" : ""}`}>{value}</span>
    </div>
  );
}

function ActionBtn({
  icon,
  label,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-800"
    >
      {icon}
      {label}
    </button>
  );
}

export function BranchDetail({
  repoPath,
  branch,
  onAction,
  onClose,
  onOpenPr,
}: {
  repoPath: string;
  branch: Branch;
  onAction: (kind: BranchActionKind, branch: Branch) => void;
  onClose: () => void;
  onOpenPr?: (number: number) => void;
}) {
  const [commits, setCommits] = useState<CommitInfo[] | null>(null);

  useEffect(() => {
    let alive = true;
    setCommits(null);
    api
      .branchCommits(repoPath, branch.name)
      .then((c) => alive && setCommits(c))
      .catch(() => alive && setCommits([]));
    return () => {
      alive = false;
    };
  }, [repoPath, branch.name]);

  return (
    <aside className="flex h-full w-full flex-col border-l border-neutral-800 bg-neutral-900/40">
      <div className="flex h-14 items-center gap-2 border-b border-neutral-800 px-4">
        <span className="truncate font-mono text-sm text-neutral-100">{branch.name}</span>
        {branch.isTrunk && (
          <span className="rounded bg-amber-900/40 px-1.5 py-0.5 text-[10px] text-amber-300">
            trunk
          </span>
        )}
        {branch.isCurrent && (
          <span className="rounded bg-indigo-900/60 px-1.5 py-0.5 text-[10px] text-indigo-300">
            HEAD
          </span>
        )}
        <button
          onClick={onClose}
          className="ml-auto rounded p-1 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="flex-1 space-y-4 overflow-auto p-4">
        <div className="space-y-1.5">
          <Row
            label="Parent"
            value={branch.parent ?? (branch.isTrunk ? "—" : "untracked")}
            mono
          />
          {!branch.isTrunk && (
            <Row label="Status" value={`${branch.ahead} ahead · ${branch.behind} behind`} />
          )}
          {branch.baseSha && <Row label="Base" value={branch.baseSha.slice(0, 8)} mono />}
          <Row
            label="Working tree"
            value={branch.dirty ? "uncommitted changes" : "clean"}
          />
          <Row
            label="Remote"
            value={branch.needsPush ? "unpushed commits" : "up to date"}
          />
        </div>

        <div>
          <h4 className="mb-1 text-xs uppercase tracking-wider text-neutral-500">
            Pull request
          </h4>
          {branch.pr ? (
            <div className="flex items-center gap-2 rounded-md border border-neutral-700 px-3 py-2">
              <button
                onClick={() => branch.pr && onOpenPr?.(branch.pr.number)}
                title="View PR details"
                className="flex flex-1 items-center gap-2 text-left"
              >
                <span className="font-mono text-xs text-neutral-200">#{branch.pr.number}</span>
                <span className="text-xs text-neutral-400">{branch.pr.state}</span>
                {branch.pr.reviewDecision && (
                  <span className="text-xs text-neutral-500">{branch.pr.reviewDecision}</span>
                )}
                {branch.pr.checks && (
                  <span
                    className={`text-xs ${
                      branch.pr.checks === "SUCCESS"
                        ? "text-emerald-400"
                        : branch.pr.checks === "FAILURE"
                        ? "text-red-400"
                        : "text-amber-400"
                    }`}
                  >
                    CI {branch.pr.checks}
                  </span>
                )}
              </button>
              <button
                onClick={() => branch.pr && openUrl(branch.pr.url)}
                title="Open on GitHub"
                className="rounded p-1 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
              >
                <ExternalLink className="h-3.5 w-3.5" />
              </button>
            </div>
          ) : (
            <p className="text-xs text-neutral-600">No PR yet.</p>
          )}
        </div>

        <div>
          <h4 className="mb-1 text-xs uppercase tracking-wider text-neutral-500">
            {branch.isTrunk ? "Recent commits" : "Commits on this branch"}
          </h4>
          {commits === null && <p className="text-xs text-neutral-600">Loading…</p>}
          {commits && commits.length === 0 && (
            <p className="text-xs text-neutral-600">No commits.</p>
          )}
          <ul className="space-y-1.5">
            {commits?.map((c) => (
              <li key={c.sha} className="flex gap-2">
                <span className="font-mono text-[11px] text-amber-300/80">{c.sha}</span>
                <div className="min-w-0">
                  <p className="truncate text-xs text-neutral-200">{c.subject}</p>
                  <p className="text-[10px] text-neutral-600">
                    {c.author} · {c.date}
                  </p>
                </div>
              </li>
            ))}
          </ul>
        </div>
      </div>

      <div className="border-t border-neutral-800 p-3">
        <div className="flex flex-wrap gap-1.5">
          {!branch.isCurrent && (
            <ActionBtn
              icon={<ArrowRightToLine className="h-3.5 w-3.5" />}
              label="Checkout"
              onClick={() => onAction("checkout", branch)}
            />
          )}
          <ActionBtn
            icon={<Plus className="h-3.5 w-3.5" />}
            label="New branch"
            onClick={() => onAction("new-child", branch)}
          />
          {!branch.isTrunk && branch.needsPush && (
            <ActionBtn
              icon={<UploadCloud className="h-3.5 w-3.5" />}
              label="Publish"
              onClick={() => onAction("publish", branch)}
            />
          )}
          {!branch.isTrunk &&
            (branch.tracked ? (
              <>
                <ActionBtn
                  icon={<Layers className="h-3.5 w-3.5" />}
                  label="Restack"
                  onClick={() => onAction("restack", branch)}
                />
                <ActionBtn
                  icon={<GitFork className="h-3.5 w-3.5" />}
                  label="Set parent"
                  onClick={() => onAction("set-parent", branch)}
                />
                <ActionBtn
                  icon={<Unlink className="h-3.5 w-3.5" />}
                  label="Untrack"
                  onClick={() => onAction("untrack", branch)}
                />
              </>
            ) : (
              <ActionBtn
                icon={<LinkIcon className="h-3.5 w-3.5" />}
                label="Track"
                onClick={() => onAction("track", branch)}
              />
            ))}
        </div>
      </div>
    </aside>
  );
}
