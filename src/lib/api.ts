import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  CommitDetail,
  CommitInfo,
  CommitNode,
  Health,
  IssueDetail,
  IssueSummary,
  PrDetail,
  PrSummary,
  RepoGraph,
  RepoView,
  SubmitStepInfo,
  UpdateReport,
} from "./types";

export const api = {
  health: () => invoke<Health>("health"),
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
    titles: Record<string, string>
  ) => invoke<RepoView>("submit", { path, from, draft, titles }),
  submitPlan: (path: string, from: string | null) =>
    invoke<SubmitStepInfo[]>("submit_plan", { path, from }),
  sync: (path: string) => invoke<RepoView>("sync", { path }),
  checkout: (path: string, branch: string) =>
    invoke<RepoView>("checkout", { path, branch }),
  publishBranch: (path: string, branch: string) =>
    invoke<RepoView>("publish_branch", { path, branch }),
  branchCommits: (path: string, branch: string) =>
    invoke<CommitInfo[]>("branch_commits", { path, branch }),
  stackCommits: (path: string, branches?: string[] | null) =>
    invoke<CommitNode[]>("stack_commits", { path, branches: branches ?? null }),
  commitDetail: (path: string, sha: string) =>
    invoke<CommitDetail>("commit_detail", { path, sha }),
  prDetail: (path: string, number: number) =>
    invoke<PrDetail>("pr_detail", { path, number }),
  listIssues: (path: string, state: string) =>
    invoke<IssueSummary[]>("list_issues", { path, state }),
  listPullRequests: (path: string, state: string) =>
    invoke<PrSummary[]>("list_pull_requests", { path, state }),
  issueDetail: (path: string, number: number) =>
    invoke<IssueDetail>("issue_detail", { path, number }),
  repoGraph: (paths: string[]) => invoke<RepoGraph>("repo_graph", { paths }),
  analyzeCommit: (path: string, sha: string, mode: string) =>
    invoke<void>("analyze_commit", { path, sha, mode }),
  checkUpdates: (path: string) => invoke<UpdateReport>("check_updates", { path }),
  markUpdatesSeen: (path: string) => invoke<void>("mark_updates_seen", { path }),
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
