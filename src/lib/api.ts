import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  CheckRun,
  CommitDetail,
  CommitInfo,
  CommitNode,
  ConflictSuggestion,
  Health,
  IssueDetail,
  IssueSummary,
  PrDescription,
  PrDetail,
  PrFinding,
  PrReview,
  PrSummary,
  RepoGraph,
  RepoView,
  SplitDiffFile,
  StashEntry,
  SubmitStepInfo,
  UpdateItem,
  UpdateReport,
} from "./types";

export const api = {
  health: () => invoke<Health>("health"),
  setAiBackend: (
    backend: string,
    ollamaHost: string,
    ollamaModel: string,
    anthropicModel: string
  ) => invoke<void>("set_ai_backend", { backend, ollamaHost, ollamaModel, anthropicModel }),
  ollamaModels: (host: string) => invoke<string[]>("ollama_models", { host }),
  getRepoView: (path: string) => invoke<RepoView>("get_repo_view", { path }),
  cloneRepo: (url: string, destParent: string) =>
    invoke<RepoView>("clone_repo", { url, destParent }),
  createBranch: (path: string, name: string, parent: string | null) =>
    invoke<RepoView>("create_branch", { path, name, parent }),
  setParent: (path: string, branch: string, parent: string) =>
    invoke<RepoView>("set_parent", { path, branch, parent }),
  untrackBranch: (path: string, branch: string) =>
    invoke<RepoView>("untrack_branch", { path, branch }),
  restack: (path: string, from: string | null) =>
    invoke<RepoView>("restack", { path, from }),
  continueRestack: (path: string) => invoke<RepoView>("continue_restack", { path }),
  abortRestack: (path: string) => invoke<RepoView>("abort_restack", { path }),
  submit: (
    path: string,
    from: string | null,
    draft: boolean,
    titles: Record<string, string>,
    bodies: Record<string, string>
  ) => invoke<RepoView>("submit", { path, from, draft, titles, bodies }),
  submitPlan: (path: string, from: string | null) =>
    invoke<SubmitStepInfo[]>("submit_plan", { path, from }),
  sync: (path: string) => invoke<RepoView>("sync", { path }),
  undo: (path: string) => invoke<RepoView>("undo", { path }),
  undoPeek: (path: string) => invoke<string | null>("undo_peek", { path }),
  listStashes: (path: string) => invoke<StashEntry[]>("list_stashes", { path }),
  stashCount: (path: string) => invoke<number>("stash_count", { path }),
  stashPush: (path: string, message: string | null, includeUntracked: boolean) =>
    invoke<StashEntry[]>("stash_push", { path, message, includeUntracked }),
  stashApply: (path: string, refName: string) =>
    invoke<StashEntry[]>("stash_apply", { path, refName }),
  stashPop: (path: string, refName: string) =>
    invoke<StashEntry[]>("stash_pop", { path, refName }),
  stashDrop: (path: string, refName: string) =>
    invoke<StashEntry[]>("stash_drop", { path, refName }),
  checkout: (path: string, branch: string) =>
    invoke<RepoView>("checkout", { path, branch }),
  publishBranch: (path: string, branch: string) =>
    invoke<RepoView>("publish_branch", { path, branch }),
  branchCommits: (path: string, branch: string) =>
    invoke<CommitInfo[]>("branch_commits", { path, branch }),
  rewordCommit: (path: string, branch: string, sha: string, message: string) =>
    invoke<RepoView>("reword_commit", { path, branch, sha, message }),
  dropCommit: (path: string, branch: string, sha: string) =>
    invoke<RepoView>("drop_commit", { path, branch, sha }),
  moveCommit: (path: string, branch: string, sha: string, direction: "up" | "down") =>
    invoke<RepoView>("move_commit", { path, branch, sha, direction }),
  squashCommit: (path: string, branch: string, sha: string) =>
    invoke<RepoView>("squash_commit", { path, branch, sha }),
  splitDiff: (path: string, sha: string) =>
    invoke<SplitDiffFile[]>("split_diff", { path, sha }),
  splitCommit: (
    path: string,
    branch: string,
    sha: string,
    lines: number[],
    msg1: string,
    msg2: string
  ) => invoke<RepoView>("split_commit", { path, branch, sha, lines, msg1, msg2 }),
  cherryPick: (path: string, sha: string, target: string) =>
    invoke<RepoView>("cherry_pick", { path, sha, target }),
  stackCommits: (path: string, branches?: string[] | null) =>
    invoke<CommitNode[]>("stack_commits", { path, branches: branches ?? null }),
  commitDetail: (path: string, sha: string) =>
    invoke<CommitDetail>("commit_detail", { path, sha }),
  prDetail: (path: string, number: number) =>
    invoke<PrDetail>("pr_detail", { path, number }),
  submitPrReview: (
    path: string,
    number: number,
    event: "approve" | "request_changes" | "comment",
    body: string
  ) => invoke<PrDetail>("submit_pr_review", { path, number, event, body }),
  postReviewComments: (
    path: string,
    number: number,
    summary: string,
    findings: PrFinding[]
  ) => invoke<string>("post_review_comments", { path, number, summary, findings }),
  prChecks: (path: string, number: number) =>
    invoke<CheckRun[]>("pr_checks", { path, number }),
  reviewPr: (path: string, number: number) =>
    invoke<PrReview>("review_pr", { path, number }),
  reviewCommit: (path: string, sha: string) =>
    invoke<PrReview>("review_commit", { path, sha }),
  suggestBranchName: (path: string) => invoke<string>("suggest_branch_name", { path }),
  generatePrDescription: (path: string, branch: string) =>
    invoke<PrDescription>("generate_pr_description", { path, branch }),
  suggestConflictResolution: (path: string, file: string) =>
    invoke<ConflictSuggestion>("suggest_conflict_resolution", { path, file }),
  applyConflictResolution: (path: string, file: string, content: string) =>
    invoke<RepoView>("apply_conflict_resolution", { path, file, content }),
  listIssues: (path: string, state: string) =>
    invoke<IssueSummary[]>("list_issues", { path, state }),
  listPullRequests: (path: string, state: string) =>
    invoke<PrSummary[]>("list_pull_requests", { path, state }),
  issueDetail: (path: string, number: number) =>
    invoke<IssueDetail>("issue_detail", { path, number }),
  repoGraph: (paths: string[]) => invoke<RepoGraph>("repo_graph", { paths }),
  analyzeCommit: (path: string, sha: string, mode: string) =>
    invoke<void>("analyze_commit", { path, sha, mode }),
  generateCommitMessage: (path: string, sha: string, mode: "simple" | "complet") =>
    invoke<string>("generate_commit_message", { path, sha, mode }),
  checkUpdates: (path: string) => invoke<UpdateReport>("check_updates", { path }),
  markUpdatesSeen: (path: string) => invoke<void>("mark_updates_seen", { path }),
  summarizeUpdates: (path: string, items: UpdateItem[]) =>
    invoke<string>("summarize_updates", { path, items }),
  openInVscode: (path: string) => invoke<void>("open_in_vscode", { path }),
  listMarkdown: (path: string) => invoke<string[]>("list_markdown", { path }),
  readMarkdown: (path: string, rel: string) =>
    invoke<string>("read_markdown", { path, rel }),
  createMarkdown: (path: string, branch: string, rel: string, content: string) =>
    invoke<RepoView>("create_markdown", { path, branch, rel, content }),
};

/** Tauri command errors come back as `{ message }`; extract a readable string. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object") {
    const m = (e as { message?: unknown }).message;
    if (typeof m === "string") return m;
  }
  return String(e);
}

/** Native folder picker. Returns the chosen directory, or null if cancelled. */
export async function pickRepoFolder(
  title = "Select a git repository"
): Promise<string | null> {
  const result = await open({ directory: true, multiple: false, title });
  return typeof result === "string" ? result : null;
}
