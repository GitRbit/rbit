//! Tokens to bounded expression trees; original precedence and diagnostics.
use super::lex::{Token, tokenize};
use super::syntax::{Accessor, Comparison, Expr, Function};
use super::{CONTEXTS, ExprError, MAX_NESTING, MAX_TEMPLATE_BYTES, fail};

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
    pub(super) fn precedence(&self) -> u8 {
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

#[cfg(test)]
mod tests;
