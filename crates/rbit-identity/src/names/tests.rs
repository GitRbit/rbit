use super::{AccountName, Email};
use rbit_contracts::NameError;

#[test]
fn account_name_constructors_keep_case_byte_limits_and_owned_allocation() {
    let source = String::from("Alice.Example-1");
    let allocation = source.as_ptr();
    let name = AccountName::try_from(source).unwrap();
    assert_eq!(name.as_str().as_ptr(), allocation, "owned construction must retain its existing allocation");
    assert_eq!(name.as_str(), "Alice.Example-1");
    assert_eq!(AsRef::<str>::as_ref(&name), name.as_str());
    assert_eq!(name.to_string(), "Alice.Example-1");
    assert_eq!(String::from(name), "Alice.Example-1");
    assert_eq!(AccountName::MAX_LEN, 39);
    assert!("a".repeat(39).parse::<AccountName>().is_ok());
    assert_eq!("a".repeat(40).parse::<AccountName>(), Err(NameError::TooLong { kind: "username", max: 39 }));
    assert_eq!(" Alice ".parse::<AccountName>(), Err(NameError::InvalidCharacter { kind: "username" }));
}

#[test]
fn account_names_preserve_reserved_routes_traversal_and_error_shapes() {
    for reserved in
        ["Admin", "api", "Email", "Explore", "Healthz", "Issues", "Notifications", "Organizations", "Pulls", "Readyz", "Settings", "Sw.js"]
    {
        assert_eq!(reserved.parse::<AccountName>(), Err(NameError::Reserved { kind: "username", value: reserved.into() }));
    }
    for input in ["a/b", "a\\b", "a\0b", "雪"] {
        assert_eq!(input.parse::<AccountName>(), Err(NameError::InvalidCharacter { kind: "username" }));
    }
    for input in ["x..y", "x.GIT", "x.LoCk"] {
        assert_eq!(input.parse::<AccountName>(), Err(NameError::Reserved { kind: "username", value: input.into() }));
    }
    assert_eq!("".parse::<AccountName>(), Err(NameError::Empty { kind: "username" }));
    assert_eq!(".alice".parse::<AccountName>(), Err(NameError::InvalidBoundary { kind: "username" }));
}

#[test]
fn names_keep_their_exact_string_serde_and_postcard_representations() {
    let name: AccountName = "Alice.Example".parse().unwrap();
    assert_eq!(serde_json::to_string(&name).unwrap(), "\"Alice.Example\"");
    assert_eq!(serde_json::from_str::<AccountName>("\"Alice.Example\"").unwrap(), name);
    assert!(serde_json::from_str::<AccountName>("\"Settings\"").is_err());
    assert_eq!(postcard::to_allocvec(&name).unwrap(), postcard::to_allocvec("Alice.Example").unwrap());
    assert_eq!(postcard::from_bytes::<AccountName>(&postcard::to_allocvec(&name).unwrap()).unwrap(), name);
    let email: Email = " Alice@Example.com ".parse().unwrap();
    assert_eq!(serde_json::to_string(&email).unwrap(), "\"Alice@Example.com\"");
    assert_eq!(serde_json::from_str::<Email>("\" Alice@Example.com \"").unwrap(), email);
    assert_eq!(postcard::to_allocvec(&email).unwrap(), postcard::to_allocvec("Alice@Example.com").unwrap());
    assert!(serde_json::from_str::<Email>("\"alice@localhost\"").is_err());
}

#[test]
fn email_preserves_existing_plausibility_and_trimming_without_tightening_rules() {
    assert_eq!(Email::MAX_LEN, 254);
    for input in ["Alice@Example.com", "alice@@example.com", "雪@example.com", "alice@example..com", "\0@example.com"] {
        let parsed: Email = input.parse().unwrap();
        assert_eq!(parsed.as_str(), input);
        assert_eq!(parsed.to_string(), input);
        assert_eq!(String::from(parsed), input);
    }
    let trimmed = Email::try_from(" \talice@example.com\r\n".to_owned()).unwrap();
    assert_eq!(trimmed.as_str(), "alice@example.com");
    for input in ["", "alice@localhost", "@example.com", "alice@.example.com", "alice@example.", "a b@example.com"] {
        assert_eq!(input.parse::<Email>(), Err(NameError::InvalidEmail));
    }
    let at_limit = format!("{}@b.co", "a".repeat(249));
    assert_eq!(at_limit.len(), 254);
    assert!(at_limit.parse::<Email>().is_ok());
    assert_eq!(format!("a{at_limit}").parse::<Email>(), Err(NameError::InvalidEmail));
}
