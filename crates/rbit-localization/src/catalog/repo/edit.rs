//! Editing files in the browser: the editor, deleting, uploading and the commit box.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

labels! {
    /// Fixed text of the file editor, delete and upload pages.
    RepoEditLabel {
        NewFile => "repo.edit.new-file", "New file";
        Edit => "repo.edit.edit", "Edit";
        On => "repo.edit.on", "On";
        NewFileHint => "repo.edit.new-hint", "Type a path with / to create directories.";
        EditHint => "repo.edit.edit-hint", "Change the path to rename or move the file.";
        BackToFile => "repo.edit.back", "Back to the file";
        Path => "repo.edit.path", "Path";
        Content => "repo.edit.content", "Content";
        CommitChanges => "repo.edit.commit", "Commit changes";
        Cancel => "repo.edit.cancel", "Cancel";
        Delete => "repo.delete.delete", "Delete";
        DeleteHint => "repo.delete.hint", "The file stays in the history; a directory left empty disappears.";
        DeleteFile => "repo.delete.submit", "Delete file";
        UploadFiles => "repo.upload.title", "Upload files";
        Into => "repo.upload.into", "Into";
        OnBranch => "repo.upload.on", "on";
        UploadHint => "repo.upload.hint", "Files with the same name are replaced.";
        Files => "repo.upload.files", "Files";
        NothingCommitted => "repo.commit-box.nothing", "Nothing was committed.";
        ReviewNewCommits => "repo.commit-box.review", "Review the new commits";
        KeptAndCommittedOnTop => "repo.commit-box.kept", "then commit again: your changes are kept below and will be committed on top of the new version.";
        BranchGone => "repo.commit-box.branch-gone", "Nothing was committed. Commit to a new branch instead; your changes are kept below.";
        CommitChangesLegend => "repo.commit-box.legend", "Commit changes";
        CommitMessage => "repo.commit-box.message", "Commit message";
        ExtendedDescription => "repo.commit-box.description", "Extended description";
        Optional => "repo.commit-box.optional", "optional";
        CommitDirectlyTo => "repo.commit-box.direct", "Commit directly to";
        NotAllowed => "repo.commit-box.not-allowed", "Not allowed:";
        NewBranchAndPull => "repo.commit-box.new-branch", "Create a new branch and start a pull request";
        NewBranchName => "repo.commit-box.new-branch-name", "New branch name";
    }
}

/// Named or sized text of the editing pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoEditMessage<'a> {
    /// The editor's size limit: "At most 1 MiB".
    AtMost { size: &'a str },
    /// The upload limits: "Up to 100 files of at most 10 MiB each, committed together."
    UploadLimits { files: usize, size: &'a str },
    /// The conflict heading when a branch moved: "main changed since you started".
    BranchChanged { branch: &'a str },
    /// The conflict heading when a branch was deleted: "main no longer exists".
    BranchMissing { branch: &'a str },
    /// Before the commit the branch now points at: "It now points at".
    NowPointsAt,
    /// A file that cannot be edited: binary or over `limit`.
    NotEditable { limit: &'a str },
    /// The editor's document title: "Editing src/main.rs".
    EditingTitle { path: &'a str },
    /// The delete page's document title: "Delete src/main.rs".
    DeleteTitle { path: &'a str },
}

impl MessageFamily for RepoEditMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::AtMost { .. } => "repo.edit.at-most",
            Self::UploadLimits { .. } => "repo.upload.limits",
            Self::BranchChanged { .. } => "repo.commit-box.changed",
            Self::BranchMissing { .. } => "repo.commit-box.missing",
            Self::NowPointsAt => "repo.commit-box.now-points-at",
            Self::NotEditable { .. } => "repo.edit.not-editable",
            Self::EditingTitle { .. } => "repo.edit.document-title",
            Self::DeleteTitle { .. } => "repo.delete.document-title",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::AtMost { size } => {
                text.literal("At most ");
                text.argument(size);
            }
            Self::UploadLimits { files, size } => {
                text.literal("Up to ");
                text.number(files as u64);
                text.literal(if files == 1 { " file of at most " } else { " files of at most " });
                text.argument(size);
                text.literal(" each, committed together.");
            }
            Self::BranchChanged { branch } => {
                text.argument(branch);
                text.literal(" changed since you started");
            }
            Self::BranchMissing { branch } => {
                text.argument(branch);
                text.literal(" no longer exists");
            }
            Self::NowPointsAt => text.literal("It now points at"),
            Self::NotEditable { limit } => {
                text.literal("This file is binary or larger than ");
                text.argument(limit);
                text.literal(", so it cannot be edited in the browser. You can delete it, or upload a replacement.");
            }
            Self::EditingTitle { path } => {
                text.literal("Editing ");
                text.argument(path);
            }
            Self::DeleteTitle { path } => {
                text.literal("Delete ");
                text.argument(path);
            }
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

lookup!(RepoEditMessage<'_>);
