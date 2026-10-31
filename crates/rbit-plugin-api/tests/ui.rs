use rbit_plugin_api::{
    MAX_UI_CONTRIBUTIONS, PluginId, UiContent, UiContribution, UiContributionSet, UiIcon, UiLink, UiMessage, UiNoticeTone, UiSlot,
    UiTarget, UiValidationError, UiVisibility,
};

fn owner() -> PluginId {
    "example.plugin".parse().unwrap()
}
fn message() -> UiMessage {
    UiMessage { namespace: "example.plugin".into(), key: "extension-label".into(), fallback: "A safe <text> label".into() }
}
fn link(target: &str) -> UiLink {
    UiLink { label: message(), target: target.parse().unwrap(), icon: UiIcon::Box }
}
fn contribution() -> UiContribution {
    UiContribution {
        id: "example.navigation".parse().unwrap(),
        slot: UiSlot::GlobalNav,
        order: 0,
        visibility: UiVisibility::SignedIn,
        content: UiContent::Link(link("/plugins/example")),
    }
}

#[test]
fn valid_ui_contract_roundtrips_without_markup_or_code_fields() {
    let set = UiContributionSet::new(vec![contribution()]);
    set.validate(&owner()).unwrap();
    let json = serde_json::to_string(&set).unwrap();
    let decoded: UiContributionSet = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, set);
    for field in ["html", "script", "class", "on_click", "asset_url"] {
        let mut value = serde_json::to_value(&set).unwrap();
        value["contributions"][0][field] = serde_json::json!("bad");
        assert!(serde_json::from_value::<UiContributionSet>(value).is_err(), "{field}");
    }
}

#[test]
fn targets_reject_external_encoded_or_ambiguous_paths_and_placeholders() {
    for target in [
        "https://evil.test",
        "//evil.test",
        "/a/../b",
        "/./a",
        "/%2e%2e/",
        "/a\\b",
        "/a?x=1",
        "/a#x",
        "/a//b",
        "/{anything}/x",
        "/owner-{owner}/x",
        "/{owner}/{owner}",
        "/x\nheader",
        "/javascript:alert",
    ] {
        assert!(target.parse::<UiTarget>().is_err(), "{target}");
    }
    let target: UiTarget = "/{owner}/{repo}/actions".parse().unwrap();
    assert_eq!(target.render(Some("José"), Some("repo.name")).unwrap(), "/Jos%C3%A9/repo.name/actions");
    for parameter in ["", ".", "..", "%2e%2e", "a/b", "a\\b", "a?b", "a#b", "a\0b"] {
        assert!(target.render(Some(parameter), Some("repo")).is_err(), "{parameter}");
    }
    assert!(target.render(None, Some("repo")).is_err());
    assert!(target.render(Some("owner"), None).is_err());
    assert_eq!("/".parse::<UiTarget>().unwrap().render(None, None).unwrap(), "/");
}

#[test]
fn ownership_slot_context_and_visibility_are_validated() {
    let mut item = contribution();
    if let UiContent::Link(link) = &mut item.content {
        link.label.namespace = "core".into();
    }
    assert!(matches!(UiContributionSet::new(vec![item]).validate(&owner()), Err(UiValidationError::Message)));
    let mut item = contribution();
    item.slot = UiSlot::AdminNav;
    assert!(matches!(UiContributionSet::new(vec![item]).validate(&owner()), Err(UiValidationError::Slot)));
    let mut item = contribution();
    item.slot = UiSlot::RepositorySettings;
    item.visibility = UiVisibility::RepositoryReader;
    item.content = UiContent::Link(link("/{owner}/{repo}/settings/example"));
    assert!(matches!(UiContributionSet::new(vec![item.clone()]).validate(&owner()), Err(UiValidationError::Slot)));
    item.visibility = UiVisibility::RepositoryAdmin;
    UiContributionSet::new(vec![item]).validate(&owner()).unwrap();
    let mut item = contribution();
    item.content = UiContent::Link(link("/{owner}/example"));
    assert!(UiContributionSet::new(vec![item]).validate(&owner()).is_err());
    let mut item = contribution();
    item.slot = UiSlot::OrganizationSettings;
    item.visibility = UiVisibility::OrganizationAdmin;
    item.content = UiContent::Card { title: message(), body: message(), action: Some(link("/{owner}/settings/example")) };
    UiContributionSet::new(vec![item]).validate(&owner()).unwrap();
}

#[test]
fn declaration_counts_duplicates_and_component_resources_are_bounded() {
    let mut set = UiContributionSet::new(vec![contribution()]);
    set.schema_version = 2;
    assert!(matches!(set.validate(&owner()), Err(UiValidationError::Schema)));
    let set = UiContributionSet::new(vec![contribution(); 2]);
    assert!(matches!(set.validate(&owner()), Err(UiValidationError::Duplicate)));
    let set = UiContributionSet::new(vec![contribution(); MAX_UI_CONTRIBUTIONS + 1]);
    assert!(matches!(set.validate(&owner()), Err(UiValidationError::Limit)));
    let mut item = contribution();
    item.slot = UiSlot::AdminPanel;
    item.visibility = UiVisibility::InstanceAdmin;
    item.content = UiContent::List { title: message(), items: vec![link("/plugins/example"); 9] };
    assert!(matches!(UiContributionSet::new(vec![item]).validate(&owner()), Err(UiValidationError::Limit)));
    let mut item = contribution();
    item.content = UiContent::Notice { message: message(), tone: UiNoticeTone::Info };
    assert!(matches!(UiContributionSet::new(vec![item]).validate(&owner()), Err(UiValidationError::Slot)));
    let mut item = contribution();
    if let UiContent::Link(link) = &mut item.content {
        link.label.fallback = "x".repeat(1025);
    }
    assert!(UiContributionSet::new(vec![item]).validate(&owner()).is_err());
}
