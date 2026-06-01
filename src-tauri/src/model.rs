use serde::Serialize;

/// A GitHub pull request associated with a branch.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PrInfo {
    pub number: u64,
    pub url: String,
    /// OPEN | CLOSED | MERGED
    pub state: String,
    pub base_ref: String,
    /// APPROVED | CHANGES_REQUESTED | REVIEW_REQUIRED | null
    pub review_decision: Option<String>,
    /// CI rollup: SUCCESS | FAILURE | PENDING | null
    pub checks: Option<String>,
}

/// A branch within a stack, enriched with status and metadata.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub parent: Option<String>,
    pub base_sha: Option<String>,
    pub is_trunk: bool,
    pub is_current: bool,
    /// Commits on this branch not on its parent.
    pub ahead: u32,
    /// Commits on the parent not on this branch (i.e. this branch needs a restack).
    pub behind: u32,
    pub dirty: bool,
    /// Local commits not yet on origin/<branch>.
    pub needs_push: bool,
    /// Whether this branch has gitstack metadata (a recorded parent).
    pub tracked: bool,
    pub pr: Option<PrInfo>,
}

/// A node in the stack tree (a branch and its dependent children).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StackNode {
    pub branch: Branch,
    pub children: Vec<StackNode>,
}

/// An in-progress rebase paused on conflicts.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConflictState {
    /// The branch being rebased, if known.
    pub branch: Option<String>,
    /// Files with unresolved conflicts.
    pub files: Vec<String>,
}

/// The full view of a repository sent to the frontend.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoView {
    pub repo_root: String,
    pub name: String,
    pub trunk: String,
    pub current_branch: Option<String>,
    /// Stack roots, typically a single tree rooted at the trunk.
    pub roots: Vec<StackNode>,
    /// Branches with no resolvable parent (candidates for tracking).
    pub untracked: Vec<Branch>,
    /// False when gh is unavailable / repo has no GitHub remote.
    pub prs_available: bool,
    pub dirty: bool,
    /// Present when a restack is paused on conflicts.
    pub conflict: Option<ConflictState>,
}

/// A single commit, for the branch detail panel.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}

/// A node in the commit DAG (for the commit-level graph view).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CommitNode {
    pub sha: String,
    pub short_sha: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub author: String,
    pub date: String,
    /// Branch names whose tip is this commit.
    pub refs: Vec<String>,
}

/// A file changed in a commit.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub status: String,
    pub path: String,
}

/// Full detail for a single commit.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub message: String,
    pub files: Vec<FileChange>,
    /// The commit's unified diff (possibly truncated for very large commits).
    pub diff: String,
}

/// A file changed in a pull request.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PrFile {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
}

/// Full detail for a single pull request.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PrDetail {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub author: String,
    pub base_ref: String,
    pub head_ref: String,
    pub url: String,
    pub additions: u64,
    pub deletions: u64,
    pub review_decision: Option<String>,
    pub checks: Option<String>,
    pub files: Vec<PrFile>,
    /// Commit subject lines, oldest first.
    pub commits: Vec<String>,
    /// The PR's unified diff (possibly truncated).
    pub diff: String,
    /// Conversation comments left on the PR, oldest first.
    pub comments: Vec<Comment>,
    /// Reviews (approve / request changes / comment) with their summary body.
    pub reviews: Vec<Review>,
}

/// A conversation comment on a PR or issue.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// A PR review with its overall verdict.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub author: String,
    /// APPROVED | CHANGES_REQUESTED | COMMENTED | DISMISSED | PENDING
    pub state: String,
    pub body: String,
    pub created_at: String,
}

/// A GitHub issue in the list view.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct IssueSummary {
    pub number: u64,
    pub title: String,
    /// OPEN | CLOSED
    pub state: String,
    pub author: String,
    pub url: String,
    pub labels: Vec<String>,
    pub comment_count: u64,
    pub updated_at: String,
}

/// Full detail for a single issue.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct IssueDetail {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub author: String,
    pub url: String,
    pub labels: Vec<String>,
    pub comments: Vec<Comment>,
}

/// A pull request in the list view.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PrSummary {
    pub number: u64,
    pub title: String,
    /// OPEN | CLOSED | MERGED
    pub state: String,
    pub author: String,
    pub head_ref: String,
    pub base_ref: String,
    pub url: String,
    pub is_draft: bool,
    pub review_decision: Option<String>,
    pub checks: Option<String>,
    pub updated_at: String,
}

/// A planned submit step, shown in the submit preview dialog.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubmitStepInfo {
    pub branch: String,
    pub base: String,
    /// "create" | "update" | "uptodate"
    pub action: String,
    pub pr: Option<u64>,
    /// Suggested PR title for branches that will be created.
    pub default_title: String,
}
