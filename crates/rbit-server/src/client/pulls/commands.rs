//! Pull request command arguments.

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum PrCommand {
    /// Open a pull request from a branch.
    Create {
        #[arg(short, long)]
        title: String,
        #[arg(short, long, default_value = "")]
        body: String,
        /// The branch to merge into; the default branch when not given.
        #[arg(short = 'B', long)]
        base: Option<String>,
        /// The branch with the changes; the current branch when not given.
        #[arg(short = 'H', long)]
        head: Option<String>,
        #[arg(short, long)]
        draft: bool,
    },
    /// Publish or recover one draft pull request for a branch: an existing one is returned,
    /// and `--request-id` makes a retry return the first answer.
    Ensure {
        #[arg(short, long, default_value = "")]
        title: String,
        #[arg(short, long, default_value = "")]
        body: String,
        #[arg(short = 'B', long)]
        base: Option<String>,
        #[arg(short = 'H', long)]
        head: Option<String>,
        /// Open it ready for review instead of as a draft.
        #[arg(long)]
        ready: bool,
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Whether a pull request can merge, by the merge's own rules: exit 0 when ready, 1 when
    /// a required check failed, 8 while something is still expected or running.
    Ready {
        number: u64,
        /// The head you expect; a different one is `head_changed` (exit 5).
        #[arg(long)]
        head: Option<String>,
        /// Mark a draft ready for review first.
        #[arg(long)]
        mark: bool,
        /// Wait until it is ready or a required check failed.
        #[arg(long)]
        watch: bool,
        #[arg(long, default_value_t = 3600)]
        timeout: u64,
    },
    /// Follow a pull request until it is ready for a human handoff or needs attention.
    #[command(visible_alias = "full-stop")]
    Babysit {
        number: u64,
        /// Seconds to wait. Zero evaluates the current snapshot.
        #[arg(long, default_value_t = 3600)]
        timeout: u64,
    },
    /// Queue the reviewed head for merge and wait for the queue's result.
    FullSend {
        number: u64,
        /// The full commit id of the reviewed pull request head.
        #[arg(long)]
        head: String,
        #[arg(long, conflicts_with = "rebase")]
        squash: bool,
        #[arg(long, conflicts_with = "squash")]
        rebase: bool,
        /// Seconds to wait after queueing. Zero returns the immediate queue state.
        #[arg(long, default_value_t = 7200)]
        timeout: u64,
    },
    /// Queue a pull request to land only after its merge on the current base passed CI;
    /// `--watch` waits: exit 0 when it landed, 1 when it failed, 5 when taken out of line.
    Queue {
        number: u64,
        #[arg(long)]
        head: Option<String>,
        #[arg(long, conflicts_with = "rebase")]
        squash: bool,
        #[arg(long)]
        rebase: bool,
        #[arg(long)]
        watch: bool,
        #[arg(long, default_value_t = 7200)]
        timeout: u64,
    },
    /// Take a pull request out of the merge queue.
    Dequeue { number: u64 },
    /// Merge the base into a pull request's head branch, if the head is still `--head`.
    UpdateBranch {
        number: u64,
        #[arg(long)]
        head: String,
    },
    /// Revert a commit into a new branch and a pull request (never merged on its own).
    Revert {
        commit: String,
        #[arg(short = 'B', long)]
        base: Option<String>,
    },
    /// Open pull requests.
    List {
        #[arg(short, long, default_value = "open")]
        state: String,
        /// Only pull requests from this branch.
        #[arg(short = 'H', long)]
        head: Option<String>,
        #[arg(short = 'L', long, default_value_t = 30)]
        limit: u32,
    },
    /// A pull request.
    View { number: u64 },
    /// Assess an older pull request against its base without changing any refs.
    Modernize { number: u64 },
    /// Collect the revisions, review state, and changes needed to take over a pull request.
    Takeover { number: u64 },
    /// Post a PR discussion comment or reply to an inline review thread.
    Comment {
        number: u64,
        #[arg(long, required_unless_present = "body_file", conflicts_with = "body_file")]
        body: Option<String>,
        /// Read the comment from a file, or standard input with `-`.
        #[arg(long)]
        body_file: Option<std::path::PathBuf>,
        /// Reply to this inline review comment instead of posting to the discussion.
        #[arg(long)]
        reply_to: Option<u64>,
    },
    /// Get a PR snapshot or wait for checks, reviews, or the PR itself to change.
    Watch {
        number: u64,
        /// The cursor from a previous watch. Without it, return immediately.
        #[arg(long)]
        cursor: Option<String>,
        /// Seconds to wait for a change (at most 25).
        #[arg(long, default_value_t = 25)]
        timeout: u64,
    },
    /// Merge a pull request, only if its head is still `--head` (by default the head read
    /// just before merging).
    Merge {
        number: u64,
        #[arg(long, conflicts_with_all = ["squash", "rebase"])]
        merge: bool,
        #[arg(long, conflicts_with = "rebase")]
        squash: bool,
        #[arg(long)]
        rebase: bool,
        /// The head commit that may be merged; a newer head is refused (`head_changed`).
        #[arg(long)]
        head: Option<String>,
    },
    /// The checks of a pull request's head: exit 0 when all passed, 1 when one failed, 8
    /// while some are pending or none has been reported yet.
    Checks {
        number: u64,
        /// Wait until every check finished.
        #[arg(long)]
        watch: bool,
        #[arg(short, long, default_value_t = 5)]
        interval: u64,
    },
}
