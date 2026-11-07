use rbit_contracts::{AccountId, RepoId};
use rbit_wikis::{
    WikiContent, WikiId,
    commands::{WikiCommandId, WikiWriteCommand},
    policy::WikiExpected,
};

#[test]
fn fixed_command_identity_and_debug_do_not_expose_private_input() {
    assert!(WikiCommandId::new([0; 16]).is_none());
    let command = WikiWriteCommand {
        id: WikiCommandId::new([1; 16]).unwrap(),
        actor: AccountId(1),
        expected: WikiExpected::Create { repository: RepoId(2), wiki: WikiId::new(3).unwrap(), head: None },
        path: "private-sentinel".parse().unwrap(),
        title: "private-sentinel".parse().unwrap(),
        content: WikiContent::try_from("private-sentinel".to_owned()).unwrap(),
    };
    assert_eq!(command.wiki().get(), 3);
    assert_eq!(format!("{command:?}"), "WikiWriteCommand(..)");
}
