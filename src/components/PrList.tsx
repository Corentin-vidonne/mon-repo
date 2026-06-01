import { useEffect, useState } from "react";
import { GitPullRequest, GitMerge } from "lucide-react";
import type { PrSummary } from "../lib/types";
import { api, errorText } from "../lib/api";

const STATES = ["open", "merged", "closed", "all"] as const;
type PrState = (typeof STATES)[number];

function stateColor(s: string): string {
  if (s === "MERGED") return "text-purple-400";
  if (s === "CLOSED") return "text-red-400";
  return "text-emerald-400";
}
function ciColor(c: string): string {
  if (c === "SUCCESS") return "text-emerald-400";
  if (c === "FAILURE") return "text-red-400";
  return "text-amber-400";
}

export function PrList({
  repoPath,
  selected,
  onSelect,
}: {
  repoPath: string;
  selected: number | null;
  onSelect: (number: number) => void;
}) {
  const [state, setState] = useState<PrState>("open");
  const [prs, setPrs] = useState<PrSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setPrs(null);
    setError(null);
    api
      .listPullRequests(repoPath, state)
      .then((d) => alive && setPrs(d))
      .catch((e) => alive && setError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [repoPath, state]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-1 border-b border-neutral-800 px-4 py-2">
        {STATES.map((s) => (
          <button
            key={s}
            onClick={() => setState(s)}
            className={`rounded px-2 py-1 text-xs capitalize ${
              state === s
                ? "bg-neutral-800 text-neutral-100"
                : "text-neutral-400 hover:bg-neutral-900"
            }`}
          >
            {s}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-auto p-3">
        {error && (
          <div className="rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-300">
            {error}
          </div>
        )}
        {!prs && !error && <p className="text-sm text-neutral-500">Loading…</p>}
        {prs && prs.length === 0 && (
          <p className="text-sm text-neutral-600">No {state === "all" ? "" : state} pull requests.</p>
        )}

        <div className="mx-auto max-w-3xl space-y-1">
          {prs?.map((p) => (
            <button
              key={p.number}
              onClick={() => onSelect(p.number)}
              className={`flex w-full items-center gap-2 rounded-md px-3 py-2 text-left ${
                selected === p.number
                  ? "bg-indigo-950/50 ring-1 ring-indigo-700/60"
                  : "hover:bg-neutral-900"
              }`}
            >
              {p.state === "MERGED" ? (
                <GitMerge className={`h-4 w-4 shrink-0 ${stateColor(p.state)}`} />
              ) : (
                <GitPullRequest className={`h-4 w-4 shrink-0 ${stateColor(p.state)}`} />
              )}
              <span className="truncate text-sm text-neutral-200">{p.title}</span>
              {p.isDraft && (
                <span className="shrink-0 rounded-full border border-neutral-700 px-1.5 text-[10px] text-neutral-400">
                  draft
                </span>
              )}
              <span className="ml-auto flex shrink-0 items-center gap-2 text-xs text-neutral-500">
                <span className="hidden font-mono text-neutral-600 sm:inline">
                  {p.headRef} → {p.baseRef}
                </span>
                {p.checks && <span className={ciColor(p.checks)}>●</span>}
                <span className="font-mono">#{p.number}</span>
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
