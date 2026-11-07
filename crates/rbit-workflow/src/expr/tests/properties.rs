//! Original bounded expression generators and property cases.

use proptest as prop;
use proptest::{
    prelude::{Just, ProptestConfig, any},
    prop_assert, prop_assert_eq, prop_assume, prop_oneof, proptest,
    strategy::Strategy,
    test_runner::TestCaseError,
};

use super::super::{
    Accessor, CONTEXTS, Comparison, Condition, ContextMap, Expr, Function, MAX_STEPS, MAX_TEMPLATE_BYTES, Status, Template, eval,
};
use serde_json::json;

fn leaf() -> impl Strategy<Value = Expr> {
    prop_oneof![
        Just(Expr::Null),
        any::<bool>().prop_map(Expr::Bool),
        (-1e6..1e6f64).prop_map(Expr::Number),
        any::<i32>().prop_map(|value| Expr::Number(f64::from(value))),
        ".{0,8}".prop_map(Expr::String),
        prop::sample::select(&CONTEXTS[..]).prop_map(|name| Expr::Context(name.to_owned())),
    ]
}

fn expr() -> impl Strategy<Value = Expr> {
    leaf().prop_recursive(5, 64, 4, |inner| {
        let accessor = prop_oneof![
            "[a-zA-Z_][a-zA-Z0-9_-]{0,6}".prop_map(Accessor::Property),
            inner.clone().prop_map(Accessor::Index),
            Just(Accessor::Filter),
        ];
        let comparison = prop::sample::select(vec![
            Comparison::Equal,
            Comparison::NotEqual,
            Comparison::Less,
            Comparison::LessEqual,
            Comparison::Greater,
            Comparison::GreaterEqual,
        ]);
        let call =
            (prop::sample::select(&Function::ALL[..]), prop::collection::vec(inner.clone(), 0..4)).prop_map(|(function, mut arguments)| {
                let (least, most) = function.arity();
                while arguments.len() < least {
                    arguments.push(Expr::String("x".into()));
                }
                arguments.truncate(most);
                Expr::Call(function, arguments)
            });
        prop_oneof![
            (inner.clone(), prop::collection::vec(accessor, 1..4)).prop_map(|(base, accessors)| Expr::Access(Box::new(base), accessors)),
            inner.clone().prop_map(|inner| Expr::Not(Box::new(inner))),
            (comparison, inner.clone(), inner.clone()).prop_map(|(op, left, right)| Expr::Compare(op, Box::new(left), Box::new(right))),
            prop::collection::vec(inner.clone(), 2..4).prop_map(Expr::And),
            prop::collection::vec(inner.clone(), 2..4).prop_map(Expr::Or),
            call,
        ]
    })
}

fn expression_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        prop::sample::select(&CONTEXTS[..]).prop_map(str::to_owned),
        prop::sample::select(&Function::ALL[..]).prop_map(|function| function.name().to_owned()),
        prop::sample::select(vec![
            ".", ".*", "[", "]", "(", ")", ",", "!", "<", "<=", ">", ">=", "==", "!=", "&&", "||", "'", "''", "null", "true", "-", "0x",
            "1e", " ", "${{", "}}", "{0}", "a", "_b-1",
        ])
        .prop_map(str::to_owned),
        "[0-9]{1,3}(\\.[0-9]{1,2})?",
        "'[^']{0,4}'",
        any::<char>().prop_map(String::from),
    ];
    prop::collection::vec(piece, 0..40).prop_map(|pieces| pieces.concat())
}

/// Every context set, with data deep and wide enough for filters to cost.
static RICH_CONTEXTS: std::sync::LazyLock<ContextMap> = std::sync::LazyLock::new(|| {
    let wide = json!(vec![json!({"a": [1, 2, 3], "b": "x", "c": {"d": [true, null]}}); 300]);
    let mut contexts = ContextMap::new();
    for name in CONTEXTS {
        contexts.insert(name, json!({"a": wide.clone(), "b": [wide.clone(), "text"], "n": 1.5, "s": "S"}));
    }
    contexts.with_status(Status::Failure)
});

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn parsing_never_panics(text in expression_text()) {
        let _ = Expr::parse(&text);
        let _ = Template::parse(&text);
        let _ = Condition::parse(&text);
    }

    #[test]
    fn parsing_arbitrary_strings_never_panics(text in ".{0,64}") {
        let _ = Expr::parse(&text);
        let _ = Template::parse(&text);
    }

    #[test]
    fn printing_round_trips_trees(expr in expr()) {
        let printed = expr.to_string();
        prop_assume!(printed.len() <= MAX_TEMPLATE_BYTES);
        let parsed = Expr::parse(&printed).map_err(|error| TestCaseError::fail(format!("{printed}: {error}")))?;
        prop_assert_eq!(&parsed, &expr, "{}", printed);
    }

    #[test]
    fn printing_round_trips_text(text in expression_text()) {
        if let Ok(parsed) = Expr::parse(&text) {
            let printed = parsed.to_string();
            prop_assert_eq!(Expr::parse(&printed).ok(), Some(parsed), "{} printed as {}", text, printed);
        }
    }

    #[test]
    fn evaluation_ends_within_the_budget(expr in expr()) {
        let contexts = &*RICH_CONTEXTS;
        let mut evaluator = eval::Evaluator::new(contexts);
        let _ = evaluator.eval(&expr);
        prop_assert!(evaluator.steps <= MAX_STEPS);
    }
}
