//! Releases: the list, a release with its files, and the release form.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

labels! {
    /// Fixed text of the release pages.
    ReleaseLabel {
        Releases => "releases.title", "Releases";
        NewRelease => "releases.new", "New release";
        EditRelease => "releases.edit-title", "Edit release";
        NoReleasesYet => "releases.empty-title", "No releases yet";
        EmptyForWriters => "releases.empty-writer", "A release packages a tag with notes and files. Push a tag, then create a release for it.";
        EmptyForReaders => "releases.empty", "Releases mark versions of this project with notes and downloads.";
        Draft => "releases.draft", "Draft";
        PreRelease => "releases.prerelease", "Pre-release";
        Latest => "releases.latest", "Latest";
        Bot => "releases.bot", "bot";
        Created => "releases.created", "created";
        Edit => "releases.edit", "Edit";
        Downloads => "releases.downloads", "Downloads";
        SourceZip => "releases.source-zip", "Source code (zip)";
        SourceTarGz => "releases.source-tar", "Source code (tar.gz)";
        Files => "releases.files", "Files";
        Remove => "releases.remove", "Remove";
        FileToAttach => "releases.file-label", "File to attach";
        Upload => "releases.upload", "Upload";
        DeleteTitle => "releases.delete-title", "Delete this release";
        DeleteHintStart => "releases.delete-hint-start", "The release, its notes and its files are removed. The tag";
        DeleteHintEnd => "releases.delete-hint-end", "stays.";
        DeleteRelease => "releases.delete", "Delete release";
        FormLead => "releases.form-lead", "A release describes a tag: its notes, and files to download.";
        Tag => "releases.tag", "Tag";
        AnExisting => "releases.tag-hint-start", "An existing";
        TagLink => "releases.tag-link", "tag";
        OrNewTag => "releases.tag-hint-end", "or a new one created from the branch below.";
        CreateTagFrom => "releases.target", "Create a new tag from";
        TargetHint => "releases.target-hint", "used only when the tag does not exist";
        OneOfTags => "releases.tags-hint-start", "One of the repository's";
        TagsLink => "releases.tags-link", "tags";
        Title => "releases.name", "Title";
        TitleHint => "releases.name-hint", "optional, defaults to the tag";
        Notes => "releases.notes", "Notes";
        IsPreRelease => "releases.prerelease-label", "This is a pre-release";
        PreReleaseHint => "releases.prerelease-hint", "Marked as not ready for production; a full release is preferred as the latest.";
        SaveRelease => "releases.save", "Save release";
        PublishRelease => "releases.publish", "Publish release";
        SaveDraft => "releases.save-draft", "Save draft";
        Cancel => "releases.cancel", "Cancel";
    }
}

/// Named, sized or counted text of the release pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseMessage<'a> {
    /// The checksum title of a file link: "SHA-256 ab12…".
    Checksum { sha256: &'a str },
    /// A file's download count: "3 downloads".
    Downloads { count: u64 },
    /// A file's remove button: "Remove app.tar.gz".
    RemoveFile { name: &'a str },
    /// The upload limit: "Up to 100 MiB per file. Downloads are always served as files, never displayed."
    UploadLimit { size: &'a str },
    /// After an upload: "Uploaded app.tar.gz."
    Uploaded { name: &'a str },
    /// After a removal: "Removed app.tar.gz."
    Removed { name: &'a str },
    /// The edit form's document title: "Edit v1.0".
    EditTitle { tag: &'a str },
}

impl MessageFamily for ReleaseMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::Checksum { .. } => "releases.checksum",
            Self::Downloads { .. } => "releases.download-count",
            Self::RemoveFile { .. } => "releases.remove-file",
            Self::UploadLimit { .. } => "releases.upload-limit",
            Self::Uploaded { .. } => "releases.uploaded",
            Self::Removed { .. } => "releases.removed",
            Self::EditTitle { .. } => "releases.edit-document-title",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::Checksum { sha256 } => {
                text.literal("SHA-256 ");
                text.argument(sha256);
            }
            Self::Downloads { count } => {
                text.number(count);
                text.literal(if count == 1 { " download" } else { " downloads" });
            }
            Self::RemoveFile { name } => {
                text.literal("Remove ");
                text.argument(name);
            }
            Self::UploadLimit { size } => {
                text.literal("Up to ");
                text.argument(size);
                text.literal(" per file. Downloads are always served as files, never displayed.");
            }
            Self::Uploaded { name } => {
                text.literal("Uploaded ");
                text.argument(name);
                text.literal(".");
            }
            Self::Removed { name } => {
                text.literal("Removed ");
                text.argument(name);
                text.literal(".");
            }
            Self::EditTitle { tag } => {
                text.literal("Edit ");
                text.argument(tag);
            }
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

lookup!(ReleaseMessage<'_>);
