import { useEffect, useState } from "react";
import { CircleDot, CircleCheck, MessageSquare } from "lucide-react";
import type { IssueSummary } from "../lib/types";
import { api, errorText } from "../lib/api";

const STATES = ["open", "closed", "all"] as const;
type IssueState = (typeof STATES)[number];

export function IssuesList({
  repoPath,
  selected,
  onSelect,
}: {
  repoPath: string;
  selected: number | null;
  onSelect: (number: number) => void;
}) {
  const [state, setState] = useState<IssueState>("open");
  const [issues, setIssues] = useState<IssueSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setIssues(null);
    setError(null);
    api
      .listIssues(repoPath, state)
      .then((d) => alive && setIssues(d))
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
        {!issues && !error && <p className="text-sm text-neutral-500">Loading…</p>}
        {issues && issues.length === 0 && (
          <p className="text-sm text-neutral-600">No {state === "all" ? "" : state} issues.</p>
        )}

        <div className="mx-auto max-w-3xl space-y-1">
          {issues?.map((i) => {
            const open = i.state === "OPEN";
            return (
              <button
                key={i.number}
                onClick={() => onSelect(i.number)}
                className={`flex w-full items-center gap-2 rounded-md px-3 py-2 text-left ${
                  selected === i.number
                    ? "bg-indigo-950/50 ring-1 ring-indigo-700/60"
                    : "hover:bg-neutral-900"
                }`}
              >
                {open ? (
                  <CircleDot className="h-4 w-4 shrink-0 text-emerald-400" />
                ) : (
                  <CircleCheck className="h-4 w-4 shrink-0 text-purple-400" />
                )}
                <span className="truncate text-sm text-neutral-200">{i.title}</span>
                {i.labels.slice(0, 3).map((l) => (
                  <span
                    key={l}
                    className="hidden shrink-0 rounded-full border border-neutral-700 px-1.5 text-[10px] text-neutral-400 sm:inline"
                  >
                    {l}
                  </span>
                ))}
                <span className="ml-auto flex shrink-0 items-center gap-2 text-xs text-neutral-500">
                  {i.commentCount > 0 && (
                    <span className="inline-flex items-center gap-0.5">
                      <MessageSquare className="h-3 w-3" />
                      {i.commentCount}
                    </span>
                  )}
                  <span className="font-mono">#{i.number}</span>
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
