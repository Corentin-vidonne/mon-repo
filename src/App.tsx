import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import {
  RefreshCw,
  FolderPlus,
  FolderTree,
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
  Settings as SettingsIcon,
  Undo2,
  Sparkles,
  Archive,
  Keyboard,
  AlertTriangle,
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
import { CommitPage } from "./components/CommitPage";
import { NewBranchDialog, SetParentDialog, MergeBranchDialog } from "./components/BranchDialogs";
import { ConflictPanel } from "./components/ConflictPanel";
import { SubmitDialog } from "./components/SubmitDialog";
import { RepoGraphView } from "./components/RepoGraphView";
import { TerminalDock, type AnalyzeTarget } from "./components/TerminalDock";
import { ChatDock } from "./components/ChatDock";
import { PrPage } from "./components/PrPage";
import { IssuesList } from "./components/IssuesList";
import { IssueDetailPanel } from "./components/IssueDetailPanel";
import { PrList } from "./components/PrList";
import { DocsView } from "./components/DocsView";
import { AddRepoDialog } from "./components/AddRepoDialog";
import { Sidebar } from "./components/Sidebar";
import { GroupNameDialog } from "./components/GroupNameDialog";
import { WorkspaceGroupFilter } from "./components/WorkspaceGroupFilter";
import { SettingsModal } from "./components/SettingsModal";
import { StashModal } from "./components/StashModal";
import { DigestBanner } from "./components/DigestBanner";
import { CommandPalette, type PaletteItem } from "./components/CommandPalette";
import { ShortcutsHelp } from "./components/ShortcutsHelp";
import { HelpPage } from "./components/HelpPage";
import { DependencyGate, dependenciesIncomplete } from "./components/DependencyGate";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { Tour, type GuideStep } from "./components/Tour";
import { Spinner } from "./components/Spinner";
import { AppUpdateBanner } from "./components/AppUpdateBanner";
import {
  loadSettings,
  saveSettings,
  hasSeenTour,
  markTourSeen,
  hasSeenWelcome,
  markWelcomeSeen,
  aiLabel,
  type Settings,
} from "./lib/settings";
import { useTheme } from "./lib/theme";
import {
  loadGroups,
  saveGroups,
  buildSections,
  createGroup,
  renameGroup,
  deleteGroup,
  assignRepo,
  toggleCollapsed,
  forgetRepo,
  pruneAssignments,
  UNGROUPED,
  type RepoGroupsState,
} from "./lib/groups";

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
  | { type: "parent"; branch: Branch }
  | { type: "merge"; branch: Branch };
type ViewMode = "graph" | "commits" | "tree" | "prs" | "issues" | "docs";

// The full overview tour (targets always-present toolbar elements).
const MAIN_TOUR: GuideStep[] = [
  { title: "Bienvenue 👋", body: "Petit tour des fonctions principales. Tu peux fermer à tout moment (Échap)." },
  { selector: '[data-tour="views"]', title: "Changer de vue", body: "Graphe des branches, commits, arborescence, PRs, issues, docs. (Raccourcis 1–6.)" },
  { selector: '[data-tour="stack-actions"]', title: "Actions de pile", body: "Sync met à jour, Restack rebase la pile, Undo annule la dernière opération. (s / r / u)" },
  { selector: '[data-tour="new-branch"]', title: "Créer une branche", body: "Au sommet de la pile, avec un nom suggéré par l'IA. (n)" },
  { selector: '[data-tour="submit"]', title: "Publier", body: "Pousse et ouvre/met à jour les PRs ; description rédigée par l'IA. (p)" },
  { selector: '[data-tour="claude"]', title: "Assistant IA", body: "Discute du dépôt, génère des messages, relis du code, merge guidé." },
  { selector: '[data-tour="stash"]', title: "Stashes", body: "Vois ce que contient chaque stash, applique-le ou supprime-le." },
  { title: "À toi de jouer 🚀", body: "Ctrl/⌘ + K pour la palette, ? pour les raccourcis. La page d'aide récapitule tout." },
];

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
  const [commitSearch, setCommitSearch] = useState("");
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
  const [groupState, setGroupState] = useState<RepoGroupsState>(loadGroups);
  const [groupSyncBusy, setGroupSyncBusy] = useState<Record<string, boolean>>({});
  const [workspaceGroup, setWorkspaceGroup] = useState<string | null>(null);
  const [groupDialog, setGroupDialog] = useState<
    | { mode: "new" }
    | { mode: "rename"; id: string }
    | { mode: "assign"; path: string }
    | null
  >(null);
  const [settings, setSettings] = useState<Settings>(loadSettings);
  // Display name of the active AI engine (Ollama model, or "Claude") for the AI surfaces.
  const aiName = aiLabel(settings);
  const [showSettings, setShowSettings] = useState(false);
  const [undoLabel, setUndoLabel] = useState<string | null>(null);
  const [showStash, setShowStash] = useState(false);
  const [stashCount, setStashCount] = useState(0);
  const [digestDismissed, setDigestDismissed] = useState<Set<string>>(new Set());
  const [showPalette, setShowPalette] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
  const [showDeps, setShowDeps] = useState(false);
  const [showWelcome, setShowWelcome] = useState(() => !hasSeenWelcome());
  const [guideSteps, setGuideSteps] = useState<GuideStep[] | null>(null);
  const { isModern } = useTheme();
  const notifiedKeys = useRef<Set<string>>(new Set());
  const totalUpdates = Object.values(updates).reduce((n, a) => n + a.length, 0);

  // True while a *different* repo's view is loading (first open or switch), but
  // not for in-place mutations/refreshes of the already-shown repo — those keep
  // the current view with the inline spinning refresh icon. `loading` going back
  // to false dismisses this even in the (impossible) path-mismatch case.
  const switchingRepo =
    !!selected && loading && (!view || view.repoRoot !== selected);

  function notify(msg: string) {
    setToast(msg);
    window.setTimeout(() => setToast(null), 3000);
  }

  // Apply a pure reducer to the group state and persist the result.
  const mutateGroups = useCallback(
    (fn: (s: RepoGroupsState) => RepoGroupsState) =>
      setGroupState((prev) => {
        const next = fn(prev);
        saveGroups(next);
        return next;
      }),
    []
  );

  function createGroupAndAssign(path: string, name: string) {
    mutateGroups((s) => {
      const { state, id } = createGroup(s, name);
      return assignRepo(state, path, id);
    });
  }

  const sections = useMemo(
    () => buildSections(groupState, repos),
    [groupState, repos]
  );

  // Repos shown in the Workspace graph, filtered by the active group.
  const workspaceRepos = useMemo(() => {
    if (!workspaceGroup) return repos;
    if (workspaceGroup === UNGROUPED)
      return repos.filter(
        (p) =>
          !groupState.assignments[p] ||
          !groupState.groups.some((g) => g.id === groupState.assignments[p])
      );
    return repos.filter((p) => groupState.assignments[p] === workspaceGroup);
  }, [repos, workspaceGroup, groupState]);

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

  // Re-probe the toolchain; surfaced by the "Revérifier" button in the dependency gate.
  // Only refreshes status — the gate stays open so the user sees the result and closes it.
  const recheckHealth = useCallback(async () => {
    try {
      setHealth(await api.health());
    } catch {
      /* ignore */
    }
  }, []);

  // On launch, fetch health once and pop the gate if a required tool is missing or gh
  // isn't authenticated, with the install links and instructions to fix it.
  useEffect(() => {
    api
      .health()
      .then((h) => {
        setHealth(h);
        // On the very first launch the welcome screen handles onboarding, so don't also
        // pop the dependency gate (the footer "outils" pill still flags git/gh).
        if (dependenciesIncomplete(h) && hasSeenWelcome()) setShowDeps(true);
      })
      .catch(() => {});
  }, []);

  // Keep the Rust-side AI backend (Anthropic vs Ollama) in sync with settings — on
  // startup and on any change — since the spawn funnels read a process-global config.
  useEffect(() => {
    api
      .setAiBackend(
        settings.aiBackend,
        settings.ollamaHost,
        settings.ollamaModel,
        settings.anthropicModel
      )
      .catch(() => {});
  }, [
    settings.aiBackend,
    settings.ollamaHost,
    settings.ollamaModel,
    settings.anthropicModel,
  ]);

  // One-time cleanup: drop group assignments for repos that no longer exist.
  useEffect(() => {
    mutateGroups((s) => pruneAssignments(s, repos));
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    mutateGroups((s) => forgetRepo(s, path));
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

    if (!settings.notifications) return;
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
  }, [repos, settings.notifications]);

  useEffect(() => {
    if (repos.length === 0) return;
    checkAllUpdates();
    const id = window.setInterval(checkAllUpdates, settings.pollIntervalMs);
    return () => window.clearInterval(id);
  }, [checkAllUpdates, settings.pollIntervalMs]);

  // Refresh "what can I undo?" and the stash count after every view change.
  useEffect(() => {
    if (!selected) {
      setUndoLabel(null);
      setStashCount(0);
      return;
    }
    api
      .undoPeek(selected)
      .then(setUndoLabel)
      .catch(() => setUndoLabel(null));
    api
      .stashCount(selected)
      .then(setStashCount)
      .catch(() => setStashCount(0));
  }, [selected, view]);

  // Global keyboard shortcuts (Ctrl/Cmd+K palette, ? help, single-key actions).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setShowPalette((s) => !s);
        return;
      }
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const t = e.target as HTMLElement | null;
      if (
        t &&
        (t.tagName === "INPUT" ||
          t.tagName === "TEXTAREA" ||
          t.tagName === "SELECT" ||
          t.isContentEditable)
      )
        return;
      if (showPalette || showShortcuts) return;
      if (e.key === "?") {
        setShowShortcuts(true);
        return;
      }
      // Escape closes a full-page view (PR, then commit), back to the list/graph.
      if (e.key === "Escape") {
        if (inspectPr != null) {
          setInspectPr(null);
          return;
        }
        if (viewMode === "commits" && inspectCommit) {
          setInspectCommit(null);
          return;
        }
      }
      if (!selected || !view) return;
      switch (e.key) {
        case "1": setViewMode("graph"); break;
        case "2": setViewMode("commits"); break;
        case "3": setViewMode("tree"); break;
        case "4": setViewMode("prs"); break;
        case "5": setViewMode("issues"); break;
        case "6": setViewMode("docs"); break;
        case "s":
          if (!view.conflict)
            runMutation(api.sync(selected)).then((ok) => ok && notify("Synced ✓"));
          break;
        case "r":
          if (!view.conflict) runMutation(api.restack(selected, null));
          break;
        case "n":
          setDialog({ type: "new", parent: view.currentBranch ?? view.trunk });
          break;
        case "p":
          if (view.prsAvailable && !view.conflict) setShowSubmit(true);
          break;
        case "u":
          if (undoLabel && !view.conflict) runMutation(api.undo(selected));
          break;
        default:
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, view, undoLabel, showPalette, showShortcuts, viewMode, inspectCommit, inspectPr]);

  // First launch ever: run the overview tour once, when the first repo is loaded
  // (so the spotlights have a real toolbar to point at). The welcome screen and the
  // dependency gate take priority — don't overlay the tour on top of them.
  useEffect(() => {
    if (view && !hasSeenTour() && !showDeps && !showWelcome) {
      markTourSeen();
      setGuideSteps(MAIN_TOUR);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  // Dismiss the welcome screen; chain into the tour when the user opts in and a repo is
  // already loaded (otherwise the tour starts via the effect above on the first repo).
  const finishWelcome = (startTour: boolean) => {
    markWelcomeSeen();
    setShowWelcome(false);
    if (startTour) {
      if (view && !hasSeenTour()) {
        markTourSeen();
        setGuideSteps(MAIN_TOUR);
      }
    } else {
      markTourSeen(); // skipped onboarding → don't pop the tour later either
    }
  };

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

  // Sync every repo in a group sequentially. Per-repo errors are counted, not
  // fatal, so one bad repo doesn't abort the rest. Refresh the on-screen view
  // only if the selected repo was part of the group.
  const syncGroup = useCallback(
    async (groupId: string) => {
      const section = sections.find((s) => (s.group?.id ?? UNGROUPED) === groupId);
      if (!section || section.repos.length === 0) return;
      setGroupSyncBusy((b) => ({ ...b, [groupId]: true }));
      let ok = 0;
      let fail = 0;
      for (const p of section.repos) {
        try {
          const v = await api.sync(p);
          if (p === selected) setView(v);
          ok++;
        } catch {
          fail++;
        }
      }
      setGroupSyncBusy((b) => {
        const { [groupId]: _drop, ...rest } = b;
        return rest;
      });
      notify(fail === 0 ? `Synced ${ok} repo(s) ✓` : `Synced ${ok}, ${fail} failed`);
      checkAllUpdates();
    },
    [sections, selected, checkAllUpdates]
  );

  function onAction(kind: BranchActionKind, branch: Branch) {
    if (!selected) return;
    if (kind === "new-child") setDialog({ type: "new", parent: branch.name });
    else if (kind === "untrack") runMutation(api.untrackBranch(selected, branch.name));
    else if (kind === "restack") runMutation(api.restack(selected, branch.name));
    else if (kind === "checkout") runMutation(api.checkout(selected, branch.name));
    else if (kind === "publish") runMutation(api.publishBranch(selected, branch.name));
    else if (kind === "merge") setDialog({ type: "merge", branch });
    else setDialog({ type: "parent", branch });
  }

  const inspectedBranch =
    view && inspect ? flattenBranches(view).find((b) => b.name === inspect) ?? null : null;
  const selectedCommit =
    inspectCommit ? commits.find((c) => c.sha === inspectCommit) ?? null : null;

  // Command-palette items (branches + actions + views). PRs/issues are added lazily
  // inside the palette itself.
  const paletteItems: PaletteItem[] = [];
  if (view && selected) {
    const repo = selected;
    for (const b of flattenBranches(view)) {
      paletteItems.push({
        id: `b-${b.name}`,
        group: "Branches",
        label: b.name,
        hint: b.isCurrent ? "courante" : b.isTrunk ? "tronc" : undefined,
        icon: <GitBranch className="h-4 w-4 text-indigo-400" />,
        run: () => {
          setInspectPr(null);
          setInspectIssue(null);
          setInspect(b.name);
          setViewMode("graph");
        },
      });
    }
    if (undoLabel) {
      paletteItems.push({
        id: "a-undo",
        group: "Actions",
        label: `Undo : ${undoLabel}`,
        icon: <Undo2 className="h-4 w-4" />,
        run: () => {
          runMutation(api.undo(repo));
        },
      });
    }
    paletteItems.push(
      {
        id: "a-sync",
        group: "Actions",
        label: "Sync",
        icon: <DownloadCloud className="h-4 w-4" />,
        run: () => {
          runMutation(api.sync(repo)).then((ok) => ok && notify("Synced ✓"));
        },
      },
      {
        id: "a-restack",
        group: "Actions",
        label: "Restack all",
        icon: <Layers className="h-4 w-4" />,
        run: () => {
          runMutation(api.restack(repo, null));
        },
      },
      {
        id: "a-submit",
        group: "Actions",
        label: "Submit",
        icon: <GitPullRequest className="h-4 w-4" />,
        run: () => setShowSubmit(true),
      },
      {
        id: "a-new",
        group: "Actions",
        label: "New branch",
        icon: <Plus className="h-4 w-4" />,
        run: () => setDialog({ type: "new", parent: view.currentBranch ?? view.trunk }),
      },
      {
        id: "a-stash",
        group: "Actions",
        label: "Stashes",
        icon: <Archive className="h-4 w-4" />,
        run: () => setShowStash(true),
      },
      {
        id: "a-claude",
        group: "Actions",
        label: `Demander à ${aiName}`,
        icon: <Sparkles className="h-4 w-4" />,
        run: () => setTerminal({ repoPath: repo, target: { kind: "repo" }, mode: "" }),
      },
      {
        id: "a-settings",
        group: "Actions",
        label: "Settings",
        icon: <SettingsIcon className="h-4 w-4" />,
        run: () => setShowSettings(true),
      },
      {
        id: "a-shortcuts",
        group: "Actions",
        label: "Raccourcis clavier",
        icon: <Keyboard className="h-4 w-4" />,
        run: () => setShowShortcuts(true),
      },
      {
        id: "a-help",
        group: "Actions",
        label: "Aide / Guide interactif",
        icon: <Sparkles className="h-4 w-4" />,
        run: () => setShowHelp(true),
      }
    );
    const palViews: [ViewMode, string][] = [
      ["graph", "Vue : Branch graph"],
      ["commits", "Vue : Commit graph"],
      ["tree", "Vue : Tree"],
      ["prs", "Vue : Pull requests"],
      ["issues", "Vue : Issues"],
      ["docs", "Vue : Markdown docs"],
    ];
    for (const [m, label] of palViews) {
      paletteItems.push({
        id: `v-${m}`,
        group: "Vues",
        label,
        icon: <ListTree className="h-4 w-4" />,
        run: () => setViewMode(m),
      });
    }
  }

  // Per-feature guides launched from the "Tester" buttons in the help page. Each navigates
  // to the feature (action), then spotlights/explains it — without blocking interaction.
  const guides: Record<string, GuideStep[]> = {
    "view-graph": [{ action: () => setViewMode("graph"), title: "Graphe des branches", body: "Voici ta pile. Glisse une branche sur une autre pour la re-parenter ; les pastilles montrent l'état (CI, review, retard…)." }],
    "view-commits": [{ action: () => setViewMode("commits"), selector: '[data-tour="commit-search"]', title: "Graphe des commits", body: "Le DAG des commits. Tape ici pour filtrer par message / sha / auteur." }],
    "commit-search": [{ action: () => setViewMode("commits"), selector: '[data-tour="commit-search"]', title: "Recherche de commits", body: "Tape : les commits non-matchés s'estompent et la vue se recentre sur les résultats." }],
    "commit-ops": [{ action: () => setViewMode("commits"), title: "Actions de commit", body: "Clique un commit → panneau de détail : Summary/Detailed, AI Review, Cherry-pick, et au survol reword (IA) / split / drop / squash / move." }],
    tree: [{ action: () => setViewMode("tree"), title: "Vue Tree", body: "La pile en liste. Survole une branche pour ses actions, clique-la pour le détail." }],
    "branch-ops": [{ action: () => setViewMode("tree"), title: "Actions de branche", body: "Survole une branche → checkout, set parent, restack, merge, track/untrack." }],
    prs: [{ action: () => setViewMode("prs"), title: "Pull requests", body: "Ouvre une PR : Approuver / Changements / Commenter, checks CI + logs, AI Review (postable en commentaires inline sur la PR)." }],
    issues: [{ action: () => setViewMode("issues"), title: "Issues", body: "La liste des issues et leur détail." }],
    docs: [{ action: () => setViewMode("docs"), title: "Docs", body: "Les fichiers Markdown du dépôt." }],
    views: [{ selector: '[data-tour="views"]', title: "Vues", body: "Clique pour changer de vue (ou les touches 1–6)." }],
    sync: [{ selector: '[data-tour="stack-actions"]', title: "Sync / Restack / Undo", body: "Sync met à jour, Restack rebase la pile, Undo annule. Clique pour tester." }],
    "new-branch": [{ selector: '[data-tour="new-branch"]', title: "Nouvelle branche", body: "Clique pour créer une branche (nom suggéré par l'IA)." }],
    submit: [{ selector: '[data-tour="submit"]', title: "Submit", body: "Pousse et ouvre/met à jour les PRs. Clique pour voir le plan et la description rédigée par l'IA." }],
    claude: [{ selector: '[data-tour="claude"]', title: "Assistant IA", body: "Clique pour discuter du dépôt avec Claude." }],
    stash: [{ action: () => setShowStash(true), title: "Stashes", body: "Voici tes stashes : déplie pour voir les fichiers, puis Apply / Pop / Drop." }],
    palette: [{ action: () => setShowPalette(true), title: "Palette de commandes", body: "Tape une branche, une PR, une action… Entrée pour lancer, Échap pour fermer." }],
    shortcuts: [{ action: () => setShowShortcuts(true), title: "Raccourcis clavier", body: "La liste des raccourcis (tu peux aussi appuyer sur ?)." }],
  };

  const panel =
    selected &&
    (inspectPr != null ? null : viewMode === "issues" ? (
      inspectIssue != null && (
        <IssueDetailPanel
          repoPath={selected}
          number={inspectIssue}
          onClose={() => setInspectIssue(null)}
        />
      )
    ) : viewMode === "commits" ? null : viewMode === "graph" || viewMode === "tree" ? (
      inspectedBranch && (
        <BranchDetail
          repoPath={selected}
          branch={inspectedBranch}
          onAction={onAction}
          onClose={() => setInspect(null)}
          onOpenPr={(n) => setInspectPr(n)}
          onEdited={(v) => setView(v)}
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
      className={
        isModern
          ? `rounded-md px-2 py-1 transition-colors ${
              viewMode === mode
                ? "bg-neutral-700/80 text-neutral-50 shadow-sm"
                : "text-neutral-400 hover:bg-neutral-800/70 hover:text-neutral-100"
            }`
          : `rounded p-1 ${
              viewMode === mode
                ? "bg-neutral-700 text-neutral-100"
                : "text-neutral-400 hover:text-neutral-200"
            }`
      }
    >
      {icon}
    </button>
  );

  return (
    <div className="flex h-screen w-screen bg-neutral-950 text-neutral-200">
      {/* Sidebar */}
      <aside className="flex w-64 shrink-0 flex-col border-r border-neutral-800 bg-neutral-900/40">
        {isModern ? (
          <div className="flex h-16 items-center gap-2.5 border-b border-neutral-800 px-4">
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-indigo-500/15 ring-1 ring-indigo-500/30">
              <GitBranch className="h-4 w-4 text-indigo-400" />
            </div>
            <div className="leading-tight">
              <div className="text-[15px] font-semibold tracking-tight text-neutral-100">
                gitui
              </div>
              <div className="text-[10px] uppercase tracking-wider text-neutral-500">
                stacked PRs
              </div>
            </div>
          </div>
        ) : (
          <div className="flex h-14 items-center gap-2 border-b border-neutral-800 px-4">
            <GitBranch className="h-5 w-5 text-indigo-400" />
            <span className="font-semibold tracking-tight">gitui</span>
            <span className="text-xs text-neutral-500">stacked PRs</span>
          </div>
        )}

        <button
          onClick={() => setWorkspace(true)}
          disabled={repos.length === 0}
          className={
            isModern
              ? `mx-2 mt-2 flex items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors disabled:opacity-40 ${
                  workspace
                    ? "bg-indigo-500/10 text-neutral-100 ring-1 ring-inset ring-indigo-500/25"
                    : "text-neutral-300 hover:bg-neutral-800/60"
                }`
              : `mx-2 mt-2 flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm disabled:opacity-40 ${
                  workspace
                    ? "bg-neutral-800 text-neutral-100"
                    : "text-neutral-300 hover:bg-neutral-900"
                }`
          }
        >
          <Boxes className="h-4 w-4 text-indigo-400" />
          Workspace
          <span
            className={
              isModern
                ? "ml-auto rounded-full bg-neutral-800 px-1.5 text-[11px] text-neutral-400"
                : "ml-auto text-xs text-neutral-500"
            }
          >
            {repos.length}
          </span>
        </button>
        <div className="flex items-center justify-between px-4 py-3">
          <span className="text-xs uppercase tracking-wider text-neutral-500">
            Repositories
          </span>
          <div className="flex items-center gap-0.5">
            <button
              onClick={() => setGroupDialog({ mode: "new" })}
              title="New group"
              className="rounded p-1 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
            >
              <FolderTree className="h-4 w-4" />
            </button>
            <button
              onClick={() => setShowAdd(true)}
              title="Add repository"
              className="rounded p-1 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
            >
              <FolderPlus className="h-4 w-4" />
            </button>
          </div>
        </div>

        <Sidebar
          sections={sections}
          groups={groupState.groups}
          selected={selected}
          workspace={workspace}
          updates={updates}
          groupSyncBusy={groupSyncBusy}
          onOpenRepo={openRepo}
          onRemoveRepo={removeRepo}
          onAssignRepo={(path, gid) => mutateGroups((s) => assignRepo(s, path, gid))}
          onCreateGroupForRepo={(path) => setGroupDialog({ mode: "assign", path })}
          onToggleCollapsed={(id) => mutateGroups((s) => toggleCollapsed(s, id))}
          onRenameGroup={(id) => setGroupDialog({ mode: "rename", id })}
          onDeleteGroup={(id) => {
            mutateGroups((s) => deleteGroup(s, id));
            setWorkspaceGroup((w) => (w === id ? null : w));
          }}
          onSyncGroup={syncGroup}
        />

        <div className="flex items-center justify-between border-t border-neutral-800 px-4 py-2 text-xs">
          {health?.ghAuthenticated ? (
            isModern ? (
              <span className="inline-flex items-center gap-1.5 rounded-full bg-emerald-500/10 px-2 py-0.5 text-emerald-400 ring-1 ring-inset ring-emerald-500/20">
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
                {health.ghAccount ?? "gh"}
              </span>
            ) : (
              <span className="text-emerald-400">● {health.ghAccount ?? "gh"}</span>
            )
          ) : isModern ? (
            <span className="inline-flex items-center gap-1.5 rounded-full bg-neutral-800/60 px-2 py-0.5 text-neutral-500">
              <span className="h-1.5 w-1.5 rounded-full bg-neutral-600" />
              gh: not logged in
            </span>
          ) : (
            <span className="text-neutral-500">gh: not logged in</span>
          )}
          <div className="flex items-center gap-1.5">
            {health && dependenciesIncomplete(health) && (
              <button
                onClick={() => setShowDeps(true)}
                title="Des outils requis manquent — cliquer pour voir"
                className="inline-flex items-center gap-1 rounded-full bg-amber-500/10 px-2 py-0.5 text-amber-400 ring-1 ring-inset ring-amber-500/20 hover:bg-amber-500/20"
              >
                <AlertTriangle className="h-3 w-3" /> outils
              </button>
            )}
            <button
              onClick={() => setShowSettings(true)}
              title="Settings"
              className="rounded p-1 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
            >
              <SettingsIcon className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </aside>

      {/* Main */}
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <header
          className={`flex shrink-0 items-center gap-3 border-b border-neutral-800 px-6 ${
            isModern ? "h-16" : "h-14"
          }`}
        >
          {workspace ? (
            <>
              <h1 className="text-sm font-medium text-neutral-200">
                Workspace — repository links
              </h1>
              {groupState.groups.length > 0 && (
                <div className="ml-auto">
                  <WorkspaceGroupFilter
                    groups={groupState.groups}
                    value={workspaceGroup}
                    onChange={setWorkspaceGroup}
                  />
                </div>
              )}
            </>
          ) : switchingRepo ? (
            <>
              <Spinner className="h-4 w-4" />
              <h1
                className={
                  isModern
                    ? "text-[15px] font-semibold tracking-tight text-neutral-100"
                    : "text-sm font-medium text-neutral-200"
                }
              >
                {repoName(selected!)}
              </h1>
              <span className="text-xs text-neutral-500">Loading…</span>
            </>
          ) : view ? (
            <>
              <h1
                className={
                  isModern
                    ? "text-[15px] font-semibold tracking-tight text-neutral-100"
                    : "text-sm font-medium text-neutral-200"
                }
              >
                {view.name}
              </h1>
              <span
                className={
                  isModern
                    ? "inline-flex items-center gap-1.5 rounded-full border border-neutral-700/70 bg-neutral-900/60 px-2.5 py-0.5 font-mono text-[11px] text-neutral-400"
                    : "rounded bg-neutral-800 px-2 py-0.5 font-mono text-xs text-neutral-400"
                }
              >
                {isModern && <GitBranch className="h-3 w-3 text-indigo-400" />}
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
                <button
                  data-tour="stash"
                  onClick={() => setShowStash(true)}
                  title={stashCount > 0 ? `${stashCount} stash(es)` : "Stashes"}
                  className="relative rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
                >
                  <Archive className="h-4 w-4" />
                  {stashCount > 0 && (
                    <span className="absolute -right-0.5 -top-0.5 flex h-3.5 min-w-3.5 items-center justify-center rounded-full bg-amber-600 px-1 text-[9px] font-semibold text-white">
                      {stashCount}
                    </span>
                  )}
                </button>
                <div
                  data-tour="views"
                  className={
                    isModern
                      ? "flex gap-0.5 rounded-lg border border-neutral-700/80 bg-neutral-900/40 p-1"
                      : "flex rounded-md border border-neutral-700 p-0.5"
                  }
                >
                  {toggle("graph", "Branch graph", <Network className="h-4 w-4" />)}
                  {toggle("commits", "Commit graph", <Waypoints className="h-4 w-4" />)}
                  {toggle("tree", "Tree", <ListTree className="h-4 w-4" />)}
                  {toggle("prs", "Pull requests", <GitPullRequest className="h-4 w-4" />)}
                  {toggle("issues", "Issues", <CircleDot className="h-4 w-4" />)}
                  {toggle("docs", "Markdown docs", <FileText className="h-4 w-4" />)}
                </div>
                {isModern && <div className="mx-0.5 h-5 w-px bg-neutral-800" />}
                <div
                  data-tour="stack-actions"
                  className={
                    isModern
                      ? "flex gap-0.5 rounded-lg border border-neutral-700/80 bg-neutral-900/40 p-1"
                      : "flex rounded-md border border-neutral-700 p-0.5"
                  }
                >
                  <button
                    onClick={async () => {
                      if (selected && (await runMutation(api.sync(selected))))
                        notify("Synced ✓");
                    }}
                    disabled={loading || !!view.conflict}
                    title="Sync : fetch + fast-forward du tronc + nettoyage des PR mergées, puis restack"
                    className="rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40"
                  >
                    <DownloadCloud className="h-4 w-4" />
                  </button>
                  <button
                    onClick={() => selected && runMutation(api.restack(selected, null))}
                    disabled={loading || !!view.conflict}
                    title="Restack toute la pile sur ses parents"
                    className="rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40"
                  >
                    <Layers className="h-4 w-4" />
                  </button>
                  <button
                    onClick={async () => {
                      if (selected && undoLabel && (await runMutation(api.undo(selected))))
                        notify("Undone ✓");
                    }}
                    disabled={loading || !undoLabel || !!view.conflict}
                    title={undoLabel ? `Annuler : ${undoLabel}` : "Rien à annuler"}
                    className="rounded p-1.5 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40"
                  >
                    <Undo2 className="h-4 w-4" />
                  </button>
                </div>
                <button
                  data-tour="new-branch"
                  onClick={() =>
                    setDialog({ type: "new", parent: view.currentBranch ?? view.trunk })
                  }
                  className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-2.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500"
                >
                  <Plus className="h-3.5 w-3.5" /> New branch
                </button>
                <button
                  data-tour="submit"
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
                  data-tour="claude"
                  onClick={() =>
                    selected &&
                    setTerminal({ repoPath: selected, target: { kind: "repo" }, mode: "" })
                  }
                  title={`Demander à ${aiName} (discuter du dépôt)`}
                  className="rounded p-1.5 text-indigo-300 hover:bg-indigo-950/40"
                >
                  <Sparkles className="h-4 w-4" />
                </button>
                {isModern && <div className="mx-0.5 h-5 w-px bg-neutral-800" />}
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
              <RepoGraphView
                repos={workspaceRepos}
                groups={groupState.groups}
                assignments={groupState.assignments}
                onOpenRepo={openRepo}
              />
            </div>
          ) : inspectPr != null && selected && !switchingRepo ? (
            <PrPage
              repoPath={selected}
              number={inspectPr}
              aiName={aiName}
              trunk={view?.trunk ?? null}
              onClose={() => setInspectPr(null)}
              onAnalyze={(number, mode) =>
                setTerminal({ repoPath: selected!, target: { kind: "pr", number }, mode })
              }
              onMerged={(v) => setView(v)}
            />
          ) : viewMode === "commits" && selectedCommit && selected && !switchingRepo ? (
            <CommitPage
              repoPath={selected}
              node={selectedCommit}
              branches={view ? flattenBranches(view).map((b) => b.name) : []}
              aiName={aiName}
              onClose={() => setInspectCommit(null)}
              onAnalyze={(sha, mode) =>
                setTerminal({ repoPath: selected!, target: { kind: "commit", sha }, mode })
              }
              onCherryPick={(sha, target) =>
                selected && runMutation(api.cherryPick(selected, sha, target))
              }
            />
          ) : (
            <>
              {/* View region */}
              <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
                {!switchingRepo &&
                  selected &&
                  (updates[selected]?.length ?? 0) > 0 &&
                  !digestDismissed.has(selected) && (
                    <DigestBanner
                      repoPath={selected}
                      items={updates[selected] ?? []}
                      onSeen={() => {
                        const s = selected;
                        if (!s) return;
                        api.markUpdatesSeen(s).catch(() => {});
                        setUpdates((u) => {
                          const next = { ...u };
                          delete next[s];
                          return next;
                        });
                      }}
                      onDismiss={() => {
                        const s = selected;
                        if (s) setDigestDismissed((d) => new Set(d).add(s));
                      }}
                    />
                  )}
                {!switchingRepo && (view?.conflict || error) && (
                  <div className="space-y-3 border-b border-neutral-800 p-4">
                    {view?.conflict && selected && (
                      <ConflictPanel
                        conflict={view.conflict}
                        repoPath={selected}
                        busy={loading}
                        onContinue={() => runMutation(api.continueRestack(selected))}
                        onAbort={() => runMutation(api.abortRestack(selected))}
                        onResolved={(v) => setView(v)}
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
                    isModern ? (
                      <div className="flex h-full flex-col items-center justify-center px-6 text-center">
                        <div className="mb-5 flex h-16 w-16 items-center justify-center rounded-2xl bg-indigo-500/10 ring-1 ring-inset ring-indigo-500/25">
                          <GitBranch className="h-8 w-8 text-indigo-400" />
                        </div>
                        <h2 className="text-base font-semibold text-neutral-100">
                          No repository yet
                        </h2>
                        <p className="mt-1 max-w-xs text-sm text-neutral-500">
                          Add a git repository to visualize and manage your stacked branches
                          and pull requests.
                        </p>
                        <button
                          onClick={() => setShowAdd(true)}
                          className="mt-5 inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-indigo-500"
                        >
                          <FolderPlus className="h-4 w-4" /> Add repository
                        </button>
                      </div>
                    ) : (
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
                    )
                  ) : switchingRepo ? (
                    <div className="flex h-full flex-col items-center justify-center gap-3">
                      <Spinner className="h-7 w-7" />
                      <p className="text-sm text-neutral-500">
                        Loading {repoName(selected)}…
                      </p>
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
                      onReparent={(branch, newParent) =>
                        selected &&
                        runMutation(api.setParent(selected, branch, newParent)).then(
                          (ok) => ok && notify(`${branch} → ${newParent}`)
                        )
                      }
                    />
                  ) : viewMode === "commits" ? (
                    <div className="relative h-full">
                      <div className="absolute left-3 top-3 z-10 flex items-center gap-2">
                        <CommitFilter
                          branches={flattenBranches(view).map((b) => b.name)}
                          value={commitFilter}
                          onChange={setCommitFilter}
                        />
                        <input
                          data-tour="commit-search"
                          value={commitSearch}
                          onChange={(e) => setCommitSearch(e.target.value)}
                          placeholder="Rechercher un commit…"
                          className="w-56 rounded-md border border-neutral-700 bg-neutral-900/90 px-2.5 py-1.5 text-xs text-neutral-100 shadow-sm outline-none focus:border-indigo-600"
                        />
                      </div>
                      <CommitGraph
                        nodes={commits}
                        selected={inspectCommit}
                        query={commitSearch}
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

        {terminal &&
          (terminal.target.kind === "repo" || settings.assistantUi === "chat" ? (
            <ChatDock
              repoPath={terminal.repoPath}
              target={terminal.target}
              mode={terminal.mode}
              streaming={settings.chatStreaming}
              aiName={aiName}
              onClose={() => setTerminal(null)}
            />
          ) : (
            <TerminalDock
              repoPath={terminal.repoPath}
              target={terminal.target}
              mode={terminal.mode}
              aiName={aiName}
              onClose={() => setTerminal(null)}
            />
          ))}
      </main>

      {dialog?.type === "new" && view && (
        <NewBranchDialog
          repoPath={selected ?? ""}
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
      {dialog?.type === "merge" && view && (
        <MergeBranchDialog
          source={dialog.branch.name}
          current={view.currentBranch}
          branches={flattenBranches(view).map((b) => b.name)}
          onClose={() => setDialog(null)}
          onSubmit={(source, target) => {
            if (selected)
              setTerminal({
                repoPath: selected,
                target: { kind: "merge-branches", source, target },
                mode: "merge",
              });
            setDialog(null);
          }}
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
      {showSettings && (
        <SettingsModal
          settings={settings}
          onClose={() => setShowSettings(false)}
          onOpenHelp={() => {
            setShowSettings(false);
            setShowHelp(true);
          }}
          onOpenDeps={() => {
            setShowSettings(false);
            void recheckHealth();
            setShowDeps(true);
          }}
          onSave={(s) => {
            setSettings(s);
            saveSettings(s);
            setShowSettings(false);
          }}
        />
      )}
      {showStash && selected && view && (
        <StashModal
          repoPath={selected}
          dirty={view.dirty}
          onClose={() => setShowStash(false)}
          onChanged={() => {
            if (selected) api.getRepoView(selected).then(setView).catch(() => {});
          }}
        />
      )}
      {showPalette && (
        <CommandPalette
          items={paletteItems}
          repoPath={selected}
          onOpenPr={(n) => {
            setInspectPr(n);
            setViewMode("prs");
          }}
          onOpenIssue={(n) => {
            setInspectIssue(n);
            setViewMode("issues");
          }}
          onClose={() => setShowPalette(false)}
        />
      )}
      {showShortcuts && <ShortcutsHelp onClose={() => setShowShortcuts(false)} />}
      {showHelp && (
        <HelpPage
          onClose={() => setShowHelp(false)}
          onStartTour={() => {
            setShowHelp(false);
            setGuideSteps(MAIN_TOUR);
          }}
          onTest={(id) => {
            const g = guides[id];
            if (g) {
              setShowHelp(false);
              setGuideSteps(g);
            }
          }}
        />
      )}
      {showWelcome && <WelcomeScreen onFinish={finishWelcome} />}
      {showDeps && !showWelcome && health && (
        <DependencyGate
          health={health}
          onRecheck={recheckHealth}
          onClose={() => setShowDeps(false)}
        />
      )}
      {guideSteps && <Tour steps={guideSteps} onClose={() => setGuideSteps(null)} />}
      {groupDialog?.mode === "new" && (
        <GroupNameDialog
          title="New group"
          confirmLabel="Create"
          onClose={() => setGroupDialog(null)}
          onSubmit={(name) => {
            mutateGroups((s) => createGroup(s, name).state);
            setGroupDialog(null);
          }}
        />
      )}
      {groupDialog?.mode === "assign" && (
        <GroupNameDialog
          title="New group"
          confirmLabel="Create & move"
          onClose={() => setGroupDialog(null)}
          onSubmit={(name) => {
            createGroupAndAssign(groupDialog.path, name);
            setGroupDialog(null);
          }}
        />
      )}
      {groupDialog?.mode === "rename" && (
        <GroupNameDialog
          title="Rename group"
          confirmLabel="Rename"
          initial={
            groupState.groups.find((g) => g.id === groupDialog.id)?.name ?? ""
          }
          onClose={() => setGroupDialog(null)}
          onSubmit={(name) => {
            mutateGroups((s) => renameGroup(s, groupDialog.id, name));
            setGroupDialog(null);
          }}
        />
      )}
      <AppUpdateBanner />
      {toast && (
        <div className="fixed bottom-4 right-4 z-50 rounded-md border border-neutral-700 bg-neutral-900 px-3 py-2 text-sm text-neutral-100 shadow-lg">
          {toast}
        </div>
      )}
    </div>
  );
}
