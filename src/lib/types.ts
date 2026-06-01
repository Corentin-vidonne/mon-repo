// Mirrors the serde structs in src-tauri/src/model.rs (camelCase).

export type PrInfo = {
  number: number;
  url: string;
  /** OPEN | CLOSED | MERGED */
  state: string;
  baseRef: string;
  /** APPROVED | CHANGES_REQUESTED | REVIEW_REQUIRED | null */
  reviewDecision: string | null;
  /** SUCCESS | FAILURE | PENDING | null */
  checks: string | null;
};

export type Branch = {
  name: string;
  parent: string | null;
  baseSha: string | null;
  isTrunk: boolean;
  isCurrent: boolean;
  ahead: number;
  behind: number;
  dirty: boolean;
  needsPush: boolean;
  tracked: boolean;
  pr: PrInfo | null;
};

export type StackNode = {
  branch: Branch;
  children: StackNode[];
};

export type ConflictState = {
  branch: string | null;
  files: string[];
};

export type RepoView = {
  repoRoot: string;
  name: string;
  trunk: string;
  currentBranch: string | null;
  roots: StackNode[];
  untracked: Branch[];
  prsAvailable: boolean;
  dirty: boolean;
  conflict: ConflictState | null;
};

export type Health = {
  gitVersion: string | null;
  ghVersion: string | null;
  ghAuthenticated: boolean;
  ghAccount: string | null;
};

export type BranchActionKind =
  | "new-child"
  | "set-parent"
  | "track"
  | "untrack"
  | "restack"
  | "checkout"
  | "publish";

export type CommitInfo = {
  sha: string;
  subject: string;
  author: string;
  date: string;
};

export type CommitNode = {
  sha: string;
  shortSha: string;
  parents: string[];
  subject: string;
  author: string;
  date: string;
  refs: string[];
};

export type FileChange = {
  status: string;
  path: string;
};

export type CommitDetail = {
  message: string;
  files: FileChange[];
  diff: string;
};

export type SubmitStepInfo = {
  branch: string;
  base: string;
  /** "create" | "update" | "uptodate" */
  action: string;
  pr: number | null;
  defaultTitle: string;
};

export type RepoNode = { id: string; name: string; remoteUrl: string | null };
export type RepoEdge = { from: string; to: string; via: string };
export type RepoGraph = { nodes: RepoNode[]; edges: RepoEdge[] };

export type PrFile = {
  path: string;
  additions: number;
  deletions: number;
};

export type Comment = {
  author: string;
  body: string;
  createdAt: string;
};

export type Review = {
  author: string;
  /** APPROVED | CHANGES_REQUESTED | COMMENTED | DISMISSED | PENDING */
  state: string;
  body: string;
  createdAt: string;
};

export type PrDetail = {
  number: number;
  title: string;
  body: string;
  state: string;
  author: string;
  baseRef: string;
  headRef: string;
  url: string;
  additions: number;
  deletions: number;
  reviewDecision: string | null;
  checks: string | null;
  files: PrFile[];
  commits: string[];
  diff: string;
  comments: Comment[];
  reviews: Review[];
};

export type IssueSummary = {
  number: number;
  title: string;
  /** OPEN | CLOSED */
  state: string;
  author: string;
  url: string;
  labels: string[];
  commentCount: number;
  updatedAt: string;
};

export type IssueDetail = {
  number: number;
  title: string;
  body: string;
  state: string;
  author: string;
  url: string;
  labels: string[];
  comments: Comment[];
};

export type PrSummary = {
  number: number;
  title: string;
  /** OPEN | CLOSED | MERGED */
  state: string;
  author: string;
  headRef: string;
  baseRef: string;
  url: string;
  isDraft: boolean;
  reviewDecision: string | null;
  checks: string | null;
  updatedAt: string;
};

export type UpdateItem = {
  /** Stable, change-sensitive id (notify once per distinct change). */
  key: string;
  /** "trunk" | "pr" | "issue" */
  kind: string;
  number: number | null;
  title: string;
  detail: string;
};

export type UpdateReport = { items: UpdateItem[] };
