use crate::PackError;
use fluent_bundle::{FluentResource, concurrent::FluentBundle};
use fluent_syntax::ast;
use rbit_messages::{MessageSchema, MessageSpec};
use std::collections::BTreeSet;
use unic_langid::LanguageIdentifier;

pub(crate) const MAX_MESSAGES: usize = 4096;
pub(crate) const MAX_OUTPUT: usize = 8192;
pub(crate) const MAX_ARGUMENT: usize = 1024;
const MAX_NODES: usize = 128;
const MAX_DEPTH: usize = 8;

pub(crate) struct Catalog {
    pub bundle: FluentBundle<FluentResource>,
    pub keys: BTreeSet<String>,
}
impl std::fmt::Debug for Catalog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Catalog").field("messages", &self.keys.len()).finish_non_exhaustive()
    }
}

impl Catalog {
    pub fn parse(text: &str, locale: LanguageIdentifier, namespace: &str, schema: &MessageSchema) -> Result<Self, PackError> {
        // Guard the parser itself before it can recurse into rejected constructs.
        preflight_syntax(text)?;
        let parsed = fluent_syntax::parser::parse(text).map_err(|_| PackError::Catalog)?;
        let mut keys = BTreeSet::new();
        let mut total_nodes = 0usize;
        for entry in &parsed.body {
            match entry {
                ast::Entry::Message(message) => {
                    if !message.attributes.is_empty() || !keys.insert(message.id.name.to_owned()) {
                        return Err(PackError::Catalog);
                    }
                    if keys.len() > MAX_MESSAGES {
                        return Err(PackError::Limit);
                    }
                    let spec = schema.message(namespace, message.id.name).ok_or(PackError::Schema)?;
                    let mut budget =
                        Budget { nodes: 0, output: 0, spec, allow_select: namespace != "core" || !message.id.name.starts_with("browser-") };
                    budget.pattern(message.value.as_ref().ok_or(PackError::Catalog)?, 0)?;
                    total_nodes += budget.nodes;
                    if total_nodes > MAX_MESSAGES * 32 {
                        return Err(PackError::Limit);
                    }
                }
                ast::Entry::Comment(_) | ast::Entry::GroupComment(_) | ast::Entry::ResourceComment(_) => {}
                _ => return Err(PackError::Catalog),
            }
        }
        if keys.is_empty() {
            return Err(PackError::Catalog);
        }
        let resource = FluentResource::try_new(text.to_owned()).map_err(|_| PackError::Catalog)?;
        let mut bundle = FluentBundle::new_concurrent(vec![locale]);
        // Fluent's bidi isolation is kept enabled. No custom functions are registered.
        bundle.add_resource(resource).map_err(|_| PackError::Catalog)?;
        Ok(Self { bundle, keys })
    }
}

struct Budget<'a> {
    nodes: usize,
    output: usize,
    spec: &'a MessageSpec,
    allow_select: bool,
}
impl Budget<'_> {
    fn node(&mut self, depth: usize, bytes: usize) -> Result<(), PackError> {
        self.nodes += 1;
        self.output = self.output.checked_add(bytes).ok_or(PackError::Limit)?;
        if depth > MAX_DEPTH || self.nodes > MAX_NODES || self.output > MAX_OUTPUT {
            return Err(PackError::Limit);
        }
        Ok(())
    }
    fn pattern(&mut self, pattern: &ast::Pattern<&str>, depth: usize) -> Result<(), PackError> {
        self.node(depth, 0)?;
        for element in &pattern.elements {
            match element {
                ast::PatternElement::TextElement { value } => self.node(depth, value.len())?,
                ast::PatternElement::Placeable { expression } => {
                    self.node(depth, 6)?; // worst-case FSI/PDI isolation markers
                    self.expression(expression, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn expression(&mut self, expression: &ast::Expression<&str>, depth: usize) -> Result<(), PackError> {
        self.node(depth, 0)?;
        match expression {
            ast::Expression::Inline(inline) => self.inline(inline, depth + 1),
            ast::Expression::Select { selector, variants } => {
                // Selectors cannot evaluate nested patterns, functions or references.
                // Browser exports interpolate real values later in JavaScript; evaluating
                // a select against export sentinels would choose the wrong branch.
                if !self.allow_select || !matches!(selector, ast::InlineExpression::VariableReference { .. }) || variants.len() > 16 {
                    return Err(PackError::Catalog);
                }
                self.inline(selector, depth + 1)?;
                let mut seen = BTreeSet::new();
                for variant in variants {
                    let key = match &variant.key {
                        ast::VariantKey::Identifier { name } => (*name).to_owned(),
                        ast::VariantKey::NumberLiteral { value } => {
                            if !finite_number(value) {
                                return Err(PackError::Catalog);
                            }
                            (*value).to_owned()
                        }
                    };
                    if key.len() > 64 || !seen.insert(key) {
                        return Err(PackError::Catalog);
                    }
                    self.pattern(&variant.value, depth + 1)?;
                }
                Ok(())
            }
        }
    }
    fn inline(&mut self, inline: &ast::InlineExpression<&str>, depth: usize) -> Result<(), PackError> {
        match inline {
            ast::InlineExpression::VariableReference { id } => {
                if !self.spec.args.contains_key(id.name) {
                    return Err(PackError::Schema);
                }
                self.node(depth, MAX_ARGUMENT)
            }
            ast::InlineExpression::StringLiteral { value } => self.node(depth, value.len()),
            ast::InlineExpression::NumberLiteral { value } => {
                if !finite_number(value) {
                    return Err(PackError::Catalog);
                }
                self.node(depth, MAX_ARGUMENT)
            }
            // No attributes, message/term references, nested inline placeables or
            // functions: evaluation is a finite tree with no call graph or hooks.
            _ => Err(PackError::Catalog),
        }
    }
}
fn finite_number(value: &str) -> bool {
    value.len() <= 32 && value.parse::<f64>().is_ok_and(f64::is_finite)
}

/// Track placeables versus select-pattern text without constructing an AST.
/// Functions/term calls recurse while parsing their arguments, so reject their
/// opening parenthesis immediately. Quotes in select text remain ordinary text;
/// quoted expression literals cannot hide nesting or cross a newline.
fn preflight_syntax(text: &str) -> Result<(), PackError> {
    let bytes = text.as_bytes();
    let mut stack = Vec::new(); // false: expression; true: select pattern
    let mut position = 0usize;
    while let Some(&byte) = bytes.get(position) {
        if byte == b'#' && stack.is_empty() && (position == 0 || bytes[position - 1] == b'\n') {
            while bytes.get(position).is_some_and(|byte| *byte != b'\n') {
                position += 1;
            }
            continue;
        }
        let expression = stack.last() == Some(&false);
        if expression && byte == b'"' {
            position += 1;
            loop {
                match bytes.get(position) {
                    None | Some(b'\n' | b'\r') => return Err(PackError::Catalog),
                    Some(b'"') => break,
                    Some(b'\\') => {
                        position += 1;
                        if matches!(bytes.get(position), None | Some(b'\n' | b'\r')) {
                            return Err(PackError::Catalog);
                        }
                    }
                    _ => {}
                }
                position += 1;
            }
        } else if expression && byte == b'(' {
            return Err(PackError::Catalog);
        } else if expression && byte == b'-' && bytes.get(position + 1) == Some(&b'>') {
            *stack.last_mut().ok_or(PackError::Catalog)? = true;
            position += 1;
        } else if byte == b'{' {
            stack.push(false);
            if stack.len() > MAX_DEPTH {
                return Err(PackError::Limit);
            }
        } else if byte == b'}' {
            stack.pop();
        }
        position += 1;
    }
    Ok(())
}
