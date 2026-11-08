//! Native Git object preparation/read with bounded headers and a flat stable-ID tree.
use super::{
    PrepareWikiPage, WikiObjectError,
    changes::{PrepareWikiChange, WikiTreeChange},
    exact_object, layout,
};
use rbit_core::RepoId;
use rbit_wikis::{
    MAX_WIKI_CONTENT_BYTES, WikiContent, WikiId, WikiPageId, WikiRevisionId, WikiStoreId, commands::WikiIntent, policy::WikiExpected,
};
use std::path::Path;

const MAX_COMMIT_BYTES: u64 = 64 * 1024;
const MAX_TREE_BYTES: u64 = 1024 * 1024;
const MAX_PAGES: usize = 10_000;

fn id(value: WikiRevisionId) -> Result<gix::ObjectId, WikiObjectError> {
    gix::ObjectId::try_from(value.as_bytes()).map_err(|_| WikiObjectError::Format)
}

fn checked_header(repo: &gix::Repository, id: gix::ObjectId, kind: gix::objs::Kind, limit: u64) -> Result<(), WikiObjectError> {
    let header = repo.find_header(id).map_err(crate::GitError::backend)?;
    if header.kind() != kind || header.size() > limit {
        return Err(WikiObjectError::Format);
    }
    Ok(())
}

fn tree(repo: &gix::Repository, revision: WikiRevisionId) -> Result<gix::Tree<'_>, WikiObjectError> {
    let id = id(revision)?;
    checked_header(repo, id, gix::objs::Kind::Commit, MAX_COMMIT_BYTES)?;
    let commit = repo.find_commit(id).map_err(crate::GitError::backend)?;
    let tree = commit.tree_id().map_err(crate::GitError::backend)?.detach();
    checked_header(repo, tree, gix::objs::Kind::Tree, MAX_TREE_BYTES)?;
    repo.find_tree(tree).map_err(|error| crate::GitError::backend(error).into())
}

/// One bounded streaming pass: no second decoded-entry vector or formatted name per
/// row. Flat blob names have the same Git and ordinary byte ordering.
fn visit_tree<'a>(tree: &'a gix::Tree<'_>, mut visit: impl FnMut(gix::objs::tree::EntryRef<'a>)) -> Result<(), WikiObjectError> {
    let mut previous = None;
    for (index, entry) in gix::objs::TreeRefIter::from_bytes(&tree.data, tree.id.kind()).enumerate() {
        if index >= MAX_PAGES {
            return Err(WikiObjectError::Format);
        }
        let entry = entry.map_err(crate::GitError::backend)?;
        let bytes: &[u8] = entry.filename.as_ref();
        let name = std::str::from_utf8(bytes).map_err(|_| WikiObjectError::Format)?;
        let valid = name.strip_suffix(".md").is_some_and(|stem| {
            !stem.is_empty()
                && stem.len() <= 19
                && !stem.starts_with('0')
                && stem.bytes().all(|byte| byte.is_ascii_digit())
                && stem.parse::<i64>().ok().and_then(WikiPageId::new).is_some()
        });
        if !valid || entry.mode.kind() != gix::objs::tree::EntryKind::Blob || previous.is_some_and(|previous| previous >= entry) {
            return Err(WikiObjectError::Format);
        }
        previous = Some(entry);
        visit(entry);
    }
    Ok(())
}

pub(super) fn read(
    root: &Path,
    identity: WikiStoreId,
    repository: RepoId,
    wiki: WikiId,
    page: WikiPageId,
    revision: WikiRevisionId,
) -> Result<WikiContent, WikiObjectError> {
    let repo = layout::repository(root, identity, repository, wiki, false)?;
    let tree = tree(&repo, revision)?;
    let filename = format!("{}.md", page.get());
    let mut blob = None;
    visit_tree(&tree, |entry| {
        let name: &[u8] = entry.filename.as_ref();
        if name == filename.as_bytes() {
            blob = Some(entry.oid.to_owned());
        }
    })?;
    let blob = blob.ok_or(WikiObjectError::Format)?;
    checked_header(&repo, blob, gix::objs::Kind::Blob, u64::try_from(MAX_WIKI_CONTENT_BYTES).map_err(|_| WikiObjectError::Format)?)?;
    let blob = repo.find_blob(blob).map_err(crate::GitError::backend)?.take_data();
    let text = String::from_utf8(blob).map_err(|_| WikiObjectError::Format)?;
    WikiContent::try_from(text).map_err(|_| WikiObjectError::Format)
}

pub(super) fn prepare(root: &Path, identity: WikiStoreId, request: &PrepareWikiPage) -> Result<WikiRevisionId, WikiObjectError> {
    prepare_page(root, identity, request, format!("account-{}", request.actor.0))
}

pub(super) fn prepare_intent(root: &Path, intent: &WikiIntent) -> Result<WikiRevisionId, WikiObjectError> {
    let (repository, wiki, head, page) = match intent.command.expected {
        WikiExpected::Create { repository, wiki, head } => (repository, wiki, head, intent.page),
        WikiExpected::Change { repository, wiki, head, page } => (repository, wiki, Some(head), page),
    };
    if page != intent.page
        || head.map_or(Some(rbit_wikis::WikiGeneration::INITIAL), |head| head.generation.checked_next()) != Some(intent.generation)
    {
        return Err(WikiObjectError::Binding);
    }
    let request = PageInput {
        repository,
        wiki,
        page,
        parent: head.map(|head| head.revision),
        entry: Entry::Upsert(&intent.command.content),
        actor: intent.command.actor,
        at: intent.accepted_at,
    };
    let revision = prepare_input(
        root,
        intent.store,
        &request,
        format!("account-{}-wiki-actor-{}", intent.command.actor.0, intent.actor_incarnation.get()),
    )?;
    if intent.target.is_some_and(|target| target != revision) {
        return Err(WikiObjectError::Binding);
    }
    Ok(revision)
}

/// The prepared commit's effect on the page's blob.
#[derive(Clone, Copy)]
enum Entry<'a> {
    Upsert(&'a WikiContent),
    Remove,
    Keep,
}

struct PageInput<'a> {
    repository: RepoId,
    wiki: WikiId,
    page: WikiPageId,
    parent: Option<WikiRevisionId>,
    entry: Entry<'a>,
    actor: rbit_core::AccountId,
    at: rbit_core::Timestamp,
}

fn prepare_page(
    root: &Path,
    identity: WikiStoreId,
    request: &PrepareWikiPage,
    actor_label: String,
) -> Result<WikiRevisionId, WikiObjectError> {
    prepare_input(
        root,
        identity,
        &PageInput {
            repository: request.repository,
            wiki: request.wiki,
            page: request.page,
            parent: request.parent,
            entry: Entry::Upsert(&request.content),
            actor: request.actor,
            at: request.at,
        },
        actor_label,
    )
}

fn prepare_input(
    root: &Path,
    identity: WikiStoreId,
    request: &PageInput<'_>,
    actor_label: String,
) -> Result<WikiRevisionId, WikiObjectError> {
    if request.actor.0 <= 0 {
        return Err(WikiObjectError::Format);
    }
    let repo = layout::repository(root, identity, request.repository, request.wiki, true)?;
    let mut entries = if let Some(parent) = request.parent {
        let tree = tree(&repo, parent)?;
        let mut entries = Vec::with_capacity((tree.data.len() / 32).min(MAX_PAGES));
        visit_tree(&tree, |entry| entries.push(gix::objs::tree::Entry::from(entry)))?;
        entries
    } else {
        Vec::new()
    };
    let filename = format!("{}.md", request.page.get());
    let position = entries.binary_search_by(|entry| {
        let stored: &[u8] = entry.filename.as_ref();
        stored.cmp(filename.as_bytes())
    });
    match (request.entry, position) {
        (Entry::Upsert(_), Err(_)) if entries.len() == MAX_PAGES => return Err(WikiObjectError::Format),
        (Entry::Upsert(content), position) => {
            let blob = exact_object::blob(&repo, content.as_str().as_bytes())?;
            let entry = gix::objs::tree::Entry { mode: gix::objs::tree::EntryKind::Blob.into(), filename: filename.into(), oid: blob };
            match position {
                Ok(index) => entries[index] = entry,
                Err(index) => entries.insert(index, entry),
            }
        }
        (Entry::Remove, Ok(index)) => {
            entries.remove(index);
        }
        (Entry::Keep, Ok(_)) => {}
        // Removing or keeping requires the page in the parent snapshot.
        (Entry::Remove | Entry::Keep, Err(_)) => return Err(WikiObjectError::Format),
    }
    let tree = exact_object::object(&repo, gix::objs::Tree { entries })?;
    let signature = gix::actor::Signature {
        name: actor_label.into(),
        email: "wiki@localhost".into(),
        time: gix::date::Time { seconds: request.at.seconds(), offset: 0 },
    };
    let commit = gix::objs::Commit {
        tree,
        parents: request.parent.map(id).transpose()?.into_iter().collect(),
        author: signature.clone(),
        committer: signature,
        encoding: None,
        message: match request.entry {
            Entry::Upsert(_) => "Wiki page update\n",
            Entry::Remove => "Wiki page deletion\n",
            Entry::Keep => "Wiki page rename\n",
        }
        .into(),
        extra_headers: Vec::new(),
    };
    let prepared = exact_object::object(&repo, commit)?;
    WikiRevisionId::try_from(prepared.as_slice()).map_err(|_| WikiObjectError::Format)
}

pub(super) fn prepare_change(root: &Path, identity: WikiStoreId, request: &PrepareWikiChange) -> Result<WikiRevisionId, WikiObjectError> {
    let entry = match &request.change {
        WikiTreeChange::Upsert(content) => Entry::Upsert(content),
        WikiTreeChange::Remove => Entry::Remove,
        WikiTreeChange::Keep => Entry::Keep,
    };
    prepare_input(
        root,
        identity,
        &PageInput {
            repository: request.repository,
            wiki: request.wiki,
            page: request.page,
            parent: request.parent,
            entry,
            actor: request.actor,
            at: request.at,
        },
        format!("account-{}", request.actor.0),
    )
}
