//! Original parser never panics fixtures; assertions and source samples retained.

use proptest as prop;
use proptest::{
    prelude::{Just, ProptestConfig, any},
    prop_oneof, proptest,
    strategy::Strategy,
};

use super::super::parse;
use crate::limits::{Deadline, Limits};

fn yamlish() -> impl Strategy<Value = String> {
    let token = prop_oneof![
        Just("on".to_owned()),
        Just("jobs".to_owned()),
        Just("runs-on".to_owned()),
        Just("needs".to_owned()),
        Just("strategy".to_owned()),
        Just("matrix".to_owned()),
        Just("include".to_owned()),
        Just("push".to_owned()),
        Just("branches".to_owned()),
        Just("&a".to_owned()),
        Just("*a".to_owned()),
        Just("<<".to_owned()),
        Just("${{ matrix.x }}".to_owned()),
        "[a-z0-9]{1,6}",
    ];
    let piece = prop_oneof![
        token,
        Just(": ".to_owned()),
        Just("\n".to_owned()),
        Just("\n  ".to_owned()),
        Just("\n    ".to_owned()),
        Just("- ".to_owned()),
        Just("[".to_owned()),
        Just("]".to_owned()),
        Just("{".to_owned()),
        Just("}".to_owned()),
        Just(", ".to_owned()),
        Just("'".to_owned()),
        Just("\"".to_owned()),
        Just("|\n".to_owned()),
        Just("#".to_owned()),
    ];
    prop::collection::vec(piece, 0..64).prop_map(|pieces| pieces.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn on_arbitrary_bytes(source in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = parse(&source, &Limits::default(), &Deadline::NONE);
    }

    #[test]
    fn on_yaml_like_text(source in yamlish()) {
        let _ = parse(source.as_bytes(), &Limits::default(), &Deadline::NONE);
    }
}
