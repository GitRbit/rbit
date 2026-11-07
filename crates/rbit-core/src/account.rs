//! Compatibility paths for identity-owned account entities and shared account IDs.

pub use rbit_identity_models::{Account, AccountId, AccountKind, User};

#[cfg(test)]
mod identity_compatibility {
    use std::any::TypeId;

    #[test]
    fn legacy_account_paths_remain_the_same_contract_type() {
        let module_path: super::AccountId = rbit_contracts::AccountId(-1);
        let root_path: crate::AccountId = super::AccountId(module_path.0);
        let contract: rbit_contracts::AccountId = root_path;
        assert_eq!(contract.0, -1);
        assert_eq!(TypeId::of::<crate::AccountId>(), TypeId::of::<rbit_contracts::AccountId>());
        assert_eq!(serde_json::to_string(&root_path).unwrap(), "-1");
    }
}

#[cfg(test)]
mod model_compatibility {
    use std::any::TypeId;

    #[test]
    fn legacy_account_and_name_paths_reexport_the_exact_identity_types() {
        assert_eq!(TypeId::of::<super::Account>(), TypeId::of::<rbit_identity_models::Account>());
        assert_eq!(TypeId::of::<crate::Account>(), TypeId::of::<rbit_identity_models::Account>());
        assert_eq!(TypeId::of::<crate::User>(), TypeId::of::<rbit_identity_models::User>());
        assert_eq!(TypeId::of::<super::AccountKind>(), TypeId::of::<rbit_identity_models::AccountKind>());
        assert_eq!(TypeId::of::<crate::names::AccountName>(), TypeId::of::<rbit_identity_models::AccountName>());
        assert_eq!(TypeId::of::<crate::AccountName>(), TypeId::of::<rbit_identity_models::AccountName>());
        assert_eq!(TypeId::of::<crate::names::Email>(), TypeId::of::<rbit_identity_models::Email>());
        assert_eq!(TypeId::of::<crate::Email>(), TypeId::of::<rbit_identity_models::Email>());
        let account: crate::names::AccountName = rbit_identity_models::AccountName::try_from("Alice".to_owned()).unwrap();
        assert_eq!(account.to_string(), "Alice");
        let email: crate::names::Email = rbit_identity_models::Email::try_from(" alice@example.com ".to_owned()).unwrap();
        assert_eq!(email.as_str(), "alice@example.com");
    }
}
