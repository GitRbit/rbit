//! Host rendering for declarative UI contributions. No descriptor supplies markup or authority.

use rbit_plugin_api::{UiContent, UiLink, UiMessage, UiSlot, UiVisibility};

use super::{layout::Page, org::OrgHeader, repo::RepoHeader};

#[derive(Debug, Clone)]
pub struct ExtensionLink {
    pub label: String,
    pub key: String,
    pub url: String,
    pub icon: &'static str,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct Extension {
    pub id: String,
    pub owner: String,
    pub generation: u64,
    pub kind: &'static str,
    pub tone: &'static str,
    pub title: String,
    pub body: String,
    pub links: Vec<ExtensionLink>,
}

#[derive(Default)]
struct Context<'a> {
    owner: Option<&'a str>,
    repo: Option<&'a str>,
    repository_reader: bool,
    repository_admin: bool,
    enabled_plugins: &'a [rbit_plugin_api::PluginId],
    organization_admin: bool,
}

impl Page {
    pub fn global_extensions(&self) -> Vec<Extension> {
        self.extensions(UiSlot::GlobalNav, &Context::default())
    }

    pub fn account_extensions(&self) -> Vec<Extension> {
        self.extensions(UiSlot::AccountSettings, &Context::default())
    }

    pub fn admin_extensions(&self) -> Vec<Extension> {
        self.extensions(UiSlot::AdminNav, &Context::default())
    }

    pub fn admin_panels(&self) -> Vec<Extension> {
        self.extensions(UiSlot::AdminPanel, &Context::default())
    }

    pub fn repository_tabs(&self, header: &RepoHeader) -> Vec<Extension> {
        self.extensions(UiSlot::RepositoryTab, &Context::repository(header))
    }

    pub fn repository_settings(&self, header: &RepoHeader) -> Vec<Extension> {
        self.extensions(UiSlot::RepositorySettings, &Context::repository(header))
    }

    pub fn repository_panels(&self, header: &RepoHeader) -> Vec<Extension> {
        self.extensions(UiSlot::RepositoryPanel, &Context::repository(header))
    }

    pub fn organization_settings(&self, header: &OrgHeader) -> Vec<Extension> {
        self.extensions(
            UiSlot::OrganizationSettings,
            &Context { owner: Some(&header.name), organization_admin: header.can_manage, ..Context::default() },
        )
    }

    /// The allowlisted enhancement is tied to the same published generation as its UI.
    pub fn actions_ui_generation(&self) -> Option<u64> {
        self.ui.entries.iter().find(|entry| entry.owner.as_str() == "rbit.actions").map(|entry| entry.generation.get())
    }

    /// Host-generated allowlist metadata, escaped into the document shell by Askama.
    /// Downloaded UI data never gets to supply executable asset URLs or module IDs.
    pub fn plugin_enhancements(&self) -> String {
        self.actions_ui_generation().map_or_else(
            || "[]".to_owned(),
            |generation| {
                serde_json::json!([{"owner": "rbit.actions", "generation": generation.to_string(), "id": "actions.log"}]).to_string()
            },
        )
    }

    fn extensions(&self, slot: UiSlot, context: &Context<'_>) -> Vec<Extension> {
        self.ui
            .entries
            .iter()
            .filter(|entry| entry.contribution.slot == slot && self.visible(entry.contribution.visibility, &entry.owner, context))
            .filter_map(|entry| {
                let mut view = Extension {
                    id: format!("{}:{}", entry.owner, entry.contribution.id),
                    owner: entry.owner.to_string(),
                    generation: entry.generation.get(),
                    kind: "link",
                    tone: "info",
                    title: String::new(),
                    body: String::new(),
                    links: Vec::new(),
                };
                match &entry.contribution.content {
                    UiContent::Link(link) => view.links.push(self.extension_link(link, context)?),
                    UiContent::Card { title, body, action } => {
                        view.kind = "card";
                        view.title = self.extension_message(title);
                        view.body = self.extension_message(body);
                        if let Some(link) = action {
                            view.links.push(self.extension_link(link, context)?);
                        }
                    }
                    UiContent::Notice { message, tone } => {
                        view.kind = "notice";
                        view.tone = tone.as_str();
                        view.body = self.extension_message(message);
                    }
                    UiContent::List { title, items } => {
                        view.kind = "list";
                        view.title = self.extension_message(title);
                        view.links = items.iter().map(|link| self.extension_link(link, context)).collect::<Option<_>>()?;
                    }
                }
                Some(view)
            })
            .collect()
    }

    fn visible(&self, visibility: UiVisibility, owner: &rbit_plugin_api::PluginId, context: &Context<'_>) -> bool {
        match visibility {
            UiVisibility::Public => true,
            UiVisibility::SignedIn => self.is_signed_in(),
            UiVisibility::InstanceAdmin => self.viewer.as_ref().is_some_and(|viewer| viewer.is_admin),
            UiVisibility::RepositoryReader => context.repository_reader,
            UiVisibility::RepositoryAdmin => context.repository_admin,
            UiVisibility::RepositoryFeatureEnabled => context.repository_reader && context.enabled_plugins.contains(owner),
            UiVisibility::OrganizationAdmin => context.organization_admin,
        }
    }

    fn extension_message(&self, message: &UiMessage) -> String {
        // Unknown plugin catalogs use their validated English text. Host schema messages
        // must be argument-free; descriptors cannot invent values for typed messages.
        if rbit_messages::MessageSchema::builtin().message(&message.namespace, &message.key).is_some_and(|spec| spec.args.is_empty()) {
            self.locale.text(&message.namespace, &message.key)
        } else {
            message.fallback.clone()
        }
    }

    fn extension_link(&self, link: &UiLink, context: &Context<'_>) -> Option<ExtensionLink> {
        let url = link.target.render(context.owner, context.repo).ok()?;
        let active = self.path == url || self.path.strip_prefix(&url).is_some_and(|rest| rest.starts_with('/'));
        let key = url.rsplit('/').next().unwrap_or_default().to_owned();
        Some(ExtensionLink { key, label: self.extension_message(&link.label), url, icon: link.icon.as_str(), active })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use rbit_plugin_api::{Generation, UiContribution, UiIcon};
    use rbit_services::ui::{UiEntry, UiSnapshot};
    use std::sync::Arc;

    fn message(text: &str) -> UiMessage {
        UiMessage { namespace: "example.plugin".into(), key: "title".into(), fallback: text.into() }
    }

    fn entry(slot: UiSlot, visibility: UiVisibility, content: UiContent) -> UiEntry {
        UiEntry {
            owner: "example.plugin".parse().unwrap(),
            generation: Generation::INITIAL,
            contribution: UiContribution { id: "example".parse().unwrap(), slot, visibility, order: 0, content },
        }
    }

    fn link(target: &str) -> UiContent {
        UiContent::Link(UiLink { label: message("Example"), target: target.parse().unwrap(), icon: UiIcon::None })
    }

    fn page(entries: Vec<UiEntry>) -> Page {
        Page {
            ui: UiSnapshot { revision: 1, entries: Arc::from(entries) },
            locale: Default::default(),
            has_i18n: false,
            updates: Default::default(),
            site_name: "rbit".into(),
            plugin_revision: 1,
            branding: Default::default(),
            title: None,
            viewer: None,
            path: "/alice/rocket/actions/42".into(),
            has_unread_notifications: false,
            theme: Default::default(),
        }
    }

    #[test]
    fn rendering_evaluates_repository_and_account_visibility_from_host_context() {
        let page = page(vec![
            entry(UiSlot::RepositoryTab, UiVisibility::RepositoryFeatureEnabled, link("/{owner}/{repo}/actions")),
            entry(UiSlot::RepositorySettings, UiVisibility::RepositoryAdmin, link("/{owner}/{repo}/settings/example")),
            entry(UiSlot::AccountSettings, UiVisibility::SignedIn, link("/settings/example")),
            entry(UiSlot::AdminNav, UiVisibility::InstanceAdmin, link("/admin/example")),
        ]);
        let mut header = RepoHeader { owner: "alice".into(), name: "rocket".into(), ..Default::default() };
        assert!(page.repository_tabs(&header).is_empty());
        assert!(page.repository_settings(&header).is_empty());
        assert!(page.account_extensions().is_empty());
        assert!(page.admin_extensions().is_empty());
        header.enabled_plugins = vec!["example.plugin".parse().unwrap()];
        let tabs = page.repository_tabs(&header);
        assert_eq!(tabs[0].links[0].url, "/alice/rocket/actions");
        assert!(tabs[0].links[0].active);
        header.can_admin = true;
        assert_eq!(page.repository_settings(&header).len(), 1);
        header.owner = "..".into();
        assert!(page.repository_tabs(&header).is_empty(), "invalid route parameters fail closed");
    }

    #[test]
    fn active_navigation_respects_path_boundaries() {
        let mut page = page(vec![entry(UiSlot::GlobalNav, UiVisibility::Public, link("/example"))]);
        page.path = "/example-other".into();
        assert!(!page.global_extensions()[0].links[0].active);
        page.path = "/example/details".into();
        assert!(page.global_extensions()[0].links[0].active);
    }

    #[test]
    fn declarative_content_is_escaped_and_unknown_catalogs_use_english_fallback() {
        #[derive(Template)]
        #[template(
            source = "{% import \"macros.html\" as ui %}{% for extension in page.repository_panels(header) %}{% include \"extensions/panel.html\" %}{% endfor %}",
            ext = "html"
        )]
        struct Fixture {
            page: Page,
            header: RepoHeader,
        }
        let page = page(vec![entry(
            UiSlot::RepositoryPanel,
            UiVisibility::RepositoryReader,
            UiContent::Card {
                title: message("<script>alert('title')</script>"),
                body: message("<img src=x onerror=alert(1)>"),
                action: Some(UiLink { label: message("<Click>"), target: "/{owner}/{repo}/example".parse().unwrap(), icon: UiIcon::Info }),
            },
        )]);
        let html =
            Fixture { page, header: RepoHeader { owner: "alice".into(), name: "rocket".into(), ..Default::default() } }.render().unwrap();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img"));
        assert!(html.contains("alert"));
        assert!(html.contains("/alice/rocket/example"));
        assert!(html.contains("data-plugin-owner=\"example.plugin\""));
    }

    #[test]
    fn only_host_allowlisted_owner_is_advertised_for_enhancement_loading() {
        let mut entry = entry(UiSlot::GlobalNav, UiVisibility::Public, link("/example"));
        assert_eq!(page(vec![entry.clone()]).plugin_enhancements(), "[]");
        entry.owner = "rbit.actions".parse().unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&page(vec![entry]).plugin_enhancements()).unwrap();
        assert_eq!(metadata[0]["id"], "actions.log");
        assert_eq!(metadata[0]["generation"], "1");
    }

    #[test]
    #[ignore = "manual comparative Askama rendering benchmark"]
    fn askama_extension_rendering_cost() {
        use std::{hint::black_box, time::Instant};

        #[derive(Template)]
        #[template(source = "{% for link in links %}<a href=\"{{ link.url }}\">{{ link.label }}</a>{% endfor %}", ext = "html")]
        struct CoreFixture<'a> {
            links: &'a [ExtensionLink],
        }
        #[derive(Template)]
        #[template(
            source = "{% for extension in page.global_extensions() %}{% for link in extension.links %}<a href=\"{{ link.url }}\">{{ link.label }}</a>{% endfor %}{% endfor %}",
            ext = "html"
        )]
        struct PluginFixture<'a> {
            page: &'a Page,
        }

        let mut entries = Vec::new();
        for index in 0..16 {
            let mut contribution = entry(UiSlot::GlobalNav, UiVisibility::Public, link("/example"));
            contribution.contribution.id = format!("item-{index}").parse().unwrap();
            entries.push(contribution);
        }
        let page = page(entries);
        let links: Vec<_> = page.global_extensions().into_iter().flat_map(|extension| extension.links).collect();
        let core = CoreFixture { links: &links };
        let plugin = PluginFixture { page: &page };
        assert_eq!(core.render().unwrap(), plugin.render().unwrap());
        let measure = |render: &mut dyn FnMut() -> String| {
            let mut samples = Vec::with_capacity(1000);
            for _ in 0..100 {
                black_box(render());
            }
            for _ in 0..1000 {
                let started = Instant::now();
                black_box(render());
                samples.push(started.elapsed().as_nanos());
            }
            samples.sort_unstable();
            (samples[500], samples[950])
        };
        let (core_p50, core_p95) = measure(&mut || core.render().unwrap());
        let (plugin_p50, plugin_p95) = measure(&mut || plugin.render().unwrap());
        println!(
            "16 links, 1000 warmed samples, identical HTML; core p50={core_p50}ns p95={core_p95}ns; plugin p50={plugin_p50}ns p95={plugin_p95}ns"
        );
    }
}

impl<'a> Context<'a> {
    fn repository(header: &'a RepoHeader) -> Self {
        // RepoHeader is constructed only after the route's repository access check.
        Self {
            owner: Some(&header.owner),
            repo: Some(&header.name),
            repository_reader: true,
            repository_admin: header.can_admin,
            enabled_plugins: &header.enabled_plugins,
            organization_admin: false,
        }
    }
}
