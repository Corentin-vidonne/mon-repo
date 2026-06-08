import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Sparkles, ShieldCheck, Loader2, GitBranch } from "lucide-react";
import type { CommitDetail, CommitNode, PrReview } from "../lib/types";
import { api, errorText } from "../lib/api";
import {
  DiffExplorer,
  DiffViewToggle,
  loadDiffViewMode,
  saveDiffViewMode,
  splitDiffByFile,
  type DiffViewMode,
} from "./DiffExplorer";

/** Full-width commit view: file tree + numbered diff (unified or side-by-side). */
export function CommitPage({
  repoPath,
  node,
  branches,
  aiName,
  onClose,
  onAnalyze,
  onCherryPick,
}: {
  repoPath: string;
  node: CommitNode;
  branches: string[];
  /** Display name of the active AI engine (Ollama model, or "Claude"). */
  aiName: string;
  onClose: () => void;
  onAnalyze: (sha: string, mode: string) => void;
  onCherryPick: (sha: string, target: string) => void;
}) {
  const [detail, setDetail] = useState<CommitDetail | null>(null);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [view, setView] = useState<DiffViewMode>(loadDiffViewMode);
  const [review, setReview] = useState<PrReview | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);
  const [cpTarget, setCpTarget] = useState<string>("");

  useEffect(() => {
    let alive = true;
    setDetail(null);
    setSelectedFile(null);
    setReview(null);
    setReviewError(null);
    setReviewing(false);
    api
      .commitDetail(repoPath, node.sha)
      .then((d) => {
        if (!alive) return;
        setDetail(d);
        setSelectedFile(d.files[0]?.path ?? null);
      })
      .catch(() => alive && setDetail({ message: node.subject, files: [], diff: "" }));
    return () => {
      alive = false;
    };
  }, [repoPath, node.sha]);

  const fileChunks = useMemo(() => splitDiffByFile(detail?.diff ?? ""), [detail]);

  async function runReview() {
    setReviewing(true);
    setReview(null);
    setReviewError(null);
    try {
      const r = await api.reviewCommit(repoPath, node.sha);
      setReview(r);
      // Jump to the first file that has a finding, so the annotations are visible at once.
      const first = detail?.files.find((f) => r.findings.some((fd) => fd.file === f.path));
      if (first) setSelectedFile(first.path);
    } catch (e) {
      setReviewError(errorText(e));
    } finally {
      setReviewing(false);
    }
  }

  function changeView(v: DiffViewMode) {
    setView(v);
    saveDiffViewMode(v);
  }

  const cpBranches = branches.filter((b) => !node.refs.includes(b));

  return (
    <div className="flex h-full w-full min-w-0 flex-col bg-neutral-950">
      {/* Header */}
      <div className="shrink-0 border-b border-neutral-800 px-4 py-2">
        <div className="flex items-center gap-2">
          <button
            onClick={onClose}
            title="Retour au graphe (Échap)"
            className="rounded p-1 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
          >
            <ArrowLeft className="h-4 w-4" />
          </button>
          <span className="font-mono text-sm text-amber-300">{node.shortSha}</span>
          <span className="truncate text-sm text-neutral-200">{node.subject}</span>
          <span className="ml-auto shrink-0 text-xs text-neutral-500">
            {node.author} · {node.date}
          </span>
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <button
            onClick={() => onAnalyze(node.sha, "summary")}
            title={`Synthèse rapide (${aiName})`}
            className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-indigo-500"
          >
            <Sparkles className="h-3.5 w-3.5" /> Summary
          </button>
          <button
            onClick={() => onAnalyze(node.sha, "detailed")}
            title="Analyse détaillée"
            className="inline-flex items-center gap-1.5 rounded-md border border-indigo-600 px-2.5 py-1 text-xs font-medium text-indigo-300 hover:bg-indigo-950/40"
          >
            <Sparkles className="h-3.5 w-3.5" /> Detailed
          </button>
          <button
            onClick={runReview}
            disabled={reviewing}
            title="Relecture IA structurée"
            className="inline-flex items-center gap-1.5 rounded-md border border-indigo-600 px-2.5 py-1 text-xs font-medium text-indigo-300 hover:bg-indigo-950/40 disabled:opacity-50"
          >
            {reviewing ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <ShieldCheck className="h-3.5 w-3.5" />
            )}
            AI Review
          </button>
          <div className="ml-auto flex items-center gap-2">
            <DiffViewToggle value={view} onChange={changeView} />
            {cpBranches.length > 0 && (
              <div className="flex items-center gap-1.5">
                <GitBranch className="h-3.5 w-3.5 text-neutral-500" />
                <select
                  value={cpTarget || cpBranches[0]}
                  onChange={(e) => setCpTarget(e.target.value)}
                  className="max-w-[11rem] rounded-md border border-neutral-700 bg-neutral-950 px-2 py-1 text-xs text-neutral-100 outline-none focus:border-indigo-600"
                >
                  {cpBranches.map((b) => (
                    <option key={b} value={b}>
                      {b}
                    </option>
                  ))}
                </select>
                <button
                  onClick={() => onCherryPick(node.sha, cpTarget || cpBranches[0])}
                  title="Appliquer ce commit sur la branche choisie"
                  className="shrink-0 rounded-md border border-emerald-700 px-2.5 py-1 text-xs font-medium text-emerald-300 hover:bg-emerald-950/40"
                >
                  Cherry-pick
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* AI review summary — findings are annotated inline in the diff + badged in the tree. */}
      {(reviewing || reviewError || review) && (
        <div className="shrink-0 border-b border-neutral-800 bg-neutral-900/30 px-4 py-2 text-xs">
          {reviewing && (
            <div className="flex items-center gap-2 text-neutral-400">
              <Loader2 className="h-3.5 w-3.5 animate-spin" /> Relecture… (~30 s)
            </div>
          )}
          {reviewError && <div className="text-red-300">{reviewError}</div>}
          {review && !reviewing && (
            <div className="flex items-start gap-2">
              <ShieldCheck className="mt-0.5 h-3.5 w-3.5 shrink-0 text-indigo-400" />
              <div className="min-w-0">
                {review.summary.trim() && (
                  <p className="whitespace-pre-wrap text-neutral-300">{review.summary}</p>
                )}
                <p className="mt-0.5 text-neutral-500">
                  {review.findings.length === 0
                    ? "Aucun problème détecté ✓"
                    : `${review.findings.length} remarque(s) — annotées dans le diff, badges dans l'arborescence.`}
                </p>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Commit message (full width) */}
      {detail && detail.message.trim() && (
        <pre className="max-h-40 shrink-0 overflow-auto whitespace-pre-wrap break-words border-b border-neutral-800 px-4 py-3 font-sans text-sm text-neutral-200">
          {detail.message}
        </pre>
      )}

      {detail === null ? (
        <p className="p-4 text-sm text-neutral-500">Chargement…</p>
      ) : (
        <DiffExplorer
          files={detail.files}
          diffByFile={fileChunks}
          selected={selectedFile}
          onSelect={setSelectedFile}
          view={view}
          findings={review?.findings ?? []}
        />
      )}
    </div>
  );
}
