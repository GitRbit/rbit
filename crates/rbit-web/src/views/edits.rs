//! The file editor, the delete confirmation and the upload form, and the commit box they
//! share.
//!
//! Milestone 3 workstream E (editing files in the browser) owns this file.

use askama::Template;
use askama_web::WebTemplate;

use crate::views::{components::FormError, layout::Page, repo::RepoHeader};

/// The commit section of every edit form: message, and where the commit goes.
#[derive(Debug, Clone)]
pub struct CommitBox {
    /// The title as typed (empty: `title_placeholder` is used).
    pub title: String,
    /// The default title for this change.
    pub title_placeholder: String,
    pub description: String,
    /// The branch the editor was opened on.
    pub branch: String,
    /// The commit the editor was opened at, sent back with the form.
    pub parent: String,
    /// Committing to `branch` is refused (by branch protection): the reason.
    pub protected_reason: Option<String>,
    /// "Create a new branch" is the chosen option.
    pub new_branch_selected: bool,
    /// The new branch's name (suggested or as typed).
    pub new_branch: String,
}

impl CommitBox {
    /// Whether committing to the branch directly is offered.
    pub fn direct_allowed(&self) -> bool {
        self.protected_reason.is_none()
    }
}

/// The branch moved since the form was opened.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub branch: String,
    /// The branch's new tip (short), or `None` when the branch is gone.
    pub current_short: Option<String>,
    /// The branch's history, to review what changed.
    pub history_url: String,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/edit_file.html")]
pub struct EditFilePage {
    pub page: Page,
    pub header: RepoHeader,
    /// `true` for a new file, `false` when editing one.
    pub is_new: bool,
    /// The file being edited (empty for a new file).
    pub original_path: String,
    pub action_url: String,
    pub cancel_url: String,
    /// The path field: the file's path (a new name renames it).
    pub path: String,
    pub content: String,
    /// Why the file cannot be edited in the browser (binary or too large); the editor
    /// is replaced by this message.
    pub not_editable: Option<String>,
    pub is_markdown: bool,
    /// Integration hook (W): the Markdown preview endpoint. `None` until `POST
    /// /markdown/preview` exists; the template then offers the Preview tab.
    pub preview_url: Option<String>,
    /// "1 MiB": the largest text that can be committed here.
    pub max_size: String,
    pub commit: CommitBox,
    pub conflict: Option<Conflict>,
    pub error: FormError,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/delete_file.html")]
pub struct DeleteFilePage {
    pub page: Page,
    pub header: RepoHeader,
    pub path: String,
    pub action_url: String,
    pub cancel_url: String,
    pub commit: CommitBox,
    pub conflict: Option<Conflict>,
    pub error: FormError,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/upload_files.html")]
pub struct UploadFilesPage {
    pub page: Page,
    pub header: RepoHeader,
    /// The directory the files go to, for display (`/` for the root).
    pub directory: String,
    pub action_url: String,
    pub cancel_url: String,
    /// "50 MiB": the largest file.
    pub max_file_size: String,
    pub max_files: usize,
    pub commit: CommitBox,
    pub conflict: Option<Conflict>,
    pub error: FormError,
}
