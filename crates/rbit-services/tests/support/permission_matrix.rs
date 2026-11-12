//! The permission regression world shared by the services, web and SSH ingress tests.
//!
//! Every actor relationship of the mandatory permission group (anonymous, unrelated,
//! collaborator read/write/admin, organisation member and team member, organisation
//! owner, repository owner, site administrator and a suspended account) is set up against
//! personal and organisation repositories that are public or private, each also archived.
//!
//! [`oracle`] is the expected access written out as a table. The services test proves the
//! table equal to the pure policy for the [`relation`] each actor was given; every ingress
//! test then compares its actual decision with the table, so the ingress binaries that do not
//! link the policy crate still assert against the same proven values.
#![allow(dead_code, reason = "each ingress binary uses a different part of the shared world")]

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use rbit_core::{
    OrgRole, RepoAccess, RepoId, User, Visibility,
    membership::{BasePermission, Permission},
};
use rbit_services::{CreateRepository, Services, issues::NewIssueInput, orgs::CreateOrganization};
use rbit_storage::{Database, NewUser};

/// Who makes a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Actor {
    Anonymous,
    /// Signed in, with no relationship to any repository.
    Unrelated,
    CollaboratorRead,
    CollaboratorWrite,
    CollaboratorAdmin,
    /// A member of `acme`, whose base permission is `read`, in no team.
    OrgMember,
    /// A member of `acme` in the team `core`, which has `write` on every `acme` repository.
    TeamMember,
    /// The owner of `acme`.
    OrgOwner,
    /// `alice`, who owns the personal repositories.
    RepoOwner,
    SiteAdmin,
    /// A write collaborator everywhere whose account is suspended after credentials exist.
    Suspended,
}

pub const ACTORS: [Actor; 11] = [
    Actor::Anonymous,
    Actor::Unrelated,
    Actor::CollaboratorRead,
    Actor::CollaboratorWrite,
    Actor::CollaboratorAdmin,
    Actor::OrgMember,
    Actor::TeamMember,
    Actor::OrgOwner,
    Actor::RepoOwner,
    Actor::SiteAdmin,
    Actor::Suspended,
];

impl Actor {
    /// The account's name; `None` for anonymous visitors.
    pub fn login(self) -> Option<&'static str> {
        Some(match self {
            Self::Anonymous => return None,
            Self::Unrelated => "eve",
            Self::CollaboratorRead => "reader",
            Self::CollaboratorWrite => "writer",
            Self::CollaboratorAdmin => "maintainer",
            Self::OrgMember => "member",
            Self::TeamMember => "tess",
            Self::OrgOwner => "olga",
            Self::RepoOwner => "alice",
            Self::SiteAdmin => "root",
            Self::Suspended => "sam",
        })
    }

    /// Whether a credential of this actor is accepted once the world is
    /// [suspended](World::suspend): suspension is an authentication gate, not a policy input.
    pub fn authenticates(self) -> bool {
        !matches!(self, Self::Anonymous | Self::Suspended)
    }
}

/// The kind of repository a request is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shape {
    pub organization: bool,
    pub public: bool,
    pub archived: bool,
}

pub const SHAPES: [Shape; 8] = {
    let mut shapes = [Shape { organization: false, public: false, archived: false }; 8];
    let mut i = 0;
    while i < 8 {
        shapes[i] = Shape { organization: i & 4 != 0, public: i & 2 != 0, archived: i & 1 != 0 };
        i += 1;
    }
    shapes
};

impl Shape {
    pub fn owner(self) -> &'static str {
        if self.organization { "acme" } else { "alice" }
    }

    pub fn name(self) -> &'static str {
        match (self.public, self.archived) {
            (true, false) => "open",
            (true, true) => "open-archived",
            (false, false) => "closed",
            (false, true) => "closed-archived",
        }
    }

    pub fn full_name(self) -> String {
        format!("{}/{}", self.owner(), self.name())
    }

    fn visibility(self) -> Visibility {
        if self.public { Visibility::Public } else { Visibility::Private }
    }
}

/// The relationship the world gives `actor` to a repository of `shape`, as inputs of the
/// pure policy; `None` for anonymous visitors. Archiving changes no relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Relation {
    pub site_admin: bool,
    pub repository_owner: bool,
    pub org_role: Option<OrgRole>,
    pub org_base: Option<Permission>,
    pub team: Option<Permission>,
    pub collaborator: Option<Permission>,
}

pub fn relation(actor: Actor, shape: Shape) -> Option<Relation> {
    let org = shape.organization;
    let member = |team| Relation { org_role: Some(OrgRole::Member), org_base: Some(Permission::Read), team, ..Relation::default() };
    let collaborator = |permission| Relation { collaborator: Some(permission), ..Relation::default() };
    Some(match actor {
        Actor::Anonymous => return None,
        Actor::CollaboratorRead => collaborator(Permission::Read),
        Actor::CollaboratorWrite | Actor::Suspended => collaborator(Permission::Write),
        Actor::CollaboratorAdmin => collaborator(Permission::Admin),
        Actor::OrgMember if org => member(None),
        Actor::TeamMember if org => member(Some(Permission::Write)),
        Actor::OrgOwner if org => Relation { org_role: Some(OrgRole::Owner), org_base: Some(Permission::Read), ..Relation::default() },
        Actor::Unrelated | Actor::OrgMember | Actor::TeamMember | Actor::OrgOwner => Relation::default(),
        Actor::RepoOwner => Relation { repository_owner: !org, ..Relation::default() },
        Actor::SiteAdmin => Relation { site_admin: true, ..Relation::default() },
    })
}

/// The expected repository access, before any authentication gate. Columns: `alice/open`,
/// `alice/closed`, `acme/open`, `acme/closed`; archived copies share their column.
pub fn oracle(actor: Actor, shape: Shape) -> RepoAccess {
    use RepoAccess::{Admin as A, None as N, Read as R, Write as W};
    let row = match actor {
        Actor::Anonymous | Actor::Unrelated => [R, N, R, N],
        Actor::CollaboratorRead => [R, R, R, R],
        Actor::CollaboratorWrite | Actor::Suspended => [W, W, W, W],
        Actor::CollaboratorAdmin | Actor::SiteAdmin => [A, A, A, A],
        Actor::OrgMember => [R, N, R, R],
        Actor::TeamMember => [R, N, W, W],
        Actor::OrgOwner => [R, N, A, A],
        Actor::RepoOwner => [A, A, R, N],
    };
    row[usize::from(shape.organization) * 2 + usize::from(!shape.public)]
}

/// What an ingress lets `actor` do once suspension is in force: a refused credential.
/// Cookie-based browsing falls back to anonymous; other ingresses refuse the credential.
pub fn effective(actor: Actor, shape: Shape) -> RepoAccess {
    if actor == Actor::Suspended { oracle(Actor::Anonymous, shape) } else { oracle(actor, shape) }
}

/// One repository of the world.
#[derive(Debug, Clone)]
pub struct Repo {
    pub shape: Shape,
    pub id: RepoId,
    pub disk_path: PathBuf,
    /// The commit every branch of the seeded repository points at.
    pub head: String,
    /// The number of the one issue seeded in the repository before it is archived.
    pub issue: u32,
}

/// The users and repositories of the regression world.
#[derive(Debug)]
pub struct World {
    pub users: Vec<(Actor, User)>,
    pub repos: Vec<Repo>,
}

impl World {
    /// Builds the world. Every repository holds one commit on `main` and, for each
    /// account and each name in `probe_prefixes`, a branch `{prefix}-{login}` that ingress
    /// tests may delete. Nobody is suspended yet: issue credentials, then [`Self::suspend`].
    pub async fn build(services: &Services, database: &Database, scratch: &Path, probe_prefixes: &[&str]) -> Self {
        let mut users = Vec::new();
        for actor in ACTORS {
            let Some(login) = actor.login() else { continue };
            let new = NewUser {
                name: login.parse().unwrap(),
                email: format!("{login}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin: actor == Actor::SiteAdmin,
            };
            users.push((actor, database.accounts().create_user(new).await.unwrap()));
        }
        let world = Self { users, repos: Vec::new() };
        let olga = world.user(Actor::OrgOwner).unwrap();
        let acme =
            services.orgs.create(olga, CreateOrganization { name: "acme".to_owned(), ..CreateOrganization::default() }).await.unwrap();
        let acme = acme.account.id;
        database.orgs().set_base_permission(acme, BasePermission::Read).await.unwrap();
        for actor in [Actor::OrgMember, Actor::TeamMember] {
            database.orgs().add_member(acme, world.user(actor).unwrap().id(), OrgRole::Member).await.unwrap();
        }
        let team = database.teams().create(acme, &"core".parse().unwrap(), None).await.unwrap();
        database.teams().add_member(team.id, world.user(Actor::TeamMember).unwrap().id()).await.unwrap();

        let source = seed_source(scratch);
        let head = git(&source, &["rev-parse", "HEAD"]);
        let mut refspecs = vec!["HEAD:refs/heads/main".to_owned()];
        for (_, user) in &world.users {
            refspecs.extend(probe_prefixes.iter().map(|prefix| format!("HEAD:refs/heads/{prefix}-{}", user.name())));
        }
        let mut repos = Vec::new();
        for shape in SHAPES {
            let creator = world.user(if shape.organization { Actor::OrgOwner } else { Actor::RepoOwner }).unwrap();
            let input = CreateRepository { name: shape.name().to_owned(), description: String::new(), visibility: shape.visibility() };
            let id = services.repositories.create_in(creator, shape.owner(), input).await.unwrap().repo.id;
            if shape.organization {
                database.teams().set_repository(team.id, id, Permission::Write).await.unwrap();
            }
            for (actor, permission) in [
                (Actor::CollaboratorRead, Permission::Read),
                (Actor::CollaboratorWrite, Permission::Write),
                (Actor::CollaboratorAdmin, Permission::Admin),
                (Actor::Suspended, Permission::Write),
            ] {
                database.collaborators().add(id, world.user(actor).unwrap().id(), permission, creator.id()).await.unwrap();
            }
            let ctx = services.repositories.open(Some(creator), shape.owner(), shape.name()).await.unwrap();
            let mut push = vec!["push".to_owned(), "-q".to_owned(), ctx.disk_path.to_str().unwrap().to_owned()];
            push.extend(refspecs.iter().cloned());
            git(&source, &push.iter().map(String::as_str).collect::<Vec<_>>());
            // Seeded before archiving: an archived repository takes no new issues.
            let seeded = NewIssueInput {
                title: "seeded".to_owned(),
                body: String::new(),
                labels: Vec::new(),
                assignees: Vec::new(),
                milestone: None,
            };
            let issue = services.issues.create(creator, &ctx, seeded).await.unwrap().number.get();
            if shape.archived {
                assert!(services.repo_settings.set_archived(creator, &ctx, true).await.unwrap());
            }
            repos.push(Repo { shape, id, disk_path: ctx.disk_path, head: head.clone(), issue });
        }
        Self { repos, ..world }
    }

    /// The account of `actor`; `None` for anonymous visitors.
    pub fn user(&self, actor: Actor) -> Option<&User> {
        self.users.iter().find(|(candidate, _)| *candidate == actor).map(|(_, user)| user)
    }

    pub fn repo(&self, shape: Shape) -> &Repo {
        self.repos.iter().find(|repo| repo.shape == shape).unwrap()
    }

    /// Suspends the suspended actor's account; its credentials already exist.
    pub async fn suspend(&self, database: &Database) {
        database.admin().set_suspended(self.user(Actor::Suspended).unwrap().id(), true).await.unwrap();
    }
}

/// A working repository with one commit, to push into every repository of the world.
fn seed_source(scratch: &Path) -> PathBuf {
    let source = scratch.join("permission-seed");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "-q", "-b", "main"]);
    std::fs::write(source.join("README.md"), "# seeded\n").unwrap();
    git(&source, &["add", "README.md"]);
    git(&source, &["commit", "-q", "-m", "seed"]);
    source
}

/// Runs the real `git` with a fixed identity and no user configuration.
pub fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
