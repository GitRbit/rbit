//! Expression syntax, function metadata and canonical printing.
use std::fmt;

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Null,
    Bool(bool),
    /// Always finite.
    Number(f64),
    String(String),
    /// A context, by its lower-cased name (one of [`super::CONTEXTS`]).
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

    pub(super) fn is_equality(self) -> bool {
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
