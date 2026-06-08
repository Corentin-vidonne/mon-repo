import { useEffect, useState } from "react";
import { X, ExternalLink, CircleDot, CircleCheck } from "lucide-react";
import { safeOpen } from "../lib/safeOpen";
import type { IssueDetail } from "../lib/types";
import { api, errorText } from "../lib/api";
import { CommentList } from "./CommentList";

export function IssueDetailPanel({
  repoPath,
  number,
  onClose,
}: {
  repoPath: string;
  number: number;
  onClose: () => void;
}) {
  const [issue, setIssue] = useState<IssueDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setIssue(null);
    setError(null);
    api
      .issueDetail(repoPath, number)
      .then((d) => alive && setIssue(d))
      .catch((e) => alive && setError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [repoPath, number]);

  const open = issue?.state === "OPEN";

  return (
    <aside className="flex h-full w-full flex-col border-l border-neutral-800 bg-neutral-900/40">
      <div className="flex h-14 items-center gap-2 border-b border-neutral-800 px-4">
        {open ? (
          <CircleDot className="h-4 w-4 text-emerald-400" />
        ) : (
          <CircleCheck className="h-4 w-4 text-purple-400" />
        )}
        <span className="font-mono text-sm text-neutral-100">#{number}</span>
        {issue && (
          <span className={`text-xs ${open ? "text-emerald-300" : "text-purple-300"}`}>
            {issue.state}
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
        {error && (
          <div className="rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-300">
            {error}
          </div>
        )}
        {!issue && !error && <p className="text-sm text-neutral-500">Loading…</p>}

        {issue && (
          <>
            <h3 className="text-sm font-semibold text-neutral-100">{issue.title}</h3>

            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs text-neutral-400">by {issue.author}</span>
              {issue.labels.map((l) => (
                <span
                  key={l}
                  className="rounded-full border border-neutral-700 px-2 py-0.5 text-[10px] text-neutral-300"
                >
                  {l}
                </span>
              ))}
              <button
                onClick={() => safeOpen(issue.url)}
                className="ml-auto inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2.5 py-1 text-xs text-neutral-300 hover:bg-neutral-800"
              >
                <ExternalLink className="h-3.5 w-3.5" /> Open
              </button>
            </div>

            {issue.body.trim() && (
              <pre className="whitespace-pre-wrap break-words rounded-md border border-neutral-800 bg-neutral-950/60 p-2 font-sans text-xs text-neutral-300">
                {issue.body}
              </pre>
            )}

            <CommentList comments={issue.comments} />
          </>
        )}
      </div>
    </aside>
  );
}
