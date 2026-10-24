//! The small part of GitHub's expression language rbit evaluates itself: `${{ … }}` in
//! `runs-on` and job names (with `matrix.*`) and in `concurrency.group` (with a subset of
//! `github.*`). Everything else is passed to the runner as written.
//!
//! An expression is a literal (`'text'` with `''` for a quote, a number, `true`, `false`,
//! `null`), a context path (`matrix.os`, `github.event.pull_request.number`; names are
//! case-insensitive), `||`, `&&` and parentheses. `a || b` is the first truthy operand or
//! else the last, `a && b` the first falsy operand or else the last; falsy are `false`, `0`,
//! `-0`, `''`, `null` and `NaN`, as on GitHub. Anything else is an error, so a template
//! either evaluates exactly as GitHub would or is refused with a readable message.

use std::fmt;

/// Longest template, in bytes.
pub const MAX_TEMPLATE_BYTES: usize = 4096;
/// Deepest nesting of parentheses.
const MAX_NESTING: usize = 32;

/// A value an expression produces.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
}

impl ExprValue {
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::String(value) => !value.is_empty(),
        }
    }

    /// As GitHub interpolates it into a string.
    pub fn to_text(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Bool(value) => value.to_string(),
            Self::Number(value) => format_number(*value),
            Self::String(value) => value.clone(),
        }
    }
}

/// Numbers as GitHub prints them: integers without a fraction.
fn format_number(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity".to_owned() } else { "-Infinity".to_owned() }
    } else if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError(pub String);

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExprError {}

fn fail<T>(message: impl Into<String>) -> Result<T, ExprError> {
    Err(ExprError(message.into()))
}

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(ExprValue),
    /// Lower-cased path segments: `["matrix", "os"]`.
    Path(Vec<String>),
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Evaluates with `lookup`, which answers a path or `None` when rbit cannot evaluate it
    /// (an unknown context or property).
    pub fn evaluate(&self, lookup: &dyn Fn(&[String]) -> Option<ExprValue>) -> Result<ExprValue, ExprError> {
        match self {
            Self::Literal(value) => Ok(value.clone()),
            Self::Path(path) => lookup(path).map_or_else(|| fail(format!("`{}` cannot be used here", path.join("."))), Ok),
            Self::Or(left, right) => {
                let left = left.evaluate(lookup)?;
                if left.is_truthy() { Ok(left) } else { right.evaluate(lookup) }
            }
            Self::And(left, right) => {
                let left = left.evaluate(lookup)?;
                if left.is_truthy() { right.evaluate(lookup) } else { Ok(left) }
            }
        }
    }

    /// Every path the expression reads.
    pub fn paths(&self) -> Vec<&[String]> {
        match self {
            Self::Literal(_) => Vec::new(),
            Self::Path(path) => vec![path.as_slice()],
            Self::Or(left, right) | Self::And(left, right) => {
                let mut paths = left.paths();
                paths.extend(right.paths());
                paths
            }
        }
    }

    pub fn parse(text: &str) -> Result<Self, ExprError> {
        let tokens = tokenize(text)?;
        let mut parser = Parser { tokens: &tokens, position: 0, depth: 0 };
        let expr = parser.or()?;
        if parser.position != tokens.len() {
            return fail(format!("unexpected `{}` in `{}`", tokens[parser.position], text.trim()));
        }
        Ok(expr)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    String(String),
    Number(f64),
    Word(String),
    Dot,
    Or,
    And,
    Open,
    Close,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(text) => write!(f, "'{text}'"),
            Self::Number(value) => write!(f, "{}", format_number(*value)),
            Self::Word(word) => f.write_str(word),
            Self::Dot => f.write_str("."),
            Self::Or => f.write_str("||"),
            Self::And => f.write_str("&&"),
            Self::Open => f.write_str("("),
            Self::Close => f.write_str(")"),
        }
    }
}

fn tokenize(text: &str) -> Result<Vec<Token>, ExprError> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '\'' => {
                let mut value = String::new();
                loop {
                    match chars.next() {
                        Some((_, '\'')) if chars.peek().is_some_and(|&(_, next)| next == '\'') => {
                            chars.next();
                            value.push('\'');
                        }
                        Some((_, '\'')) => break,
                        Some((_, c)) => value.push(c),
                        None => return fail("a string is not closed with `'`"),
                    }
                }
                tokens.push(Token::String(value));
            }
            '|' | '&' => {
                if chars.next_if(|&(_, next)| next == c).is_none() {
                    return fail(format!("`{c}` must be doubled"));
                }
                tokens.push(if c == '|' { Token::Or } else { Token::And });
            }
            '(' => tokens.push(Token::Open),
            ')' => tokens.push(Token::Close),
            '.' => tokens.push(Token::Dot),
            c if c.is_ascii_digit() || c == '-' => {
                let mut end = start + c.len_utf8();
                while let Some(&(index, next)) = chars.peek() {
                    if !(next.is_ascii_alphanumeric() || next == '.' || next == '+' || next == '-') {
                        break;
                    }
                    end = index + next.len_utf8();
                    chars.next();
                }
                let literal = &text[start..end];
                let value = match literal.strip_prefix("0x") {
                    Some(hex) => u32::from_str_radix(hex, 16).ok().map(f64::from),
                    None => literal.parse::<f64>().ok(),
                };
                tokens.push(Token::Number(value.ok_or_else(|| ExprError(format!("`{literal}` is not a number")))?));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut end = start + 1;
                while let Some(&(index, next)) = chars.peek() {
                    if !(next.is_ascii_alphanumeric() || next == '_' || next == '-') {
                        break;
                    }
                    end = index + 1;
                    chars.next();
                }
                tokens.push(Token::Word(text[start..end].to_owned()));
            }
            other => return fail(format!("`{other}` is not supported in expressions rbit evaluates")),
        }
    }
    Ok(tokens)
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

    fn or(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.and()?;
        while self.peek() == Some(&Token::Or) {
            self.position += 1;
            left = Expr::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.primary()?;
        while self.peek() == Some(&Token::And) {
            self.position += 1;
            left = Expr::And(Box::new(left), Box::new(self.primary()?));
        }
        Ok(left)
    }

    fn primary(&mut self) -> Result<Expr, ExprError> {
        let Some(token) = self.tokens.get(self.position).cloned() else {
            return fail("an expression is incomplete");
        };
        self.position += 1;
        match token {
            Token::String(text) => Ok(Expr::Literal(ExprValue::String(text))),
            Token::Number(value) => Ok(Expr::Literal(ExprValue::Number(value))),
            Token::Open => {
                self.depth += 1;
                if self.depth > MAX_NESTING {
                    return fail("an expression is nested too deeply");
                }
                let inner = self.or()?;
                if self.peek() != Some(&Token::Close) {
                    return fail("a `(` is not closed");
                }
                self.position += 1;
                self.depth -= 1;
                Ok(inner)
            }
            Token::Word(word) => match word.as_str() {
                "true" => Ok(Expr::Literal(ExprValue::Bool(true))),
                "false" => Ok(Expr::Literal(ExprValue::Bool(false))),
                "null" => Ok(Expr::Literal(ExprValue::Null)),
                _ if self.peek() == Some(&Token::Open) => fail(format!("the function `{word}` is not supported here")),
                _ => {
                    let mut path = vec![word.to_ascii_lowercase()];
                    while self.peek() == Some(&Token::Dot) {
                        self.position += 1;
                        match self.tokens.get(self.position) {
                            Some(Token::Word(name)) => path.push(name.to_ascii_lowercase()),
                            _ => return fail(format!("`{}.` must be followed by a property name", path.join("."))),
                        }
                        self.position += 1;
                    }
                    Ok(Expr::Path(path))
                }
            },
            other => fail(format!("unexpected `{other}`")),
        }
    }
}

/// A string with `${{ … }}` expressions in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String),
    /// The expression and its source text (without `${{ }}`).
    Expr(Expr, String),
}

impl Template {
    pub fn parse(text: &str) -> Result<Self, ExprError> {
        if text.len() > MAX_TEMPLATE_BYTES {
            return fail(format!("an expression template must be at most {MAX_TEMPLATE_BYTES} bytes"));
        }
        let mut parts = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find("${{") {
            if start > 0 {
                parts.push(Part::Text(rest[..start].to_owned()));
            }
            let inner = &rest[start + 3..];
            let end = closing_braces(inner).ok_or_else(|| ExprError("a `${{` is not closed with `}}`".into()))?;
            let source = inner[..end].to_owned();
            parts.push(Part::Expr(Expr::parse(&source)?, source));
            rest = &inner[end + 2..];
        }
        if !rest.is_empty() {
            parts.push(Part::Text(rest.to_owned()));
        }
        Ok(Self { parts })
    }

    /// Whether the template has no expression.
    pub fn is_literal(&self) -> bool {
        self.parts.iter().all(|part| matches!(part, Part::Text(_)))
    }

    /// The single expression the template consists of, if it is exactly `${{ … }}`.
    pub fn as_single_expr(&self) -> Option<&Expr> {
        match self.parts.as_slice() {
            [Part::Expr(expr, _)] => Some(expr),
            _ => None,
        }
    }

    /// Evaluates every expression; fails if any reads something `lookup` does not know.
    pub fn render(&self, lookup: &dyn Fn(&[String]) -> Option<ExprValue>) -> Result<String, ExprError> {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Expr(expr, _) => out.push_str(&expr.evaluate(lookup)?.to_text()),
            }
        }
        Ok(out)
    }

    /// Evaluates the expressions `lookup` can answer and leaves the others as written, for
    /// the runner to evaluate.
    pub fn render_partially(&self, lookup: &dyn Fn(&[String]) -> Option<ExprValue>) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Expr(expr, source) => {
                    if let Ok(value) = expr.evaluate(lookup) {
                        out.push_str(&value.to_text());
                    } else {
                        out.push_str("${{");
                        out.push_str(source);
                        out.push_str("}}");
                    }
                }
            }
        }
        out
    }
}

/// The offset of the `}}` closing an expression, skipping quoted strings.
fn closing_braces(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quoted = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' => quoted = !quoted,
            b'}' if !quoted && bytes.get(index + 1) == Some(&b'}') => return Some(index),
            _ => {}
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(path: &[String]) -> Option<ExprValue> {
        let path: Vec<&str> = path.iter().map(String::as_str).collect();
        Some(match path.as_slice() {
            ["github", "ref"] => ExprValue::String("refs/heads/main".into()),
            ["github", "head_ref"] => ExprValue::String(String::new()),
            ["github", "event", "pull_request", "number"] => ExprValue::Null,
            ["github", "run_id"] => ExprValue::Number(42.0),
            ["matrix", "os"] => ExprValue::String("linux".into()),
            ["matrix", "zero"] => ExprValue::Number(0.0),
            ["matrix", "nan"] => ExprValue::Number(f64::NAN),
            _ => return None,
        })
    }

    fn render(template: &str) -> Result<String, ExprError> {
        Template::parse(template)?.render(&lookup)
    }

    #[test]
    fn rendering() {
        let cases = [
            ("plain", "plain"),
            ("", ""),
            ("${{ github.ref }}", "refs/heads/main"),
            ("ci-${{ github.ref }}-x", "ci-refs/heads/main-x"),
            ("${{github.REF}}", "refs/heads/main"),
            ("${{ github.head_ref || github.ref }}", "refs/heads/main"),
            ("${{ github.head_ref || github.run_id }}", "42"),
            ("${{ github.event.pull_request.number || github.ref }}", "refs/heads/main"),
            ("${{ github.ref && 'yes' }}", "yes"),
            ("${{ github.head_ref && 'yes' }}", ""),
            ("${{ matrix.zero || 'zero is falsy' }}", "zero is falsy"),
            ("${{ matrix.nan || 'NaN is falsy' }}", "NaN is falsy"),
            ("${{ null || false }}", "false"),
            ("${{ 'it''s' }}", "it's"),
            ("${{ '}}' }}", "}}"),
            ("${{ (null || 'a') && ('b' || 'c') }}", "b"),
            ("${{ 1.5 }} ${{ 2 }} ${{ -0 }} ${{ 0x10 }}", "1.5 2 -0 16"),
            ("${{ true }}${{ matrix.os }}", "truelinux"),
            ("a ${{ 'b' }} c ${{ 'd' }}", "a b c d"),
        ];
        for (template, expected) in cases {
            assert_eq!(render(template).as_deref(), Ok(expected), "{template}");
        }
    }

    #[test]
    fn refusals() {
        for template in [
            "${{ github.sha }}",
            "${{ secrets.TOKEN }}",
            "${{ format('{0}', github.ref) }}",
            "${{ github.ref == 'x' }}",
            "${{ !github.ref }}",
            "${{ github.ref | x }}",
            "${{ github. }}",
            "${{ 'open }}",
            "${{ github.ref",
            "${{ (github.ref }}",
            "${{ }}",
            "${{ github['ref'] }}",
            "${{ 1x }}",
        ] {
            assert!(render(template).is_err(), "{template}");
        }
        let deep = format!("${{{{ {}'x'{} }}}}", "(".repeat(40), ")".repeat(40));
        assert!(render(&deep).is_err());
        assert!(Template::parse(&"x".repeat(MAX_TEMPLATE_BYTES + 1)).is_err());
    }

    #[test]
    fn partial_rendering_keeps_what_rbit_cannot_evaluate() {
        let template = Template::parse("Build ${{ matrix.os }} on ${{ github.sha }} (${{ format('x') == 1 }})");
        assert!(template.is_err(), "unsupported syntax is still refused when parsing");
        let template = Template::parse("Build ${{ matrix.os }} for ${{ inputs.target }}").unwrap();
        assert_eq!(template.render_partially(&lookup), "Build linux for ${{ inputs.target }}");
        assert!(!template.is_literal());
        assert!(Template::parse("literal").unwrap().is_literal());
        let single = Template::parse("${{ matrix.os }}").unwrap();
        assert_eq!(single.as_single_expr().map(Expr::paths), Some(vec![&["matrix".to_owned(), "os".to_owned()][..]]));
        assert!(Template::parse("x ${{ matrix.os }}").unwrap().as_single_expr().is_none());
    }

    #[test]
    fn truthiness_and_text() {
        let falsy =
            [ExprValue::Null, ExprValue::Bool(false), ExprValue::Number(0.0), ExprValue::Number(-0.0), ExprValue::String(String::new())];
        assert!(falsy.iter().all(|value| !value.is_truthy()));
        let truthy = [ExprValue::Bool(true), ExprValue::Number(-1.0), ExprValue::String("0".into()), ExprValue::String("false".into())];
        assert!(truthy.iter().all(ExprValue::is_truthy));
        assert_eq!(ExprValue::Number(1e20).to_text(), "100000000000000000000");
        assert_eq!(ExprValue::Number(0.1).to_text(), "0.1");
        assert_eq!(ExprValue::Number(f64::INFINITY).to_text(), "Infinity");
    }
}
