//! Expression text → tokens.
//!
//! Literals are `null`, `true`, `false`, numbers (`711`, `-9.2`, `-9.2e-2`, `0xff`) and
//! `'strings'` with `''` for a quote. Names are `[A-Za-z_][A-Za-z0-9_-]*`, so step ids such
//! as `build-1` work in `steps.build-1.outputs`. Whitespace, including newlines, separates
//! tokens and is otherwise ignored.

use std::fmt;

use super::{ExprError, fail};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Token {
    Null,
    True,
    False,
    Number(f64),
    String(String),
    Name(String),
    Dot,
    Star,
    Comma,
    Open,
    Close,
    OpenBracket,
    CloseBracket,
    Not,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    And,
    Or,
}

impl Token {
    /// The token as a property name after `.`: keywords are names there (`inputs.null`).
    pub(super) fn as_name(&self) -> Option<&str> {
        match self {
            Self::Name(name) => Some(name),
            Self::Null => Some("null"),
            Self::True => Some("true"),
            Self::False => Some("false"),
            _ => None,
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Null => "null",
            Self::True => "true",
            Self::False => "false",
            Self::Number(value) => return f.write_str(&super::format_number(*value)),
            Self::String(text) => return write!(f, "'{}'", text.replace('\'', "''")),
            Self::Name(name) => name,
            Self::Dot => ".",
            Self::Star => "*",
            Self::Comma => ",",
            Self::Open => "(",
            Self::Close => ")",
            Self::OpenBracket => "[",
            Self::CloseBracket => "]",
            Self::Not => "!",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::And => "&&",
            Self::Or => "||",
        };
        f.write_str(text)
    }
}

pub(super) fn tokenize(text: &str) -> Result<Vec<Token>, ExprError> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let token = match c {
            c if c.is_whitespace() => continue,
            '\'' => {
                let mut value = String::new();
                loop {
                    match chars.next() {
                        Some((_, '\'')) if chars.next_if(|&(_, next)| next == '\'').is_some() => value.push('\''),
                        Some((_, '\'')) => break,
                        Some((_, c)) => value.push(c),
                        None => return fail("a string is not closed with `'`"),
                    }
                }
                Token::String(value)
            }
            '|' | '&' => {
                if chars.next_if(|&(_, next)| next == c).is_none() {
                    return fail(format!("`{c}` must be doubled"));
                }
                if c == '|' { Token::Or } else { Token::And }
            }
            '=' => {
                if chars.next_if(|&(_, next)| next == '=').is_none() {
                    return fail("`=` must be doubled: equality is `==`");
                }
                Token::Equal
            }
            '!' => {
                if chars.next_if(|&(_, next)| next == '=').is_some() {
                    Token::NotEqual
                } else {
                    Token::Not
                }
            }
            '<' => {
                if chars.next_if(|&(_, next)| next == '=').is_some() {
                    Token::LessEqual
                } else {
                    Token::Less
                }
            }
            '>' => {
                if chars.next_if(|&(_, next)| next == '=').is_some() {
                    Token::GreaterEqual
                } else {
                    Token::Greater
                }
            }
            '(' => Token::Open,
            ')' => Token::Close,
            '[' => Token::OpenBracket,
            ']' => Token::CloseBracket,
            '.' => Token::Dot,
            '*' => Token::Star,
            ',' => Token::Comma,
            c if c.is_ascii_digit() || (c == '-' && chars.peek().is_some_and(|&(_, next)| next.is_ascii_digit() || next == '.')) => {
                let mut end = start + 1;
                let mut previous = c;
                while let Some(&(index, next)) = chars.peek() {
                    let exponent_sign = (next == '+' || next == '-') && matches!(previous, 'e' | 'E');
                    if !(next.is_ascii_alphanumeric() || next == '.' || next == '_' || exponent_sign) {
                        break;
                    }
                    end = index + 1;
                    previous = next;
                    chars.next();
                }
                let literal = &text[start..end];
                Token::Number(parse_number_literal(literal).ok_or_else(|| ExprError(format!("`{literal}` is not a number")))?)
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
                match &text[start..end] {
                    "null" => Token::Null,
                    "true" => Token::True,
                    "false" => Token::False,
                    name => Token::Name(name.to_owned()),
                }
            }
            other => return fail(format!("`{other}` is not allowed in an expression")),
        };
        tokens.push(token);
    }
    Ok(tokens)
}

/// A number literal: decimal with an optional fraction and exponent, or `0x` hexadecimal,
/// either with a leading `-`. It must be finite.
fn parse_number_literal(literal: &str) -> Option<f64> {
    let (negative, digits) = literal.strip_prefix('-').map_or((false, literal), |rest| (true, rest));
    let value = match digits.strip_prefix("0x") {
        #[allow(clippy::cast_precision_loss, reason = "expressions compute with doubles, as on GitHub")]
        Some(hex) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => u64::from_str_radix(hex, 16).ok()? as f64,
        None if is_decimal(digits) => digits.parse::<f64>().ok()?,
        Some(_) | None => return None,
    };
    let value = if negative { -value } else { value };
    value.is_finite().then_some(value)
}

/// `\d+(\.\d*)?|\.\d+`, then an optional `[eE][+-]?\d+`: JSON's number syntax, a little
/// more lenient about the fraction, and never Rust's `inf` or `nan`.
pub(super) fn is_decimal(text: &str) -> bool {
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(index) => (&text[..index], Some(&text[index + 1..])),
        None => (text, None),
    };
    let (whole, fraction) = mantissa.split_once('.').map_or((mantissa, None), |(whole, fraction)| (whole, Some(fraction)));
    let digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = digits(whole) && fraction.is_none_or(digits) && (!whole.is_empty() || fraction.is_some_and(|f| !f.is_empty()));
    let exponent_ok = exponent.is_none_or(|exponent| {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        !exponent.is_empty() && digits(exponent)
    });
    mantissa_ok && exponent_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let tokens = tokenize("a.b-c['x''y'] != !(1 <= -2.5e+3) && 0xFF >= null || true == false, x.* < y > z").unwrap();
        let printed: Vec<String> = tokens.iter().map(ToString::to_string).collect();
        assert_eq!(
            printed,
            [
                "a", ".", "b-c", "[", "'x''y'", "]", "!=", "!", "(", "1", "<=", "-2500", ")", "&&", "255", ">=", "null", "||", "true",
                "==", "false", ",", "x", ".", "*", "<", "y", ">", "z"
            ]
        );
    }

    #[test]
    fn numbers() {
        let cases = [("711", 711.0), ("-9.2", -9.2), ("-9.2e-2", -0.092), ("0xff", 255.0), ("-0x10", -16.0), ("1.", 1.0), ("2E3", 2000.0)];
        for (text, expected) in cases {
            assert_eq!(tokenize(text).unwrap(), [Token::Number(expected)], "{text}");
        }
        for invalid in ["1x", "0x", "0xg", "1e", "1e+", "1.2.3", "1e999", "0x1.5", "1_000"] {
            assert!(tokenize(invalid).is_err(), "{invalid}");
        }
        assert!(tokenize("- 1").is_err(), "a lone minus is no operator");
        assert!(is_decimal(".5") && !is_decimal(".") && !is_decimal("inf") && !is_decimal(""));
    }

    #[test]
    fn refusals() {
        for invalid in ["a = b", "a | b", "a & b", "'open", "\"double\"", "a ? b", "a + b", "é"] {
            assert!(tokenize(invalid).is_err(), "{invalid}");
        }
    }
}
