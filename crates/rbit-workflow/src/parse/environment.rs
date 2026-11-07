//! Environment mappings, run defaults, shell and registry credential syntax.
use super::expressions::{Scope, template, template_field};
use super::syntax::{check_keys, error, mapping};
use crate::error::WorkflowError;
use crate::model::{Env, RegistryCredentials, RunDefaults, Shell};
use crate::spec::MAX_NAME_BYTES;
use crate::yaml::{Node, Value};

/// An `env` mapping: names without `=` or NUL, scalar values as templates.
pub(crate) fn parse_env(node: &Node, path: &str) -> Result<Env, WorkflowError> {
    if node.is_null() {
        return Ok(Env::new());
    }
    let entries =
        node.as_mapping().ok_or_else(|| error(format!("`{path}` must be a mapping; rbit does not evaluate expressions there"), node.at))?;
    let mut env = Env::new();
    for (key, value) in entries.iter() {
        let name_path = format!("{path}.{}", key.name);
        if key.name.is_empty() || key.name.len() > MAX_NAME_BYTES || key.name.contains(['=', '\0']) {
            return Err(error(format!("`{name_path}`: a variable name has 1 to {MAX_NAME_BYTES} bytes, without `=`"), key.at));
        }
        let text = match &value.value {
            Value::Scalar(_) if value.is_null() => String::new(),
            Value::Scalar(_) => value.as_text().unwrap_or_default(),
            _ => return Err(error(format!("`{name_path}` must be a string, number or boolean"), value.at)),
        };
        template(&text, Scope::Runner, &name_path, value.at)?;
        env.insert(key.name.clone(), text);
    }
    Ok(env)
}

/// `defaults`: only `run.shell` and `run.working-directory`.
pub(super) fn parse_defaults(node: &Node, path: &str) -> Result<RunDefaults, WorkflowError> {
    if node.is_null() {
        return Ok(RunDefaults::default());
    }
    let config = mapping(node, path)?;
    check_keys(config, path, &["run"], &[])?;
    let Some(run) = config.get("run").filter(|node| !node.is_null()) else {
        return Ok(RunDefaults::default());
    };
    let run_path = format!("{path}.run");
    let run = mapping(run, &run_path)?;
    check_keys(run, &run_path, &["shell", "working-directory"], &[])?;
    let shell = run.get("shell").map(|node| parse_shell(node, &format!("{run_path}.shell"))).transpose()?;
    let working_directory = template_field(run, "working-directory", &format!("{run_path}.working-directory"), Scope::Runner)?;
    Ok(RunDefaults { shell, working_directory })
}

/// A `shell`: one of [`Shell`], or an expression for the runner.
pub(super) fn parse_shell(node: &Node, path: &str) -> Result<String, WorkflowError> {
    let name = node.as_text().ok_or_else(|| error(format!("`{path}` must be a string"), node.at))?;
    if name.contains("${{") {
        template(&name, Scope::Runner, path, node.at)?;
    } else if name.contains("{0}") {
        // A custom shell, as on GitHub: a command whose `{0}` is the script's path.
        let words = crate::options::split(&name).map_err(|problem| error(format!("`{path}`: {problem}"), node.at))?;
        if words.first().is_none_or(|program| program.contains("{0}")) {
            return Err(error(format!("`{path}`: a custom shell starts with the program that runs the script"), node.at));
        }
    } else if Shell::parse(&name).is_none() {
        let supported: Vec<&str> = Shell::ALL.iter().map(|shell| shell.name()).collect();
        let message = format!(
            "`{path}`: `{name}` is not a shell rbit supports ({}), nor a command with `{{0}}` for the script",
            supported.join(", ")
        );
        return Err(error(message, node.at));
    }
    Ok(name)
}

/// `credentials` of a container or service: `username` and `password`, as templates.
pub(super) fn parse_credentials(node: &Node, path: &str) -> Result<RegistryCredentials, WorkflowError> {
    let config = mapping(node, path)?;
    check_keys(config, path, &["username", "password"], &[])?;
    let field = |key: &str| -> Result<String, WorkflowError> {
        template_field(config, key, &format!("{path}.{key}"), Scope::Runner)?
            .ok_or_else(|| error(format!("`{path}` needs `{key}`"), node.at))
    };
    Ok(RegistryCredentials { username: field("username")?, password: field("password")? })
}
