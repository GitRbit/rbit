//! Tokens → [`Expr`], and [`Expr`] → text.
//!
//! Precedence, loosest first, as GitHub documents it: `||`, `&&`, `==` `!=`, `<` `<=` `>`
//! `>=`, `!`, then property access (`.name`, `.*`, `[index]`) and calls. Binary operators
//! associate to the left. `&&` and `||` chains are one n-ary node and property chains one
//! [`Expr::Access`], so the tree only grows deep through parentheses, brackets, calls, `!`
//! and chained comparisons — exactly what [`MAX_NESTING`] counts.
//!
//! Printing ([`std::fmt::Display`]) writes the canonical text: single spaces around binary
//! operators and only the parentheses the tree needs. Parsing that text gives the same tree.

use std::fmt;

use super::{
    CONTEXTS, ExprError, MAX_NESTING, MAX_TEMPLATE_BYTES, fail,
    lex::{Token, tokenize},
};

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Null,
    Bool(bool),
    /// Always finite.
    Number(f64),
    String(String),
    /// A context, by its lower-cased name (one of [`CONTEXTS`]).
    Context(String),
    /// A base and at least one accessor: `github.event.pull_request`, `needs.*.result`,
    /// `fromJSON(x)[0]`.
    Access(Box<Expr>, Vec<Accessor>),
    /// A function and its arguments; the count was checked against the function.
    Call(Function, Vec<Expr>),
    Not(Box<Expr>),
    Compare(Comparison, Box<Expr>, Box<Expr>),
    /// At least two operands.
    And(Vec<Expr>),
    /// At least two operands.
    Or(Vec<Expr>),
}

/// One step of an [`Expr::Access`].
#[derive(Debug, Clone, PartialEq)]
pub enum Accessor {
    /// `.name`: a property, matched case-insensitively.
    Property(String),
    /// `[expr]`: an array index or a property name.
    Index(Expr),
    /// `.*`: every value of an object or item of an array.
    Filter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl Comparison {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        }
    }

    fn is_equality(self) -> bool {
        matches!(self, Self::Equal | Self::NotEqual)
    }
}

/// The functions an expression may call. Names are case-insensitive, as on GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Function {
    Contains,
    StartsWith,
    EndsWith,
    Format,
    Join,
    ToJson,
    FromJson,
    /// Only where the side evaluating supplies [`super::Contexts::hash_files`]: the runner.
    HashFiles,
    Success,
    Failure,
    Always,
    Cancelled,
}

impl Function {
    pub const ALL: [Self; 12] = [
        Self::Contains,
        Self::StartsWith,
        Self::EndsWith,
        Self::Format,
        Self::Join,
        Self::ToJson,
        Self::FromJson,
        Self::HashFiles,
        Self::Success,
        Self::Failure,
        Self::Always,
        Self::Cancelled,
    ];

    /// The function called `name`, ignoring case.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|function| function.name().eq_ignore_ascii_case(name))
    }

    /// The name as GitHub documents it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::StartsWith => "startsWith",
            Self::EndsWith => "endsWith",
            Self::Format => "format",
            Self::Join => "join",
            Self::ToJson => "toJSON",
            Self::FromJson => "fromJSON",
            Self::HashFiles => "hashFiles",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Always => "always",
            Self::Cancelled => "cancelled",
        }
    }

    /// How many arguments the function takes: at least, at most.
    pub const fn arity(self) -> (usize, usize) {
        match self {
            Self::Contains | Self::StartsWith | Self::EndsWith => (2, 2),
            Self::Format | Self::HashFiles => (1, usize::MAX),
            Self::Join => (1, 2),
            Self::ToJson | Self::FromJson => (1, 1),
            Self::Success | Self::Failure | Self::Always | Self::Cancelled => (0, 0),
        }
    }

    /// `success`, `failure`, `always` and `cancelled`, which read the job status.
    pub const fn is_status(self) -> bool {
        matches!(self, Self::Success | Self::Failure | Self::Always | Self::Cancelled)
    }
}

impl Expr {
    /// Parses one expression, written without `${{ }}`.
    pub fn parse(text: &str) -> Result<Self, ExprError> {
        if text.len() > MAX_TEMPLATE_BYTES {
            return fail(format!("an expression must be at most {MAX_TEMPLATE_BYTES} bytes"));
        }
        let tokens = tokenize(text)?;
        if tokens.is_empty() {
            return fail("an expression is empty");
        }
        let mut parser = Parser { tokens: &tokens, position: 0, depth: 0 };
        let expr = parser.or()?;
        if let Some(token) = parser.peek() {
            return fail(format!("unexpected `{token}` in `{}`", text.trim()));
        }
        Ok(expr)
    }

    /// Calls `visit` on this expression and everything inside it.
    pub fn walk<'a>(&'a self, visit: &mut dyn FnMut(&'a Self)) {
        visit(self);
        match self {
            Self::Null | Self::Bool(_) | Self::Number(_) | Self::String(_) | Self::Context(_) => {}
            Self::Access(base, accessors) => {
                base.walk(visit);
                for accessor in accessors {
                    if let Accessor::Index(index) = accessor {
                        index.walk(visit);
                    }
                }
            }
            Self::Call(_, items) | Self::And(items) | Self::Or(items) => items.iter().for_each(|item| item.walk(visit)),
            Self::Not(inner) => inner.walk(visit),
            Self::Compare(_, left, right) => {
                left.walk(visit);
                right.walk(visit);
            }
        }
    }

    /// The contexts the expression reads, lower-cased, without repeats.
    pub fn contexts(&self) -> Vec<&str> {
        let mut names = Vec::new();
        self.walk(&mut |expr| {
            if let Self::Context(name) = expr
                && !names.contains(&name.as_str())
            {
                names.push(name.as_str());
            }
        });
        names
    }

    /// The functions the expression calls, without repeats.
    pub fn functions(&self) -> Vec<Function> {
        let mut functions = Vec::new();
        self.walk(&mut |expr| {
            if let Self::Call(function, _) = expr
                && !functions.contains(function)
            {
                functions.push(*function);
            }
        });
        functions
    }

    /// Whether the expression calls `success`, `failure`, `always` or `cancelled`.
    pub fn uses_status(&self) -> bool {
        self.functions().iter().any(|function| function.is_status())
    }

    /// Binding strength for printing: higher binds tighter.
    fn precedence(&self) -> u8 {
        match self {
            Self::Or(_) => 1,
            Self::And(_) => 2,
            Self::Compare(op, ..) if op.is_equality() => 3,
            Self::Compare(..) => 4,
            Self::Not(_) => 5,
            _ => 6,
        }
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn eat(&mut self, token: &Token) -> bool {
        let found = self.peek() == Some(token);
        if found {
            self.position += 1;
        }
        found
    }

    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_NESTING { fail(format!("an expression may be nested at most {MAX_NESTING} levels deep")) } else { Ok(()) }
    }

    fn or(&mut self) -> Result<Expr, ExprError> {
        let first = self.and()?;
        if self.peek() != Some(&Token::Or) {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(&Token::Or) {
            items.push(self.and()?);
        }
        Ok(Expr::Or(items))
    }

    fn and(&mut self) -> Result<Expr, ExprError> {
        let first = self.equality()?;
        if self.peek() != Some(&Token::And) {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(&Token::And) {
            items.push(self.equality()?);
        }
        Ok(Expr::And(items))
    }

    fn equality(&mut self) -> Result<Expr, ExprError> {
        self.comparison_chain(&[(Token::Equal, Comparison::Equal), (Token::NotEqual, Comparison::NotEqual)], Self::relation)
    }

    fn relation(&mut self) -> Result<Expr, ExprError> {
        self.comparison_chain(
            &[
                (Token::Less, Comparison::Less),
                (Token::LessEqual, Comparison::LessEqual),
                (Token::Greater, Comparison::Greater),
                (Token::GreaterEqual, Comparison::GreaterEqual),
            ],
            Self::unary,
        )
    }

    /// `operand (op operand)*`, left-associative. Each operator nests the tree one level.
    fn comparison_chain(
        &mut self,
        operators: &[(Token, Comparison)],
        operand: fn(&mut Self) -> Result<Expr, ExprError>,
    ) -> Result<Expr, ExprError> {
        let depth = self.depth;
        let mut left = operand(self)?;
        while let Some(op) = self.peek().and_then(|token| operators.iter().find(|(candidate, _)| candidate == token)).map(|(_, op)| *op) {
            self.position += 1;
            self.enter()?;
            let right = operand(self)?;
            left = Expr::Compare(op, Box::new(left), Box::new(right));
        }
        self.depth = depth;
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        if self.eat(&Token::Not) {
            self.enter()?;
            let inner = self.unary()?;
            self.depth -= 1;
            return Ok(Expr::Not(Box::new(inner)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, ExprError> {
        let (base, accessible) = self.primary()?;
        let mut accessors = Vec::new();
        loop {
            match self.peek() {
                Some(Token::Dot) => {
                    self.position += 1;
                    let accessor = match self.peek() {
                        Some(Token::Star) => Accessor::Filter,
                        Some(token) if token.as_name().is_some() => Accessor::Property(token.as_name().unwrap_or_default().to_owned()),
                        _ => return fail(format!("`{base}.` must be followed by a property name or `*`")),
                    };
                    self.position += 1;
                    accessors.push(accessor);
                }
                Some(Token::OpenBracket) => {
                    self.position += 1;
                    self.enter()?;
                    let index = self.or()?;
                    if !self.eat(&Token::CloseBracket) {
                        return fail("a `[` is not closed with `]`");
                    }
                    self.depth -= 1;
                    accessors.push(Accessor::Index(index));
                }
                _ => break,
            }
        }
        if accessors.is_empty() {
            return Ok(base);
        }
        if !accessible {
            return fail(format!("`{base}` has no properties"));
        }
        Ok(Expr::Access(Box::new(base), accessors))
    }

    /// A primary expression, and whether properties may follow it (not after a literal).
    fn primary(&mut self) -> Result<(Expr, bool), ExprError> {
        let Some(token) = self.peek().cloned() else {
            return fail("an expression is incomplete");
        };
        self.position += 1;
        Ok(match token {
            Token::Null => (Expr::Null, false),
            Token::True => (Expr::Bool(true), false),
            Token::False => (Expr::Bool(false), false),
            Token::Number(value) => (Expr::Number(value), false),
            Token::String(text) => (Expr::String(text), false),
            Token::Open => {
                self.enter()?;
                let inner = self.or()?;
                if !self.eat(&Token::Close) {
                    return fail("a `(` is not closed with `)`");
                }
                self.depth -= 1;
                (inner, true)
            }
            Token::Name(name) if self.peek() == Some(&Token::Open) => {
                self.position += 1;
                let function = Function::from_name(&name).ok_or_else(|| ExprError(format!("`{name}` is not a function")))?;
                self.enter()?;
                let mut arguments = Vec::new();
                if !self.eat(&Token::Close) {
                    loop {
                        arguments.push(self.or()?);
                        if self.eat(&Token::Close) {
                            break;
                        }
                        if !self.eat(&Token::Comma) {
                            return fail(format!("the arguments of `{}` must be separated by `,` and closed with `)`", function.name()));
                        }
                    }
                }
                self.depth -= 1;
                let (least, most) = function.arity();
                if arguments.len() < least || arguments.len() > most {
                    let expected = match (least, most) {
                        (least, most) if least == most => format!("{least}"),
                        (least, usize::MAX) => format!("at least {least}"),
                        (least, most) => format!("{least} or {most}"),
                    };
                    return fail(format!("`{}` takes {expected} arguments, not {}", function.name(), arguments.len()));
                }
                (Expr::Call(function, arguments), true)
            }
            Token::Name(name) => {
                let lower = name.to_ascii_lowercase();
                if !CONTEXTS.contains(&lower.as_str()) {
                    return fail(format!("`{name}` is not a context; the contexts are {}", CONTEXTS.join(", ")));
                }
                (Expr::Context(lower), true)
            }
            other => return fail(format!("unexpected `{other}`")),
        })
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// `expr`, in parentheses when it binds looser than `minimum`.
        fn operand(f: &mut fmt::Formatter<'_>, expr: &Expr, minimum: u8) -> fmt::Result {
            if expr.precedence() < minimum { write!(f, "({expr})") } else { write!(f, "{expr}") }
        }
        fn list(f: &mut fmt::Formatter<'_>, items: &[Expr], separator: &str, minimum: u8) -> fmt::Result {
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    f.write_str(separator)?;
                }
                operand(f, item, minimum)?;
            }
            Ok(())
        }
        match self {
            Self::Null => f.write_str("null"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Number(value) => write!(f, "{value}"),
            Self::String(text) => write!(f, "'{}'", text.replace('\'', "''")),
            Self::Context(name) => f.write_str(name),
            Self::Access(base, accessors) => {
                if matches!(**base, Self::Context(_) | Self::Call(..)) {
                    write!(f, "{base}")?;
                } else {
                    write!(f, "({base})")?;
                }
                for accessor in accessors {
                    match accessor {
                        Accessor::Property(name) => write!(f, ".{name}")?,
                        Accessor::Index(index) => write!(f, "[{index}]")?,
                        Accessor::Filter => f.write_str(".*")?,
                    }
                }
                Ok(())
            }
            Self::Call(function, arguments) => {
                write!(f, "{}(", function.name())?;
                list(f, arguments, ", ", 0)?;
                f.write_str(")")
            }
            Self::Not(inner) => {
                f.write_str("!")?;
                operand(f, inner, 5)
            }
            Self::Compare(op, left, right) => {
                let own = self.precedence();
                operand(f, left, own)?;
                write!(f, " {} ", op.as_str())?;
                operand(f, right, own + 1)
            }
            // A nested chain of the same kind needs parentheses to stay a separate node.
            Self::And(items) => list(f, items, " && ", 3),
            Self::Or(items) => list(f, items, " || ", 2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printed(text: &str) -> String {
        Expr::parse(text).unwrap_or_else(|error| panic!("{text}: {error}")).to_string()
    }

    #[test]
    fn precedence_and_printing() {
        let cases = [
            ("env || vars && job", "env || vars && job"),
            ("(env || vars) && job", "(env || vars) && job"),
            ("env || (vars || job)", "env || (vars || job)"),
            ("(env || vars) || job", "(env || vars) || job"),
            ("!env == vars", "!env == vars"),
            ("!(env == vars)", "!(env == vars)"),
            ("env < vars == job > steps", "env < vars == job > steps"),
            ("env == (vars == job)", "env == (vars == job)"),
            ("(env == vars) == job", "env == vars == job"),
            ("github.event['pull_request'].head.*[0]", "github.event['pull_request'].head.*[0]"),
            ("GitHub.Ref", "github.Ref"),
            ("(github).ref", "github.ref"),
            ("('a').b", "('a').b"),
            ("(env.b).c", "(env.b).c"),
            ("STARTSWITH(env, 'x')", "startsWith(env, 'x')"),
            ("fromJSON('[1]')[0]", "fromJSON('[1]')[0]"),
            ("format('{0}''s', 1.50, -2, 0x10)", "format('{0}''s', 1.5, -2, 16)"),
            ("inputs.null || steps.true", "inputs.null || steps.true"),
            ("success()", "success()"),
        ];
        for (text, expected) in cases {
            let printed = printed(text);
            assert_eq!(printed, expected, "{text}");
            assert_eq!(Expr::parse(&printed).unwrap(), Expr::parse(text).unwrap(), "{text}");
        }
    }

    #[test]
    fn trees() {
        let tree = Expr::parse("env || vars || job && steps").unwrap();
        assert!(matches!(&tree, Expr::Or(items) if items.len() == 3 && matches!(items[2], Expr::And(_))));
        let tree = Expr::parse("needs.*.result").unwrap();
        assert_eq!(
            tree,
            Expr::Access(Box::new(Expr::Context("needs".into())), vec![Accessor::Filter, Accessor::Property("result".into())])
        );
        let tree = Expr::parse("success() && contains(needs.*.result, 'failure') || always()").unwrap();
        assert_eq!(tree.contexts(), ["needs"]);
        assert_eq!(tree.functions(), [Function::Success, Function::Contains, Function::Always]);
        assert!(tree.uses_status() && !Expr::parse("env == 1").unwrap().uses_status());
    }

    #[test]
    fn refusals() {
        let cases = [
            ("", "empty"),
            ("env.", "property name"),
            ("env.1", "property name"),
            ("env[0", "`[` is not closed"),
            ("(env", "`(` is not closed"),
            ("env vars", "unexpected `vars`"),
            ("env ==", "incomplete"),
            ("unknown.x", "not a context"),
            ("nope(1)", "not a function"),
            ("contains(env)", "takes 2 arguments"),
            ("join()", "1 or 2"),
            ("format()", "at least 1"),
            ("success(1)", "takes 0"),
            ("contains(env vars)", "separated by `,`"),
            ("'a'.b", "no properties"),
            ("1[0]", "no properties"),
            (")", "unexpected `)`"),
        ];
        for (text, message) in cases {
            let error = Expr::parse(text).unwrap_err();
            assert!(error.0.contains(message), "{text}: {error}");
        }
    }

    #[test]
    fn nesting_is_bounded() {
        let at = |depth: usize, open: &str, close: &str| format!("{}env{}", open.repeat(depth), close.repeat(depth));
        assert!(Expr::parse(&at(MAX_NESTING, "(", ")")).is_ok());
        assert!(Expr::parse(&at(MAX_NESTING + 1, "(", ")")).is_err());
        assert!(Expr::parse(&at(MAX_NESTING, "!", "")).is_ok());
        assert!(Expr::parse(&at(MAX_NESTING + 1, "!", "")).is_err());
        assert!(Expr::parse(&at(MAX_NESTING + 1, "env[", "]")).is_err());
        assert!(Expr::parse(&at(MAX_NESTING + 1, "toJSON(", ")")).is_err());
        assert!(Expr::parse(&vec!["env"; MAX_NESTING + 1].join(" == ")).is_ok());
        assert!(Expr::parse(&vec!["env"; MAX_NESTING + 2].join(" == ")).is_err(), "each chained comparison nests");
        assert!(Expr::parse(&vec!["env"; 500].join(" || ")).is_ok(), "a long || chain is flat");
        assert!(Expr::parse(&format!("github{}", ".a".repeat(500))).is_ok(), "a long path is flat");
        assert!(Expr::parse(&"env || ".repeat(1000)).unwrap_err().0.contains("at most"), "too long");
    }
}
