import { useEffect, useMemo, useState } from "react";
import { X, Sparkles } from "lucide-react";
import type { CommitDetail, CommitNode } from "../lib/types";
import { api } from "../lib/api";

function statusColor(s: string): string {
  if (s.startsWith("A")) return "text-emerald-400";
  if (s.startsWith("D")) return "text-red-400";
  if (s.startsWith("R")) return "text-blue-400";
  return "text-amber-400";
}

/** Split a unified diff into per-file chunks, keyed by the (b/) path. */
function splitDiffByFile(diff: string): Record<string, string> {
  const out: Record<string, string> = {};
  if (!diff) return out;
  let current: string | null = null;
  let buf: string[] = [];
  const flush = () => {
    if (current) out[current] = buf.join("\n");
  };
  for (const line of diff.split("\n")) {
    if (line.startsWith("diff --git ")) {
      flush();
      const m = line.match(/ b\/(.+)$/);
      current = m ? m[1] : line;
      buf = [];
    }
    buf.push(line);
  }
  flush();
  return out;
}

function DiffView({ text }: { text: string }) {
  const lines = text.split("\n");
  return (
    <pre className="overflow-x-auto rounded-md border border-neutral-800 bg-neutral-950 p-2 font-mono text-[11px] leading-relaxed">
      {lines.map((l, i) => {
        let cls = "text-neutral-400";
        if (l.startsWith("@@")) cls = "text-cyan-300";
        else if (l.startsWith("+++") || l.startsWith("---")) cls = "text-neutral-500";
        else if (l.startsWith("diff ") || l.startsWith("index ")) cls = "text-neutral-600";
        else if (l.startsWith("+")) cls = "bg-emerald-950/30 text-emerald-300";
        else if (l.startsWith("-")) cls = "bg-red-950/30 text-red-300";
        return (
          <div key={i} className={`whitespace-pre ${cls}`}>
            {l || " "}
          </div>
        );
      })}
    </pre>
  );
}

export function CommitDetailPanel({
  repoPath,
  node,
  onClose,
  onAnalyze,
}: {
  repoPath: string;
  node: CommitNode;
  onClose: () => void;
  onAnalyze: (sha: string, mode: string) => void;
}) {
  const [detail, setDetail] = useState<CommitDetail | null>(null);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setDetail(null);
    setSelectedFile(null);
    api
      .commitDetail(repoPath, node.sha)
      .then((d) => alive && setDetail(d))
      .catch(() => alive && setDetail({ message: node.subject, files: [], diff: "" }));
    return () => {
      alive = false;
    };
  }, [repoPath, node.sha]);

  const fileChunks = useMemo(() => splitDiffByFile(detail?.diff ?? ""), [detail]);
  const shownDiff = selectedFile
    ? fileChunks[selectedFile] ?? "(no diff for this file)"
    : detail?.diff ?? "";

  return (
    <aside className="flex h-full w-full flex-col border-l border-neutral-800 bg-neutral-900/40">
      <div className="flex h-14 items-center gap-2 border-b border-neutral-800 px-4">
        <span className="font-mono text-sm text-amber-300">{node.shortSha}</span>
        <span className="text-xs text-neutral-400">commit</span>
        <button
          onClick={onClose}
          className="ml-auto rounded p-1 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="flex-1 space-y-4 overflow-auto p-4">
        <div>
          <div className="mb-1.5 flex items-center gap-1.5 text-xs text-neutral-400">
            <Sparkles className="h-3.5 w-3.5 text-indigo-400" /> Analyze with Claude
          </div>
          <div className="flex gap-2">
            <button
              onClick={() => onAnalyze(node.sha, "summary")}
              title="Quick synthesis (5-8 lines)"
              className="flex-1 rounded-md bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500"
            >
              Summary
            </button>
            <button
              onClick={() => onAnalyze(node.sha, "detailed")}
              title="In-depth review (per-file, intent, risks, suggestions)"
              className="flex-1 rounded-md border border-indigo-600 px-3 py-1.5 text-sm font-medium text-indigo-300 hover:bg-indigo-950/40"
            >
              Detailed
            </button>
          </div>
        </div>

        {node.refs.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {node.refs.map((r) => (
              <span
                key={r}
                className="rounded-full bg-indigo-900/60 px-2 py-0.5 text-[10px] text-indigo-200"
              >
                {r}
              </span>
            ))}
          </div>
        )}

        <pre className="whitespace-pre-wrap break-words font-sans text-sm text-neutral-200">
          {detail ? detail.message : node.subject}
        </pre>

        <div className="space-y-1 text-xs text-neutral-500">
          <div>
            {node.author} · {node.date}
          </div>
          <div>
            parent{node.parents.length > 1 ? "s" : ""}:{" "}
            <span className="font-mono">
              {node.parents.map((p) => p.slice(0, 7)).join(", ") || "—"}
            </span>
          </div>
        </div>

        <div>
          <h4 className="mb-1 text-xs uppercase tracking-wider text-neutral-500">
            Files{detail ? ` (${detail.files.length})` : ""}
          </h4>
          {detail === null && <p className="text-xs text-neutral-600">Loading…</p>}
          {detail && detail.files.length === 0 && (
            <p className="text-xs text-neutral-600">No file changes.</p>
          )}
          <ul className="space-y-0.5">
            {detail?.files.map((f, i) => {
              const active = selectedFile === f.path;
              return (
                <li key={`${f.path}-${i}`}>
                  <button
                    onClick={() => setSelectedFile(active ? null : f.path)}
                    title="Show only this file's changes"
                    className={`flex w-full items-center gap-2 rounded px-1.5 py-0.5 text-left text-xs ${
                      active ? "bg-neutral-800" : "hover:bg-neutral-800/60"
                    }`}
                  >
                    <span className={`w-6 shrink-0 font-mono ${statusColor(f.status)}`}>
                      {f.status}
                    </span>
                    <span className="truncate font-mono text-neutral-300">{f.path}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </div>

        {detail && shownDiff.trim().length > 0 && (
          <div>
            <div className="mb-1 flex items-center gap-2">
              <h4 className="text-xs uppercase tracking-wider text-neutral-500">
                {selectedFile ? "File changes" : "Changes"}
              </h4>
              {selectedFile && (
                <button
                  onClick={() => setSelectedFile(null)}
                  className="rounded px-1.5 text-[10px] text-indigo-300 hover:bg-neutral-800"
                >
                  show all
                </button>
              )}
            </div>
            <DiffView text={shownDiff} />
          </div>
        )}
      </div>
    </aside>
  );
}
