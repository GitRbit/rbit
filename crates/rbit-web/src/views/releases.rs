//! Release list, release page and the release editor.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::release::{Release, ReleaseAsset};
use rbit_services::releases::ReleaseDetail;

use crate::{
    render::{format_size, render_markdown},
    urls::{DocumentBase, RepoUrls},
    views::{
        components::{FormError, Pagination},
        layout::Page,
        repo::RepoHeader,
    },
};

/// An attached file as listed under a release.
#[derive(Debug, Clone)]
pub struct AssetLink {
    pub id: i64,
    pub name: String,
    pub size: String,
    pub download_count: u64,
    pub sha256: String,
    pub url: String,
    /// `POST` target that deletes the file (writers only).
    pub delete_url: String,
}

/// A release as shown in the list and on its own page.
#[derive(Debug, Clone)]
pub struct ReleaseCard {
    pub tag: String,
    pub name: String,
    /// Sanitised HTML of the notes; empty when there are none.
    pub notes_html: String,
    pub is_draft: bool,
    pub is_prerelease: bool,
    pub is_latest: bool,
    pub author: Option<String>,
    pub when: String,
    pub when_iso: String,
    pub url: String,
    pub edit_url: String,
    /// Browse the repository at the tag.
    pub tag_url: String,
    pub zip_url: String,
    pub tar_url: String,
    pub assets: Vec<AssetLink>,
}

impl ReleaseCard {
    /// `latest` is the id of the repository's latest release, marked with a badge.
    pub fn new(detail: &ReleaseDetail, latest: Option<&Release>, urls: &RepoUrls) -> Self {
        let release = &detail.release;
        let tag = &release.tag_name;
        let shown = release.shown_at();
        // Notes link relative to the repository as of the tag.
        let base = DocumentBase::new(urls.clone(), tag.clone(), "");
        Self {
            tag: tag.clone(),
            name: release.name.clone(),
            notes_html: if release.notes.trim().is_empty() { String::new() } else { render_markdown(&release.notes, Some(&base)) },
            is_draft: release.is_draft,
            is_prerelease: release.is_prerelease,
            is_latest: latest.is_some_and(|l| l.id == release.id),
            author: release.author.as_ref().map(|a| a.name.clone()),
            when: shown.relative(),
            when_iso: shown.iso8601(),
            url: urls.release(tag),
            edit_url: urls.release_action(release.id.0, "edit"),
            tag_url: urls.tree(&format!("refs/tags/{tag}"), ""),
            zip_url: urls.archive(tag, ".zip"),
            tar_url: urls.archive(tag, ".tar.gz"),
            assets: detail.assets.iter().map(|asset| AssetLink::new(asset, release, urls)).collect(),
        }
    }
}

impl AssetLink {
    fn new(asset: &ReleaseAsset, release: &Release, urls: &RepoUrls) -> Self {
        Self {
            id: asset.id.0,
            name: asset.name.clone(),
            size: format_size(asset.size),
            download_count: asset.download_count,
            sha256: asset.sha256_hex(),
            url: urls.release_download(&release.tag_name, &asset.name),
            delete_url: urls.release_action(release.id.0, &format!("assets/{}/delete", asset.id.0)),
        }
    }
}

/// The latest release in the repository sidebar.
#[derive(Debug, Clone)]
pub struct LatestRelease {
    pub name: String,
    pub tag: String,
    pub url: String,
    pub when: String,
    pub when_iso: String,
    pub releases_url: String,
}

impl LatestRelease {
    pub fn new(release: &Release, urls: &RepoUrls) -> Self {
        let shown = release.shown_at();
        Self {
            name: release.name.clone(),
            tag: release.tag_name.clone(),
            url: urls.release(&release.tag_name),
            when: shown.relative(),
            when_iso: shown.iso8601(),
            releases_url: urls.releases(),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "releases/list.html")]
pub struct ReleasesPage {
    pub page: Page,
    pub header: RepoHeader,
    pub releases: Vec<ReleaseCard>,
    pub pagination: Pagination,
    pub can_write: bool,
    pub new_url: String,
    pub tags_url: String,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "releases/show.html")]
pub struct ReleasePage {
    pub page: Page,
    pub header: RepoHeader,
    pub release: ReleaseCard,
    pub can_write: bool,
    pub releases_url: String,
    pub delete_url: String,
    /// Where files are uploaded (`multipart/form-data`); writers only.
    pub upload_url: String,
    pub max_asset_size: String,
    /// A message about the last upload or deletion (`?uploaded=…`), if any.
    pub notice: Option<String>,
    pub error: FormError,
}

/// The release editor's field values.
#[derive(Debug, Clone, Default)]
pub struct ReleaseForm {
    pub tag: String,
    pub name: String,
    pub notes: String,
    pub is_draft: bool,
    pub is_prerelease: bool,
    /// The branch a new tag is created from (new releases only).
    pub target: String,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "releases/form.html")]
pub struct ReleaseFormPage {
    pub page: Page,
    pub header: RepoHeader,
    pub error: FormError,
    pub form: ReleaseForm,
    /// Existing tags, newest first, offered as suggestions.
    pub tags: Vec<String>,
    /// Branches a new tag can be created from.
    pub branches: Vec<String>,
    pub action_url: String,
    pub cancel_url: String,
    /// `false` when editing an existing release.
    pub is_new: bool,
    /// Editing a release that is already published (no "save draft" button then).
    pub is_published: bool,
    pub tags_url: String,
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        AccountId, RepoId, Timestamp,
        release::{ReleaseAssetId, ReleaseAuthor, ReleaseId},
    };

    use super::*;

    fn detail(notes: &str) -> ReleaseDetail {
        let release = Release {
            id: ReleaseId(3),
            repo_id: RepoId(1),
            tag_name: "release/1.0".into(),
            name: "One".into(),
            notes: notes.into(),
            is_draft: false,
            is_prerelease: true,
            author: Some(ReleaseAuthor { id: AccountId(1), name: "alice".into() }),
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
            published_at: Some(Timestamp(0)),
        };
        let asset = ReleaseAsset {
            id: ReleaseAssetId(9),
            release_id: release.id,
            repo_id: RepoId(1),
            name: "app 1.zip".into(),
            size: 2048,
            content_type: "application/zip".into(),
            sha256: [0; 32],
            download_count: 4,
            uploader_id: None,
            created_at: Timestamp(0),
        };
        ReleaseDetail { release, assets: vec![asset] }
    }

    #[test]
    fn cards_link_everything_by_tag() {
        let urls = RepoUrls::new("alice", "demo");
        let detail = detail("See [the guide](docs/guide.md).");
        let card = ReleaseCard::new(&detail, Some(&detail.release), &urls);
        assert_eq!(card.url, "/alice/demo/releases/tag/release/1.0");
        assert_eq!(card.edit_url, "/alice/demo/releases/3/edit");
        assert_eq!(card.zip_url, "/alice/demo/archive/release/1.0.zip");
        assert_eq!(card.assets[0].url, "/alice/demo/releases/download/release/1.0/app%201.zip");
        assert_eq!(card.assets[0].delete_url, "/alice/demo/releases/3/assets/9/delete");
        assert_eq!(card.assets[0].size, "2.0 KB");
        assert!(card.is_latest && card.is_prerelease);
        assert!(card.notes_html.contains(r#"href="/alice/demo/blob/release/1.0/docs/guide.md""#), "{}", card.notes_html);
    }

    #[test]
    fn empty_notes_render_nothing() {
        let urls = RepoUrls::new("alice", "demo");
        assert_eq!(ReleaseCard::new(&detail("  \n"), None, &urls).notes_html, "");
    }
}
