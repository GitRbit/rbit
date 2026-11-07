//! Runner selection, deployment environment, containers and service declarations.
use super::environment::{parse_credentials, parse_env};
use super::expressions::{Scope, template, template_field};
use super::jobs::valid_job_id;
use super::syntax::{check_keys, error, mapping, strings};
use crate::error::WorkflowError;
use crate::model::{Container, Env, EnvironmentRef, RegistryCredentials, Service};
use crate::spec::MAX_SERVICES;
use crate::yaml::{Mapping, Node, Value};
use std::collections::BTreeMap;

/// The container keys GitHub has and rbit lacks.
pub(super) const CONTAINER_UNSUPPORTED: [(&str, &str); 1] =
    [("ports", ": services reach the job container by name on the job's own network")];

/// The service keys GitHub has and rbit lacks.
pub(super) const SERVICE_UNSUPPORTED: [(&str, &str); 1] = [("ports", ": the job reaches a service by its id as host name")];

/// The keys of a container and of a service.
pub(super) const CONTAINER_KEYS: [&str; 5] = ["image", "env", "credentials", "options", "volumes"];

// Actions: environments.
/// `environment`: a name, or `{name, url}`. The name is the server's to evaluate; the URL
/// the runner's, after the steps.
pub(super) fn parse_environment(node: &Node, path: &str) -> Result<EnvironmentRef, WorkflowError> {
    let (name, url, name_at) = match &node.value {
        Value::Mapping(config) => {
            check_keys(config, path, &["name", "url"], &[])?;
            let name = config.get("name").ok_or_else(|| error(format!("`{path}` needs a `name`"), node.at))?;
            let text = name.as_text().ok_or_else(|| error(format!("`{path}.name` must be a string"), name.at))?;
            (text, template_field(config, "url", &format!("{path}.url"), Scope::Runner)?, name.at)
        }
        _ => (node.as_text().ok_or_else(|| error(format!("`{path}` must be a name or `{{name, url}}`"), node.at))?, None, node.at),
    };
    if name.trim().is_empty() {
        return Err(error(format!("`{path}` must name an environment"), name_at));
    }
    template(&name, Scope::Job, path, name_at)?;
    Ok(EnvironmentRef { name, url })
}

pub(super) fn parse_runs_on(node: &Node, path: &str) -> Result<Vec<String>, WorkflowError> {
    let labels = match &node.value {
        Value::Mapping(config) => {
            if config.get("group").is_some() {
                return Err(error(format!("`{path}`: runner groups are not supported by rbit; use `labels`"), node.at));
            }
            check_keys(config, path, &["labels"], &[])?;
            config.get("labels").map(|labels| strings(labels, &format!("{path}.labels"))).transpose()?.unwrap_or_default()
        }
        _ => strings(node, path)?,
    };
    if labels.is_empty() || labels.iter().any(String::is_empty) {
        return Err(error(format!("`{path}` must name at least one label, and no empty one"), node.at));
    }
    for label in &labels {
        template(label, Scope::Job, path, node.at)?;
    }
    Ok(labels)
}

/// `outputs`: names as GitHub allows them, values as templates.
pub(super) fn parse_outputs(node: &Node, path: &str) -> Result<BTreeMap<String, String>, WorkflowError> {
    if node.is_null() {
        return Ok(BTreeMap::new());
    }
    let mut outputs = BTreeMap::new();
    for (key, value) in mapping(node, path)?.iter() {
        let output_path = format!("{path}.{}", key.name);
        let text = value.as_text().ok_or_else(|| error(format!("`{output_path}` must be a string"), value.at))?;
        template(&text, Scope::Runner, &output_path, value.at)?;
        outputs.insert(key.name.clone(), text);
        crate::protocol::validate_outputs(&outputs).map_err(|problem| error(format!("`{path}`: {}", problem.message), key.at))?;
    }
    Ok(outputs)
}

/// `container`: an image, or `image`, `env` and `credentials`.
pub(super) fn parse_container(node: &Node, path: &str) -> Result<Container, WorkflowError> {
    if let Value::Scalar(_) = &node.value {
        let image =
            node.as_text().filter(|image| !image.is_empty()).ok_or_else(|| error(format!("`{path}` must name an image"), node.at))?;
        template(&image, Scope::Job, path, node.at)?;
        return Ok(Container { image, env: Env::new(), credentials: None, options: None, volumes: Vec::new(), at: node.at });
    }
    let config = mapping(node, path)?;
    check_keys(config, path, &CONTAINER_KEYS, &CONTAINER_UNSUPPORTED)?;
    let (image, env, credentials) = image_env_credentials(config, node, path)?;
    let (options, volumes) = options_volumes(config, path, false)?;
    Ok(Container { image, env, credentials, options, volumes, at: node.at })
}

/// `options` and `volumes` of a container or service, checked now unless they hold
/// expressions ([`crate::options`]).
pub(super) fn options_volumes(config: &Mapping, path: &str, service: bool) -> Result<(Option<String>, Vec<String>), WorkflowError> {
    let options = template_field(config, "options", &format!("{path}.options"), Scope::Job)?.filter(|text| !text.trim().is_empty());
    if let (Some(text), Some(node)) = (&options, config.get("options"))
        && !text.contains("${{")
    {
        crate::options::docker_options(text, service).map_err(|problem| error(format!("`{path}.options`: {problem}"), node.at))?;
    }
    let mut volumes = Vec::new();
    if let Some(node) = config.get("volumes").filter(|node| !node.is_null()) {
        for volume in strings(node, &format!("{path}.volumes"))? {
            template(&volume, Scope::Job, &format!("{path}.volumes"), node.at)?;
            if !volume.contains("${{") {
                crate::options::volume(&volume).map_err(|problem| error(format!("`{path}.volumes`: {problem}"), node.at))?;
            }
            volumes.push(volume);
        }
    }
    Ok((options, volumes))
}

/// `services`: at most [`MAX_SERVICES`], each `image`, `env` and `credentials`.
pub(super) fn parse_services(node: &Node, path: &str) -> Result<Vec<Service>, WorkflowError> {
    if node.is_null() {
        return Ok(Vec::new());
    }
    let entries = mapping(node, path)?;
    if entries.len() > MAX_SERVICES {
        return Err(WorkflowError::limit(format!("`{path}`: a job may have at most {MAX_SERVICES} services"), Some(node.at)));
    }
    let mut services = Vec::with_capacity(entries.len());
    for (key, value) in entries.iter() {
        let service_path = format!("{path}.{}", key.name);
        if !valid_job_id(&key.name) {
            return Err(error(
                format!("`{service_path}`: a service id must start with a letter or '_' and contain only letters, digits, '_' and '-'"),
                key.at,
            ));
        }
        let config = mapping(value, &service_path)?;
        check_keys(config, &service_path, &CONTAINER_KEYS, &SERVICE_UNSUPPORTED)?;
        let (image, env, credentials) = image_env_credentials(config, value, &service_path)?;
        let (options, volumes) = options_volumes(config, &service_path, true)?;
        services.push(Service { id: key.name.clone(), image, env, credentials, options, volumes, at: key.at });
    }
    Ok(services)
}

/// The `image` (required), `env` and `credentials` of a container or service.
pub(super) fn image_env_credentials(
    config: &Mapping,
    node: &Node,
    path: &str,
) -> Result<(String, Env, Option<RegistryCredentials>), WorkflowError> {
    let image = template_field(config, "image", &format!("{path}.image"), Scope::Job)?
        .filter(|image| !image.is_empty())
        .ok_or_else(|| error(format!("`{path}` needs an `image`"), node.at))?;
    let env = config.get("env").map(|env| parse_env(env, &format!("{path}.env"))).transpose()?.unwrap_or_default();
    let credentials = config
        .get("credentials")
        .filter(|node| !node.is_null())
        .map(|node| parse_credentials(node, &format!("{path}.credentials")))
        .transpose()?;
    Ok((image, env, credentials))
}
