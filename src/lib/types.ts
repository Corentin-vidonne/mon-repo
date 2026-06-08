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

export type StashFile = { status: string; path: string };

export type StashEntry = {
  index: number;
  refName: string;
  message: string;
  branch: string;
  date: string;
  files: StashFile[];
};

export type PrDescription = { title: string; body: string };

export type CheckRun = {
  name: string;
  /** pass | fail | pending | skipping | cancel */
  bucket: string;
  state: string;
  link: string;
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
  | "publish"
  | "merge";

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

/** A line in a commit's structured diff (for the split picker). */
export type SplitDiffLine = {
  /** context | add | del | meta */
  kind: "context" | "add" | "del" | "meta";
  /** Line text without the leading +/-/space marker. */
  text: string;
  /** Stable id for selectable add/del lines; null for context/meta. */
  id: number | null;
};
export type SplitHunk = { header: string; lines: SplitDiffLine[] };
/** One file in a commit's diff, with `selectable` false for binary / pure-deletion files. */
export type SplitDiffFile = { path: string; hunks: SplitHunk[]; selectable: boolean };

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

/** One structured finding from an AI PR review. */
export type PrFinding = {
  file: string;
  /** 1-based line, when the model can pin it. */
  line: number | null;
  /** info | warning | critical */
  severity: string;
  title: string;
  detail: string;
};

/** Result of an AI PR review: a short summary plus structured findings. */
export type PrReview = {
  summary: string;
  findings: PrFinding[];
};

/** AI-suggested resolution for one conflicted file. */
export type ConflictSuggestion = {
  file: string;
  explanation: string;
  /** Full resolved file content, ready to write back. */
  resolution: string;
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
