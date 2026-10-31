use rbit_i18n::{LanguagePack, PackSet};
use rbit_messages::{Argument, English, MessageRef, MessageSchema, Value};
use std::sync::Arc;

#[test]
fn bundled_packs_validate_and_resolve_locales_plurals_and_pseudolocales() {
    let schema = MessageSchema::builtin();
    let packs: Vec<_> = [
        include_bytes!("../../../language-packs/es.rbit-language").as_slice(),
        include_bytes!("../../../language-packs/en-XA.rbit-language").as_slice(),
        include_bytes!("../../../language-packs/ar-XB.rbit-language").as_slice(),
    ]
    .into_iter()
    .map(|bytes| Arc::new(LanguagePack::parse(bytes, &schema).unwrap()))
    .collect();
    let set = PackSet::new(packs, Arc::clone(&schema), Arc::new(English::new(Arc::clone(&schema))), 9).unwrap();
    let spanish = set.snapshot("es");
    assert_eq!(spanish.text("core", "nav-explore"), "Explorar");
    for (count, expected) in [(1.0, "Un paquete de idioma instalado"), (3.0, "paquetes de idioma instalados")] {
        assert!(
            spanish
                .format(&MessageRef {
                    namespace: "core",
                    key: "language-pack-count",
                    args: &[Argument { name: "count", value: Value::Number(count) }]
                })
                .contains(expected)
        );
    }
    for (locale, direction) in [("en-XA", "ltr"), ("ar-XB", "rtl")] {
        let snapshot = set.negotiate(Some(locale), None, Some("es"), "en");
        assert_eq!(snapshot.lang(), locale);
        assert_eq!(snapshot.direction(), direction);
        assert_eq!(snapshot.revision(), 9);
        for message in schema.messages().filter(|message| message.args.is_empty()) {
            let output = snapshot.text(&message.namespace, &message.key);
            assert!(!output.is_empty(), "missing {}.{}", message.namespace, message.key);
            assert_ne!(output, message.english, "untranslated {}.{}", message.namespace, message.key);
        }
    }
}
