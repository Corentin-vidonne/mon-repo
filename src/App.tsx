import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import {
  RefreshCw,
  FolderPlus,
  GitBranch,
  Plus,
  Layers,
  GitPullRequest,
  Network,
  ListTree,
  Waypoints,
  DownloadCloud,
  Boxes,
  CircleDot,
  Bell,
  Code2,
  FileText,
} from "lucide-react";
import { api, errorText } from "./lib/api";
import { notify as sendDesktopNotification } from "./lib/notify";
import type {
  Branch,
  BranchActionKind,
  CommitNode,
  Health,
  RepoView,
  StackNode,
  UpdateItem,
} from "./lib/types";
import { StackTree } from "./components/StackTree";
import { StackGraph } from "./components/StackGraph";
import { CommitGraph } from "./components/CommitGraph";
import { CommitFilter } from "./components/CommitFilter";
import { BranchRow } from "./components/BranchRow";
import { BranchDetail } from "./components/BranchDetail";
import { CommitDetailPanel } from "./components/CommitDetailPanel";
import { NewBranchDialog, SetParentDialog } from "./components/BranchDialogs";
import { ConflictPanel } from "./components/ConflictPanel";
import { SubmitDialog } from "./components/SubmitDialog";
import { RepoGraphView } from "./components/RepoGraphView";
import { TerminalDock, type AnalyzeTarget } from "./components/TerminalDock";
import { PrDetailPanel } from "./components/PrDetailPanel";
import { IssuesList } from "./components/IssuesList";
import { IssueDetailPanel } from "./components/IssueDetailPanel";
import { PrList } from "./components/PrList";
import { DocsView } from "./components/DocsView";
import { AddRepoDialog } from "./components/AddRepoDialog";

const REPOS_KEY = "gitui.repos";

function loadRepos(): string[] {
  try {
    const raw = localStorage.getItem(REPOS_KEY);
    return raw ? (JSON.parse(raw) as string[]) : [];
  } catch {
    return [];
  }
}
function saveRepos(repos: string[]) {
  localStorage.setItem(REPOS_KEY, JSON.stringify(repos));
}
function repoName(path: string): string {
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}
function flattenBranches(view: RepoView): Branch[] {
  const out: Branch[] = [];
  const walk = (n: StackNode) => {
    out.push(n.branch);
    n.children.forEach(walk);
  };
  view.roots.forEach(walk);
  return [...out, ...view.untracked];
}

type DialogState =
  | { type: "new"; parent: string }
  | { type: "parent"; branch: Branch };
type ViewMode = "graph" | "commits" | "tree" | "prs" | "issues" | "docs";

export default function App() {
  const [repos, setRepos] = useState<string[]>(loadRepos);
  const [selected, setSelected] = useState<string | null>(repos[0] ?? null);
  const [view, setView] = useState<RepoView | null>(null);
  const [health, setHealth] = useState<Health | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>("graph");
  const [inspect, setInspect] = useState<string | null>(null);
  const [commits, setCommits] = useState<CommitNode[]>([]);
  const [inspectCommit, setInspectCommit] = useState<string | null>(null);
  const [commitFilter, setCommitFilter] = useState<string[] | null>(null);
  const [panelWidth, setPanelWidth] = useState(460);
  const [showSubmit, setShowSubmit] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [workspace, setWorkspace] = useState(false);
  const [terminal, setTerminal] = useState<{
    repoPath: string;
    target: AnalyzeTarget;
    mode: string;
  } | null>(null);
  const [inspectPr, setInspectPr] = useState<number | null>(null);
  const [inspectIssue, setInspectIssue] = useState<number | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [updates, setUpdates] = useState<Record<string, UpdateItem[]>>({});
  const notifiedKeys = useRef<Set<string>>(new Set());
  const totalUpdates = Object.values(updates).reduce((n, a) => n + a.length, 0);

  function notify(msg: string) {
    setToast(msg);
    window.setTimeout(() => setToast(null), 3000);
  }

  function registerRepo(v: RepoView) {
    setRepos((prev) => {
      const next = prev.includes(v.repoRoot) ? prev : [...prev, v.repoRoot];
      saveRepos(next);
      return next;
    });
    setSelected(v.repoRoot);
    setView(v);
    setWorkspace(false);
    setShowAdd(false);
  }

  function startResize(e: ReactMouseEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startW = panelWidth;
    const onMove = (ev: MouseEvent) =>
      setPanelWidth(Math.min(1000, Math.max(300, startW - (ev.clientX - startX))));
    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.userSelect = "";
    };
    document.body.style.userSelect = "none";
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }

  useEffect(() => {
    api.health().then(setHealth).catch(() => {});
  }, []);

  const refresh = useCallback(async (path: string | null) => {
    if (!path) {
      setView(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setView(await api.getRepoView(path));
    } catch (e) {
      setView(null);
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    setInspect(null);
    setInspectCommit(null);
    setInspectPr(null);
    setInspectIssue(null);
    setCommitFilter(null); // reset branch filter when switching repo
    refresh(selected);
  }, [selected, refresh]);

  // Load the commit DAG when the commits view is active (and refresh after mutations).
  useEffect(() => {
    if (!selected || viewMode !== "commits") return;
    let alive = true;
    api
      .stackCommits(selected, commitFilter)
      .then((c) => alive && setCommits(c))
      .catch(() => alive && setCommits([]));
    return () => {
      alive = false;
    };
  }, [selected, viewMode, view, commitFilter]);

  const runMutation = useCallback(async (p: Promise<RepoView>): Promise<boolean> => {
    setLoading(true);
    setError(null);
    try {
      setView(await p);
      return true;
    } catch (e) {
      setError(errorText(e));
      return false;
    } finally {
      setLoading(false);
      setDialog(null);
    }
  }, []);

  function removeRepo(path: string) {
    setRepos((prev) => {
      const next = prev.filter((p) => p !== path);
      saveRepos(next);
      if (selected === path) setSelected(next[0] ?? null);
      return next;
    });
  }

  // Poll every added repo for new activity; fire a desktop notification once per
  // distinct change (deduped by repo+key), and keep per-repo unseen counts.
  const checkAllUpdates = useCallback(async () => {
    const entries = await Promise.all(
      repos.map(async (p) => {
        try {
          const r = await api.checkUpdates(p);
          return [p, r.items] as const;
        } catch {
          return [p, [] as UpdateItem[]] as const;
        }
      })
    );
    const map: Record<string, UpdateItem[]> = {};
    for (const [p, items] of entries) if (items.length) map[p] = items;
    setUpdates(map);

    for (const [p, items] of entries) {
      const fresh = items.filter((it) => !notifiedKeys.current.has(`${p}::${it.key}`));
      fresh.forEach((it) => notifiedKeys.current.add(`${p}::${it.key}`));
      if (fresh.length === 0) continue;
      const name = repoName(p);
      if (fresh.length <= 3) {
        for (const it of fresh) await sendDesktopNotification(name, it.detail);
      } else {
        await sendDesktopNotification(name, `${fresh.length} new updates`);
      }
    }
  }, [repos]);

  useEffect(() => {
    if (repos.length === 0) return;
    checkAllUpdates();
    const id = window.setInterval(checkAllUpdates, 180_000); // every 3 min
    return () => window.clearInterval(id);
  }, [checkAllUpdates]);

  // Open a repo and clear its update indicator (records current state as seen).
  const openRepo = useCallback((p: string) => {
    setSelected(p);
    setWorkspace(false);
    setUpdates((u) => {
      if (!u[p]) return u;
      const { [p]: _drop, ...rest } = u;
      return rest;
    });
    api.markUpdatesSeen(p).catch(() => {});
  }, []);

  function onAction(kind: BranchActionKind, branch: Branch) {
    if (!selected) return;
    if (kind === "new-child") setDialog({ type: "new", parent: branch.name });
    else if (kind === "untrack") runMutation(api.untrackBranch(selected, branch.name));
    else if (kind === "restack") runMutation(api.restack(selected, branch.name));
    else if (kind === "checkout") runMutation(api.checkout(selected, branch.name));
    else if (kind === "publish") runMutation(api.publishBranch(selected, branch.name));
    else setDialog({ type: "parent", branch });
  }

  const inspectedBranch =
    view && inspect ? flattenBranches(view).find((b) => b.name === inspect) ?? null : null;
  const selectedCommit =
    inspectCommit ? commits.find((c) => c.sha === inspectCommit) ?? null : null;

  const panel =
    selected &&
    (inspectPr != null ? (
      <PrDetailPanel
        repoPath={selected}
        number={inspectPr}
        onClose={() => setInspectPr(null)}
        onAnalyze={(number, mode) =>
          setTerminal({ repoPath: selected!, target: { kind: "pr", number }, mode })
        }
      />
    ) : viewMode === "issues" ? (
      inspectIssue != null && (
        <IssueDetailPanel
          repoPath={selected}
          number={inspectIssue}
          onClose={() => setInspectIssue(null)}
        />
      )
    ) : viewMode === "commits" ? (
      selectedCommit && (
        <CommitDetailPanel
          repoPath={selected}
          node={selectedCommit}
          onClose={() => setInspectCommit(null)}
          onAnalyze={(sha, mode) =>
            setTerminal({ repoPath: selected!, target: { kind: "commit", sha }, mode })
          }
        />
      )
    ) : viewMode === "graph" || viewMode === "tree" ? (
      inspectedBranch && (
        <BranchDetail
          repoPath={selected}
          branch={inspectedBranch}
          onAction={onAction}
          onClose={() => setInspect(null)}
          onOpenPr={(n) => setInspectPr(n)}
        />
      )
    ) : null);

  function switchView(mode: ViewMode) {
    // Clear cross-view selections so the detail panel matches the active view.
    if (mode !== "prs") setInspectPr(null);
    if (mode !== "issues") setInspectIssue(null);
    setViewMode(mode);
  }

  const toggle = (mode: ViewMode, label: string, icon: ReactNode) => (
    <button
      title={label}
      onClick={() => switchView(mode)}
      className={`rounded p-1 ${
        viewMode === mode
          ? "bg-neutral-700 text-neutral-100"
          : "text-neutral-400 hover:text-neutral-200"
      }`}
    >
      {icon}
    </button>
  );

  return (
    <div className="flex h-screen w-screen bg-neutral-950 text-neutral-200">
      {/* Sidebar */}
      <aside className="flex w-64 shrink-0 flex-col border-r border-neutral-800 bg-neutral-900/40">
        <div className="flex h-14 items-center gap-2 border-b border-neutral-800 px-4">
          <GitBranch className="h-5 w-5 text-indigo-400" />
          <span className="font-semibold tracking-tight">gitui</span>
          <span className="text-xs text-neutral-500">stacked PRs</span>
        </div>

        <button
          onClick={() => setWorkspace(true)}
          disabled={repos.length === 0}
          className={`mx-2 mt-2 flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm disabled:opacity-40 ${
            workspace
              ? "bg-neutral-800 text-neutral-100"
              : "text-neutral-300 hover:bg-neutral-900"
          }`}
        >
          <Boxes className="h-4 w-4 text-indigo-400" />
          Workspace
          <span className="ml-auto text-xs text-neutral-500">{repos.length}</span>
        </button>
        <div className="flex items-center justify-between px-4 py-3">
          <span className="text-xs uppercase tracking-wider text-neutral-500">
            Repositories
          </span>
          <button
            onClick={() => setShowAdd(true)}
            title="Add repository"
            className="rounded p-1 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
          >
            <FolderPlus className="h-4 w-4" />
          </button>
        </div>

        <div className="flex-1 overflow-auto px-2">
          {repos.length === 0 && (
            <div className="px-2 text-sm text-neutral-600">No repository yet.</div>
          )}
          {repos.map((p) => (
            <button
              key={p}
              onClick={() => openRepo(p)}
              title={p}
              className={`group flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm ${
                selected === p && !workspace
                  ? "bg-neutral-800 text-neutral-100"
                  : "text-neutral-400 hover:bg-neutral-900"
              }`}
            >
              <span className="truncate">{repoName(p)}</span>
              <span className="ml-auto flex items-center gap-1">
                {updates[p]?.length ? (
                  <span
                    title={`${updates[p].length} new update(s)`}
                    className="rounded-full bg-indigo-600 px-1.5 text-[10px] font-semibold text-white"
                  >
                    {updates[p].length}
                  </span>
                ) : null}
                <span
                  onClick={(e) => {
                    e.stopPropagation();
                    removeRepo(p);
                  }}
                  className="hidden text-neutral-600 hover:text-rose-400 group-hover:block"
                >
                  ✕
                </span>
              </span>
            </button>
          ))}
        </div>

        <div className="border-t border-neutral-800 px-4 py-2 text-xs">
          {health?.ghAuthenticated ? (
            <span className="text-emerald-400">● {health.ghAccount ?? "gh"}</span>
          ) : (
            <span className="text-neutral-500">gh: not logged in</span>
          )}
        </div>
      </aside>

      {/* Main */}
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <header className="flex h-14 shrink-0 items-center gap-3 border-b border-neutral-800 px-6">
          {workspace ? (
            <h1 className="text-sm font-medium text-neutral-200">
              Workspace — repository links
            </h1>
          ) : view ? (
            <>
              <h1 className="text-sm font-medium text-neutral-200">{view.name}</h1>
              <span className="rounded bg-neutral-800 px-2 py-0.5 font-mono text-xs text-neutral-400">
                trunk: {view.trunk}
              </span>
              {!view.prsAvailable && (
                <span className="text-xs text-neutral-600">PRs unavailable</span>
              )}
              <div className="ml-auto flex items-center gap-2">
                <button
                  onClick={() => checkAllUpdates()}
                  title={
                    totalUpdates > 0
                      ? `${totalUpdates} new update(s) across repos`
                      : "Check for updates"
                  }
                  className="relative rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
                >
                  <Bell className="h-4 w-4" />
                  {totalUpdates > 0 && (
                    <span className="absolute -right-0.5 -top-0.5 flex h-3.5 min-w-3.5 items-center justify-center rounded-full bg-indigo-600 px-1 text-[9px] font-semibold text-white">
                      {totalUpdates}
                    </span>
                  )}
                </button>
                <div className="flex rounded-md border border-neutral-700 p-0.5">
                  {toggle("graph", "Branch graph", <Network className="h-4 w-4" />)}
                  {toggle("commits", "Commit graph", <Waypoints className="h-4 w-4" />)}
                  {toggle("tree", "Tree", <ListTree className="h-4 w-4" />)}
                  {toggle("prs", "Pull requests", <GitPullRequest className="h-4 w-4" />)}
                  {toggle("issues", "Issues", <CircleDot className="h-4 w-4" />)}
                  {toggle("docs", "Markdown docs", <FileText className="h-4 w-4" />)}
                </div>
                <button
                  onClick={async () => {
                    if (selected && (await runMutation(api.sync(selected)))) notify("Synced ✓");
                  }}
                  disabled={loading || !!view.conflict}
                  title="Fetch origin, fast-forward trunk, clean up merged PRs, then restack"
                  className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2.5 py-1.5 text-xs font-medium text-neutral-200 hover:bg-neutral-800 disabled:opacity-50"
                >
                  <DownloadCloud className="h-3.5 w-3.5" /> Sync
                </button>
                <button
                  onClick={() => selected && runMutation(api.restack(selected, null))}
                  disabled={loading || !!view.conflict}
                  title="Restack the whole stack onto its parents"
                  className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2.5 py-1.5 text-xs font-medium text-neutral-200 hover:bg-neutral-800 disabled:opacity-50"
                >
                  <Layers className="h-3.5 w-3.5" /> Restack all
                </button>
                <button
                  onClick={() =>
                    setDialog({ type: "new", parent: view.currentBranch ?? view.trunk })
                  }
                  className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-2.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500"
                >
                  <Plus className="h-3.5 w-3.5" /> New branch
                </button>
                <button
                  onClick={() => setShowSubmit(true)}
                  disabled={loading || !view.prsAvailable || !!view.conflict}
                  title={
                    view.prsAvailable
                      ? "Push branches and open/update PRs bottom-up"
                      : "Sign in with gh and add a GitHub remote to submit"
                  }
                  className="inline-flex items-center gap-1.5 rounded-md bg-emerald-600 px-2.5 py-1.5 text-xs font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
                >
                  <GitPullRequest className="h-3.5 w-3.5" /> Submit
                </button>
                <button
                  onClick={() =>
                    selected &&
                    api.openInVscode(selected).catch((e) => setError(errorText(e)))
                  }
                  title="Open repository in VS Code"
                  className="rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
                >
                  <Code2 className="h-4 w-4" />
                </button>
                <button
                  onClick={() => refresh(selected)}
                  title="Refresh"
                  className="rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
                >
                  <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
                </button>
              </div>
            </>
          ) : (
            <h1 className="text-sm font-medium text-neutral-400">No repository selected</h1>
          )}
        </header>

        <div className="flex min-h-0 flex-1">
          {workspace ? (
            <div className="min-w-0 flex-1">
              <RepoGraphView repos={repos} onOpenRepo={openRepo} />
            </div>
          ) : (
            <>
              {/* View region */}
              <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
                {(view?.conflict || error) && (
                  <div className="space-y-3 border-b border-neutral-800 p-4">
                    {view?.conflict && selected && (
                      <ConflictPanel
                        conflict={view.conflict}
                        busy={loading}
                        onContinue={() => runMutation(api.continueRestack(selected))}
                        onAbort={() => runMutation(api.abortRestack(selected))}
                      />
                    )}
                    {error && (
                      <div className="rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-300">
                        {error}
                      </div>
                    )}
                  </div>
                )}

                <div className="min-h-0 flex-1">
                  {!selected ? (
                    <div className="flex h-full flex-col items-center justify-center text-center text-neutral-600">
                      <GitBranch className="mb-3 h-10 w-10 text-neutral-700" />
                      <p className="text-sm">Add a git repository to see your branch stack.</p>
                      <button
                        onClick={() => setShowAdd(true)}
                        className="mt-4 inline-flex items-center gap-2 rounded-md bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500"
                      >
                        <FolderPlus className="h-4 w-4" /> Add repository
                      </button>
                    </div>
                  ) : !view ? null : viewMode === "issues" ? (
                    <IssuesList
                      repoPath={selected}
                      selected={inspectIssue}
                      onSelect={(n) => setInspectIssue(n)}
                    />
                  ) : viewMode === "prs" ? (
                    <PrList
                      repoPath={selected}
                      selected={inspectPr}
                      onSelect={(n) => setInspectPr(n)}
                    />
                  ) : viewMode === "docs" ? (
                    <DocsView
                      repoPath={selected}
                      branches={flattenBranches(view).map((b) => b.name)}
                      defaultBranch={view.currentBranch ?? view.trunk}
                      onCreated={(v) => setView(v)}
                    />
                  ) : viewMode === "graph" ? (
                    <StackGraph
                      roots={view.roots}
                      untracked={view.untracked}
                      selected={inspect}
                      onSelect={(name) => setInspect(name)}
                    />
                  ) : viewMode === "commits" ? (
                    <div className="relative h-full">
                      <div className="absolute left-3 top-3 z-10">
                        <CommitFilter
                          branches={flattenBranches(view).map((b) => b.name)}
                          value={commitFilter}
                          onChange={setCommitFilter}
                        />
                      </div>
                      <CommitGraph
                        nodes={commits}
                        selected={inspectCommit}
                        onSelect={(sha) => setInspectCommit(sha)}
                      />
                    </div>
                  ) : (
                    <div className="h-full overflow-auto p-6">
                      <div className="mx-auto max-w-3xl">
                        <StackTree
                          roots={view.roots}
                          onAction={onAction}
                          onSelect={(b) => setInspect(b.name)}
                          selected={inspect}
                        />
                        {view.untracked.length > 0 && (
                          <div className="mt-8">
                            <h2 className="mb-2 text-xs uppercase tracking-wider text-neutral-500">
                              Untracked branches
                            </h2>
                            <div className="space-y-0.5 opacity-90">
                              {view.untracked.map((b) => (
                                <BranchRow
                                  key={b.name}
                                  branch={b}
                                  onAction={onAction}
                                  onSelect={(br) => setInspect(br.name)}
                                  isSelected={b.name === inspect}
                                />
                              ))}
                            </div>
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              </div>

              {/* Detail panel (resizable) */}
              {panel && (
                <>
                  <div
                    onMouseDown={startResize}
                    title="Drag to resize"
                    className="w-1 shrink-0 cursor-col-resize bg-neutral-800 transition-colors hover:bg-indigo-600"
                  />
                  <div style={{ width: panelWidth }} className="flex min-w-0 shrink-0">
                    {panel}
                  </div>
                </>
              )}
            </>
          )}
        </div>

        {terminal && (
          <TerminalDock
            repoPath={terminal.repoPath}
            target={terminal.target}
            mode={terminal.mode}
            onClose={() => setTerminal(null)}
          />
        )}
      </main>

      {dialog?.type === "new" && view && (
        <NewBranchDialog
          parent={dialog.parent}
          branches={flattenBranches(view).map((b) => b.name)}
          onClose={() => setDialog(null)}
          onSubmit={(name, parent) =>
            selected && runMutation(api.createBranch(selected, name, parent))
          }
        />
      )}
      {dialog?.type === "parent" && view && (
        <SetParentDialog
          branch={dialog.branch.name}
          current={dialog.branch.parent}
          branches={flattenBranches(view).map((b) => b.name)}
          onClose={() => setDialog(null)}
          onSubmit={(parent) =>
            selected && runMutation(api.setParent(selected, dialog.branch.name, parent))
          }
        />
      )}
      {showSubmit && selected && (
        <SubmitDialog
          repoPath={selected}
          onClose={() => setShowSubmit(false)}
          onDone={(v, summary) => {
            setView(v);
            setShowSubmit(false);
            notify(`Submitted — ${summary}`);
          }}
        />
      )}
      {showAdd && (
        <AddRepoDialog onClose={() => setShowAdd(false)} onDone={registerRepo} />
      )}
      {toast && (
        <div className="fixed bottom-4 right-4 z-50 rounded-md border border-neutral-700 bg-neutral-900 px-3 py-2 text-sm text-neutral-100 shadow-lg">
          {toast}
        </div>
      )}
    </div>
  );
}
