import { useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  Sparkles,
  ExternalLink,
  GitPullRequest,
  GitMerge,
  ShieldCheck,
  Loader2,
  Check,
  X,
  MessageSquare,
} from "lucide-react";
import { safeOpen } from "../lib/safeOpen";
import type { CheckRun, PrDetail, PrReview } from "../lib/types";
import { api, errorText } from "../lib/api";
import { CommentList } from "./CommentList";
import {
  DiffExplorer,
  DiffViewToggle,
  loadDiffViewMode,
  saveDiffViewMode,
  splitDiffByFile,
  type DiffViewMode,
} from "./DiffExplorer";

function stateColor(s: string): string {
  if (s === "MERGED") return "text-purple-300";
  if (s === "CLOSED") return "text-red-300";
  return "text-emerald-300";
}
function ciColor(c: string): string {
  if (c === "SUCCESS") return "text-emerald-400";
  if (c === "FAILURE") return "text-red-400";
  return "text-amber-400";
}
function sevBadge(sev: string): string {
  if (sev === "critical") return "border-red-700 bg-red-950/50 text-red-300";
  if (sev === "warning") return "border-amber-700 bg-amber-950/50 text-amber-300";
  return "border-neutral-700 bg-neutral-800 text-neutral-300";
}
function bucketColor(b: string): string {
  if (b === "pass") return "bg-emerald-400";
  if (b === "fail") return "bg-red-400";
  if (b === "pending") return "bg-amber-400";
  return "bg-neutral-500";
}

/** Full-width pull-request view: Files (tree + numbered/split diff) and Discussion tabs. */
export function PrPage({
  repoPath,
  number,
  aiName,
  onClose,
  onAnalyze,
}: {
  repoPath: string;
  number: number;
  /** Display name of the active AI engine (Ollama model, or "Claude"). */
  aiName: string;
  onClose: () => void;
  onAnalyze: (number: number, mode: string) => void;
}) {
  const [pr, setPr] = useState<PrDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<"files" | "discussion">("files");
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [view, setView] = useState<DiffViewMode>(loadDiffViewMode);
  const [review, setReview] = useState<PrReview | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);
  const [postingReview, setPostingReview] = useState(false);
  const [postReviewResult, setPostReviewResult] = useState<string | null>(null);
  const [postReviewError, setPostReviewError] = useState<string | null>(null);
  const [reviewBody, setReviewBody] = useState("");
  const [submittingReview, setSubmittingReview] = useState<string | null>(null);
  const [reviewActionError, setReviewActionError] = useState<string | null>(null);
  const [checks, setChecks] = useState<CheckRun[] | null>(null);
  const [loadingChecks, setLoadingChecks] = useState(false);
  const [checksError, setChecksError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setPr(null);
    setError(null);
    setTab("files");
    setSelectedFile(null);
    setReview(null);
    setReviewError(null);
    setReviewing(false);
    setPostReviewResult(null);
    setPostReviewError(null);
    setReviewBody("");
    setReviewActionError(null);
    setChecks(null);
    setChecksError(null);
    api
      .prDetail(repoPath, number)
      .then((d) => {
        if (!alive) return;
        setPr(d);
        setSelectedFile(d.files[0]?.path ?? null);
      })
      .catch((e) => alive && setError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [repoPath, number]);

  const fileChunks = useMemo(() => splitDiffByFile(pr?.diff ?? ""), [pr]);
  const treeFiles = useMemo(() => (pr?.files ?? []).map((f) => ({ path: f.path })), [pr]);

  async function runReview() {
    setTab("discussion");
    setReviewing(true);
    setReview(null);
    setReviewError(null);
    setPostReviewResult(null);
    setPostReviewError(null);
    try {
      setReview(await api.reviewPr(repoPath, number));
    } catch (e) {
      setReviewError(errorText(e));
    } finally {
      setReviewing(false);
    }
  }

  async function postReview() {
    if (!review) return;
    setPostingReview(true);
    setPostReviewResult(null);
    setPostReviewError(null);
    try {
      setPostReviewResult(
        await api.postReviewComments(repoPath, number, review.summary, review.findings)
      );
    } catch (e) {
      setPostReviewError(errorText(e));
    } finally {
      setPostingReview(false);
    }
  }

  async function doReview(event: "approve" | "request_changes" | "comment") {
    setSubmittingReview(event);
    setReviewActionError(null);
    try {
      setPr(await api.submitPrReview(repoPath, number, event, reviewBody.trim()));
      setReviewBody("");
    } catch (e) {
      setReviewActionError(errorText(e));
    } finally {
      setSubmittingReview(null);
    }
  }

  async function loadChecks() {
    setLoadingChecks(true);
    setChecksError(null);
    try {
      setChecks(await api.prChecks(repoPath, number));
    } catch (e) {
      setChecksError(errorText(e));
    } finally {
      setLoadingChecks(false);
    }
  }

  function changeView(v: DiffViewMode) {
    setView(v);
    saveDiffViewMode(v);
  }

  function openFinding(file: string) {
    if (!file) return;
    setSelectedFile(file);
    setTab("files");
  }

  const tabBtn = (id: "files" | "discussion", label: string) => (
    <button
      onClick={() => setTab(id)}
      className={`border-b-2 px-3 py-1.5 text-xs font-medium ${
        tab === id
          ? "border-indigo-500 text-neutral-100"
          : "border-transparent text-neutral-400 hover:text-neutral-200"
      }`}
    >
      {label}
    </button>
  );

  return (
    <div className="flex h-full w-full min-w-0 flex-col bg-neutral-950">
      {/* Header */}
      <div className="shrink-0 border-b border-neutral-800 px-4 pt-2">
        <div className="flex items-center gap-2">
          <button
            onClick={onClose}
            title="Retour à la liste (Échap)"
            className="rounded p-1 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
          >
            <ArrowLeft className="h-4 w-4" />
          </button>
          <GitPullRequest className="h-4 w-4 shrink-0 text-indigo-400" />
          <span className="font-mono text-sm text-neutral-100">#{number}</span>
          {pr && <span className={`shrink-0 text-xs ${stateColor(pr.state)}`}>{pr.state}</span>}
          <span className="truncate text-sm text-neutral-200">{pr?.title ?? ""}</span>
          {pr && (
            <button
              onClick={() => safeOpen(pr.url)}
              title="Ouvrir sur GitHub"
              className="ml-auto inline-flex shrink-0 items-center gap-1.5 rounded-md border border-neutral-700 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-800"
            >
              <ExternalLink className="h-3.5 w-3.5" /> Open
            </button>
          )}
        </div>

        {pr && (
          <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 pl-7 text-xs text-neutral-400">
            <span className="font-mono text-neutral-300">{pr.headRef}</span>
            <span>→</span>
            <span className="font-mono text-neutral-300">{pr.baseRef}</span>
            <span className="text-neutral-600">·</span>
            <span>{pr.author}</span>
            <span className="text-emerald-400">+{pr.additions}</span>
            <span className="text-red-400">−{pr.deletions}</span>
            {pr.reviewDecision && <span className="text-neutral-500">· {pr.reviewDecision}</span>}
            {pr.checks && <span className={ciColor(pr.checks)}>· CI {pr.checks}</span>}
          </div>
        )}

        {/* Actions */}
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <button
            onClick={() => onAnalyze(number, "summary")}
            title="Synthèse rapide"
            className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-indigo-500"
          >
            <Sparkles className="h-3.5 w-3.5" /> Summary
          </button>
          <button
            onClick={() => onAnalyze(number, "detailed")}
            title="Relecture détaillée"
            className="inline-flex items-center gap-1.5 rounded-md border border-indigo-600 px-2.5 py-1 text-xs font-medium text-indigo-300 hover:bg-indigo-950/40"
          >
            <Sparkles className="h-3.5 w-3.5" /> Detailed
          </button>
          <button
            onClick={runReview}
            disabled={reviewing}
            title="Relecture IA structurée (onglet Discussion)"
            className="inline-flex items-center gap-1.5 rounded-md border border-indigo-600 px-2.5 py-1 text-xs font-medium text-indigo-300 hover:bg-indigo-950/40 disabled:opacity-50"
          >
            {reviewing ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <ShieldCheck className="h-3.5 w-3.5" />
            )}
            AI Review
          </button>
          {pr?.state === "OPEN" && (
            <button
              onClick={() => onAnalyze(number, "merge")}
              title={`Assistant de merge (${aiName})`}
              className="inline-flex items-center gap-1.5 rounded-md border border-emerald-700 px-2.5 py-1 text-xs font-medium text-emerald-300 hover:bg-emerald-950/40"
            >
              <GitMerge className="h-3.5 w-3.5" /> Aide au merge
            </button>
          )}
          {tab === "files" && (
            <div className="ml-auto">
              <DiffViewToggle value={view} onChange={changeView} />
            </div>
          )}
        </div>

        {/* Tabs */}
        <div className="mt-2 flex items-center gap-1">
          {tabBtn("files", `Fichiers${pr ? ` (${pr.files.length})` : ""}`)}
          {tabBtn("discussion", "Discussion")}
        </div>
      </div>

      {/* Body */}
      {error && (
        <div className="m-4 rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-300">
          {error}
        </div>
      )}
      {!pr && !error && <p className="p-4 text-sm text-neutral-500">Chargement…</p>}

      {pr && tab === "files" && (
        <DiffExplorer
          files={treeFiles}
          diffByFile={fileChunks}
          selected={selectedFile}
          onSelect={setSelectedFile}
          view={view}
          findings={review?.findings ?? []}
        />
      )}

      {pr && tab === "discussion" && (
        <div className="min-h-0 flex-1 space-y-4 overflow-auto p-4">
          {/* AI Review */}
          {(reviewing || reviewError || review) && (
            <div>
              <h4 className="mb-1 flex items-center gap-1.5 text-xs uppercase tracking-wider text-neutral-500">
                <ShieldCheck className="h-3.5 w-3.5" /> AI Review
                {review && <span className="text-neutral-600">({review.findings.length})</span>}
              </h4>
              {reviewing && (
                <div className="flex items-center gap-2 rounded-md border border-neutral-800 bg-neutral-950/60 px-3 py-2 text-xs text-neutral-400">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" /> Relecture… (~30 s)
                </div>
              )}
              {reviewError && (
                <div className="rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-xs text-red-300">
                  {reviewError}
                </div>
              )}
              {review && !reviewing && (
                <div className="space-y-2">
                  {review.summary.trim() && (
                    <p className="whitespace-pre-wrap rounded-md border border-neutral-800 bg-neutral-950/60 p-2 text-xs text-neutral-300">
                      {review.summary}
                    </p>
                  )}
                  {review.findings.length === 0 ? (
                    <p className="text-xs text-neutral-500">Aucun problème détecté ✓</p>
                  ) : (
                    <ul className="space-y-1">
                      {review.findings.map((f, i) => (
                        <li key={i}>
                          <button
                            onClick={() => openFinding(f.file)}
                            title="Voir le fichier concerné"
                            className="w-full rounded-md border border-neutral-800 bg-neutral-950/40 p-2 text-left hover:border-neutral-700 hover:bg-neutral-900"
                          >
                            <div className="flex items-center gap-2">
                              <span
                                className={`shrink-0 rounded border px-1.5 text-[10px] font-semibold uppercase ${sevBadge(
                                  f.severity
                                )}`}
                              >
                                {f.severity}
                              </span>
                              <span className="truncate font-mono text-[11px] text-neutral-400">
                                {f.file}
                                {f.line != null ? `:${f.line}` : ""}
                              </span>
                            </div>
                            <div className="mt-1 text-xs font-medium text-neutral-200">
                              {f.title}
                            </div>
                            <div className="mt-0.5 whitespace-pre-wrap text-xs text-neutral-400">
                              {f.detail}
                            </div>
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                  <div className="space-y-1">
                    <button
                      onClick={postReview}
                      disabled={postingReview}
                      title="Poster cette relecture sur la PR : commentaires en ligne + résumé, en mode « commentaire »"
                      className="inline-flex items-center gap-1.5 rounded-md border border-indigo-600 px-2.5 py-1 text-xs font-medium text-indigo-300 hover:bg-indigo-950/40 disabled:opacity-50"
                    >
                      {postingReview ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <MessageSquare className="h-3.5 w-3.5" />
                      )}
                      Poster sur la PR
                    </button>
                    {postReviewResult && (
                      <div className="rounded-md border border-emerald-800 bg-emerald-950/40 px-2 py-1 text-[11px] text-emerald-300">
                        {postReviewResult}
                      </div>
                    )}
                    {postReviewError && (
                      <div className="rounded-md border border-red-900 bg-red-950/40 px-2 py-1 text-[11px] text-red-300">
                        {postReviewError}
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Human review */}
          {pr.state === "OPEN" && (
            <div className="space-y-2">
              <h4 className="text-xs uppercase tracking-wider text-neutral-500">Review</h4>
              <textarea
                value={reviewBody}
                onChange={(e) => setReviewBody(e.target.value)}
                rows={2}
                placeholder="Commentaire (requis pour « changements » / « commenter »)"
                className="w-full resize-y rounded-md border border-neutral-700 bg-neutral-950 px-2 py-1.5 text-xs text-neutral-100 outline-none focus:border-indigo-600"
              />
              {reviewActionError && (
                <div className="rounded-md border border-red-900 bg-red-950/40 px-2 py-1 text-[11px] text-red-300">
                  {reviewActionError}
                </div>
              )}
              <div className="flex flex-wrap gap-2">
                <button
                  onClick={() => doReview("approve")}
                  disabled={!!submittingReview}
                  className="inline-flex items-center gap-1.5 rounded-md border border-emerald-700 px-2.5 py-1 text-xs font-medium text-emerald-300 hover:bg-emerald-950/40 disabled:opacity-50"
                >
                  {submittingReview === "approve" ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Check className="h-3.5 w-3.5" />
                  )}
                  Approuver
                </button>
                <button
                  onClick={() => doReview("request_changes")}
                  disabled={!!submittingReview || !reviewBody.trim()}
                  title={!reviewBody.trim() ? "Ajoute un commentaire" : ""}
                  className="inline-flex items-center gap-1.5 rounded-md border border-amber-700 px-2.5 py-1 text-xs font-medium text-amber-300 hover:bg-amber-950/40 disabled:opacity-50"
                >
                  {submittingReview === "request_changes" ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <X className="h-3.5 w-3.5" />
                  )}
                  Changements
                </button>
                <button
                  onClick={() => doReview("comment")}
                  disabled={!!submittingReview || !reviewBody.trim()}
                  title={!reviewBody.trim() ? "Ajoute un commentaire" : ""}
                  className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2.5 py-1 text-xs font-medium text-neutral-300 hover:bg-neutral-800 disabled:opacity-50"
                >
                  {submittingReview === "comment" ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <MessageSquare className="h-3.5 w-3.5" />
                  )}
                  Commenter
                </button>
              </div>
            </div>
          )}

          {/* CI checks */}
          <div className="space-y-1.5">
            <button
              onClick={loadChecks}
              disabled={loadingChecks}
              className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2.5 py-1 text-xs text-neutral-300 hover:bg-neutral-800 disabled:opacity-50"
            >
              {loadingChecks ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <ShieldCheck className="h-3.5 w-3.5" />
              )}
              Checks CI
            </button>
            {checksError && (
              <div className="rounded-md border border-red-900 bg-red-950/40 px-2 py-1 text-[11px] text-red-300">
                {checksError}
              </div>
            )}
            {checks && checks.length === 0 && (
              <p className="text-xs text-neutral-500">Aucun check rapporté.</p>
            )}
            {checks && checks.length > 0 && (
              <ul className="space-y-1">
                {checks.map((c, i) => (
                  <li key={i} className="flex items-center gap-2 text-xs">
                    <span
                      className={`h-2 w-2 shrink-0 rounded-full ${bucketColor(c.bucket)}`}
                      title={c.state}
                    />
                    <span className="flex-1 truncate text-neutral-300">{c.name}</span>
                    {c.link && (
                      <button
                        onClick={() => safeOpen(c.link)}
                        title="Voir les logs sur GitHub"
                        className="shrink-0 text-neutral-500 hover:text-indigo-300"
                      >
                        <ExternalLink className="h-3 w-3" />
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>

          {/* Description */}
          {pr.body.trim() && (
            <div>
              <h4 className="mb-1 text-xs uppercase tracking-wider text-neutral-500">
                Description
              </h4>
              <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-md border border-neutral-800 bg-neutral-950/60 p-2 font-sans text-xs text-neutral-300">
                {pr.body}
              </pre>
            </div>
          )}

          <CommentList comments={pr.comments} reviews={pr.reviews} />

          {pr.commits.length > 0 && (
            <div>
              <h4 className="mb-1 text-xs uppercase tracking-wider text-neutral-500">
                Commits ({pr.commits.length})
              </h4>
              <ul className="space-y-0.5">
                {pr.commits.map((c, i) => (
                  <li key={i} className="truncate text-xs text-neutral-300">
                    • {c}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
