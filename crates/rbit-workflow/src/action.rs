//! An action's metadata file, `action.yml` (Actions: third-party actions).
//!
//! The runner reads it after fetching an action ([`parse_action`]): the inputs it takes
//! (with their defaults, which may be expressions such as `${{ github.token }}`), the
//! outputs it declares, and how it runs:
//!
//! * **JavaScript** (`using: node12` to `node24`): `main`, and optional `pre` and `post`
//!   scripts with `pre-if` and `post-if` conditions. Whatever version an action names, the
//!   runner runs it with its own Node.
//! * **Composite** (`using: composite`): steps, parsed as workflow steps are, so a composite
//!   step's `uses:` resolves as a workflow's does; a `run` step must name its `shell`, as on
//!   GitHub. Outputs map to expressions over the composite's `steps`.
//! * **Docker** (`using: docker`): an `image` (`Dockerfile` or `docker://…`), `entrypoint`,
//!   `args`, `env`, and `pre-entrypoint` / `post-entrypoint`.
//!
//! Keys rbit does not use (`author`, `description`, `branding`) are ignored, so any
//! action's file loads. Unknown `using` values are errors that name the action.

use std::collections::BTreeMap;

use crate::{
    error::WorkflowError,
    limits::Limits,
    parse::{parse_env, parse_steps},
    plan::step_template,
    spec::{Env, StepAction, StepTemplate},
    yaml::{self, Mapping, Node, Value},
};

/// The parts of `action.yml` the runner uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionManifest {
    pub name: String,
    /// In the file's order.
    pub inputs: Vec<ActionInput>,
    pub outputs: Vec<ActionOutput>,
    pub runs: Runs,
}

/// One input. Names are matched case-insensitively, as GitHub does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionInput {
    pub name: String,
    pub required: bool,
    /// The default, a template.
    pub default: Option<String>,
    pub deprecation: Option<String>,
}

/// One output; `value` is a template (composite actions only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOutput {
    pub name: String,
    pub value: Option<String>,
}

/// How an action runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runs {
    Node(NodeRuns),
    Composite(Vec<StepTemplate>),
    Docker(DockerRuns),
}

/// A JavaScript action's scripts, relative to the action's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRuns {
    /// `node20`, as written (lower case).
    pub using: String,
    pub main: String,
    pub pre: Option<String>,
    /// `pre-if`; `None` means `always()`.
    pub pre_if: Option<String>,
    pub post: Option<String>,
    /// `post-if`; `None` means `always()`.
    pub post_if: Option<String>,
}

/// A container action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerRuns {
    /// `Dockerfile` (a path in the action) or `docker://image`.
    pub image: String,
    pub entrypoint: Option<String>,
    pub args: Vec<String>,
    pub env: Env,
    pub pre_entrypoint: Option<String>,
    pub post_entrypoint: Option<String>,
    pub post_if: Option<String>,
}

/// The JavaScript runtimes an action may name.
const NODE_VERSIONS: [&str; 5] = ["node12", "node16", "node20", "node22", "node24"];

fn error(message: impl Into<String>, at: crate::error::Location) -> WorkflowError {
    WorkflowError::parse(message, at)
}

/// A scalar as text; `None` for null, an error for a list or mapping.
fn scalar(node: &Node, path: &str) -> Result<Option<String>, WorkflowError> {
    match &node.value {
        Value::Scalar(_) if node.is_null() => Ok(None),
        Value::Scalar(_) => Ok(node.as_text()),
        _ => Err(error(format!("`{path}` must be a string"), node.at)),
    }
}

fn text(mapping: &Mapping, key: &str, path: &str) -> Result<Option<String>, WorkflowError> {
    mapping.get(key).map(|node| scalar(node, &format!("{path}.{key}"))).transpose().map(Option::flatten)
}

/// Parses `action.yml` (its UTF-8 bytes).
pub fn parse_action(source: &[u8], limits: &Limits) -> Result<ActionManifest, WorkflowError> {
    if source.len() > limits.max_bytes {
        return Err(WorkflowError::limit(format!("action.yml is larger than {} bytes", limits.max_bytes), None));
    }
    let source = std::str::from_utf8(source).map_err(|_| WorkflowError::new(crate::error::ErrorKind::Parse, "action.yml is not UTF-8"))?;
    let document = yaml::parse_document(source, limits)?;
    let Value::Mapping(document) = &document.value else {
        return Err(error("action.yml must be a mapping with `runs`", document.at));
    };
    let name = text(document, "name", "")?.unwrap_or_default();

    let mut inputs = Vec::new();
    if let Some(node) = document.get("inputs").filter(|node| !node.is_null()) {
        let mapping = node.as_mapping().ok_or_else(|| error("`inputs` must be a mapping", node.at))?;
        for (key, value) in mapping.iter() {
            let path = format!("inputs.{}", key.name);
            let (required, default, deprecation) = match value.as_mapping() {
                Some(fields) => (
                    fields.get("required").is_some_and(|node| node.as_text().is_some_and(|text| text.eq_ignore_ascii_case("true"))),
                    text(fields, "default", &path)?,
                    text(fields, "deprecationMessage", &path)?,
                ),
                None if value.is_null() => (false, None, None),
                None => return Err(error(format!("`{path}` must be a mapping"), value.at)),
            };
            inputs.push(ActionInput { name: key.name.clone(), required, default, deprecation });
        }
    }

    let mut outputs = Vec::new();
    if let Some(node) = document.get("outputs").filter(|node| !node.is_null()) {
        let mapping = node.as_mapping().ok_or_else(|| error("`outputs` must be a mapping", node.at))?;
        for (key, value) in mapping.iter() {
            let path = format!("outputs.{}", key.name);
            let value = match value.as_mapping() {
                Some(fields) => text(fields, "value", &path)?,
                None => None,
            };
            outputs.push(ActionOutput { name: key.name.clone(), value });
        }
    }

    let (_, runs_node) =
        document.entry("runs").ok_or_else(|| WorkflowError::new(crate::error::ErrorKind::Parse, "action.yml has no `runs` section"))?;
    let runs = runs_node.as_mapping().ok_or_else(|| error("`runs` must be a mapping", runs_node.at))?;
    let using = text(runs, "using", "runs")?.ok_or_else(|| error("`runs.using` is missing", runs_node.at))?.to_ascii_lowercase();
    let runs = if NODE_VERSIONS.contains(&using.as_str()) {
        let main =
            text(runs, "main", "runs")?.filter(|main| !main.is_empty()).ok_or_else(|| error("`runs.main` is missing", runs_node.at))?;
        Runs::Node(NodeRuns {
            using,
            main,
            pre: text(runs, "pre", "runs")?,
            pre_if: text(runs, "pre-if", "runs")?,
            post: text(runs, "post", "runs")?,
            post_if: text(runs, "post-if", "runs")?,
        })
    } else if using == "composite" {
        let steps_node = runs.get("steps").ok_or_else(|| error("`runs.steps` is missing", runs_node.at))?;
        let steps = parse_steps(steps_node, "runs.steps")?;
        for (index, step) in steps.iter().enumerate() {
            if matches!(step.action, StepAction::Run(_)) && step.shell.is_none() {
                return Err(error(format!("`runs.steps[{index}]`: a composite action's `run` step needs `shell`"), step.at));
            }
        }
        Runs::Composite(steps.iter().map(step_template).collect())
    } else if using == "docker" {
        let image =
            text(runs, "image", "runs")?.filter(|image| !image.is_empty()).ok_or_else(|| error("`runs.image` is missing", runs_node.at))?;
        let args = match runs.get("args").filter(|node| !node.is_null()) {
            None => Vec::new(),
            Some(node) => node
                .as_sequence()
                .ok_or_else(|| error("`runs.args` must be a list", node.at))?
                .iter()
                .map(|arg| scalar(arg, "runs.args").map(Option::unwrap_or_default))
                .collect::<Result<_, _>>()?,
        };
        Runs::Docker(DockerRuns {
            image,
            entrypoint: text(runs, "entrypoint", "runs")?,
            args,
            env: runs.get("env").map(|node| parse_env(node, "runs.env")).transpose()?.unwrap_or_default(),
            pre_entrypoint: text(runs, "pre-entrypoint", "runs")?,
            post_entrypoint: text(runs, "post-entrypoint", "runs")?,
            post_if: text(runs, "post-if", "runs")?,
        })
    } else {
        return Err(error(format!("`runs.using: {using}` is not supported; rbit runs node, composite and docker actions"), runs_node.at));
    };
    Ok(ActionManifest { name, inputs, outputs, runs })
}

impl ActionManifest {
    /// The input called `name`, ignoring case.
    pub fn input(&self, name: &str) -> Option<&ActionInput> {
        self.inputs.iter().find(|input| input.name.eq_ignore_ascii_case(name))
    }
}

/// `INPUT_<NAME>`: upper case, spaces as `_`, as `@actions/core` reads it.
pub fn input_variable(name: &str) -> String {
    format!("INPUT_{}", name.replace(' ', "_").to_ascii_uppercase())
}

/// Every input an action's step gets: `with:` values over the action's defaults, keyed by
/// the action's own spelling of the name (inputs it does not declare are kept, with a
/// warning from the runner, as on GitHub).
pub fn merge_inputs(manifest: &ActionManifest, with: &BTreeMap<String, String>) -> Vec<(String, Option<String>)> {
    let mut merged: Vec<(String, Option<String>)> = manifest
        .inputs
        .iter()
        .map(|input| {
            (input.name.clone(), with.iter().find(|(key, _)| key.eq_ignore_ascii_case(&input.name)).map(|(_, value)| value.clone()))
        })
        .collect();
    for (key, value) in with {
        if manifest.input(key).is_none() {
            merged.push((key.clone(), Some(value.clone())));
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Result<ActionManifest, WorkflowError> {
        parse_action(source.as_bytes(), &Limits::default())
    }

    #[test]
    fn javascript_actions() {
        let manifest = parse(
            "name: Setup Node\ndescription: x\nauthor: GitHub\nbranding: {icon: code}\ninputs:\n  node-version:\n    description: v\n  token:\n    default: ${{ github.token }}\n  cache:\n    required: false\n    deprecationMessage: use x\n  must:\n    required: true\noutputs:\n  node-version:\n    description: v\nruns:\n  using: Node24\n  main: dist/setup/index.js\n  post: dist/cache-save/index.js\n  post-if: success()\n",
        )
        .unwrap();
        assert_eq!(manifest.name, "Setup Node");
        assert_eq!(manifest.inputs.len(), 4);
        assert_eq!(manifest.input("TOKEN").unwrap().default.as_deref(), Some("${{ github.token }}"));
        assert!(manifest.input("must").unwrap().required && !manifest.input("cache").unwrap().required);
        assert_eq!(manifest.input("cache").unwrap().deprecation.as_deref(), Some("use x"));
        assert_eq!(manifest.outputs, [ActionOutput { name: "node-version".into(), value: None }]);
        let Runs::Node(node) = manifest.runs else { panic!() };
        assert_eq!((node.using.as_str(), node.main.as_str()), ("node24", "dist/setup/index.js"));
        assert_eq!((node.post.as_deref(), node.post_if.as_deref(), node.pre), (Some("dist/cache-save/index.js"), Some("success()"), None));
    }

    #[test]
    fn composite_actions() {
        let manifest = parse(
            "name: Greet\ninputs:\n  who: {default: world}\noutputs:\n  greeting:\n    value: ${{ steps.say.outputs.text }}\nruns:\n  using: composite\n  steps:\n    - id: say\n      run: echo \"text=hello ${{ inputs.who }}\" >> \"$GITHUB_OUTPUT\"\n      shell: bash\n    - uses: actions/checkout@v4\n    - uses: ./inner\n",
        )
        .unwrap();
        assert_eq!(manifest.outputs[0].value.as_deref(), Some("${{ steps.say.outputs.text }}"));
        let Runs::Composite(steps) = &manifest.runs else { panic!() };
        assert_eq!(steps.len(), 3);
        assert!(matches!(steps[1].action, StepAction::Uses(_)) && matches!(steps[2].action, StepAction::Action(_)));
        let error = parse("runs:\n  using: composite\n  steps:\n    - run: echo hi\n").unwrap_err();
        assert!(error.message.contains("needs `shell`"), "{error}");
    }

    #[test]
    fn docker_actions_and_errors() {
        let manifest =
            parse("runs:\n  using: docker\n  image: Dockerfile\n  args: ['${{ inputs.x }}', 2]\n  env: {A: b}\n  entrypoint: /run.sh\n")
                .unwrap();
        let Runs::Docker(docker) = manifest.runs else { panic!() };
        assert_eq!(
            (docker.image.as_str(), docker.args.clone(), docker.entrypoint.as_deref()),
            ("Dockerfile", vec!["${{ inputs.x }}".to_owned(), "2".to_owned()], Some("/run.sh"))
        );
        assert_eq!(docker.env.get("A").map(String::as_str), Some("b"));
        for (source, message) in [
            ("name: x\n", "no `runs` section"),
            ("runs:\n  using: node20\n", "`runs.main` is missing"),
            ("runs:\n  using: deno\n  main: x\n", "`runs.using: deno` is not supported"),
            ("runs:\n  using: docker\n", "`runs.image` is missing"),
            ("- a\n", "must be a mapping"),
            ("inputs: [a]\nruns: {using: node20, main: x}\n", "`inputs` must be a mapping"),
        ] {
            let error = parse(source).unwrap_err();
            assert!(error.message.contains(message), "{source}: {error}");
        }
    }

    #[test]
    fn inputs_merge_with_defaults() {
        let manifest = parse("inputs:\n  Node-Version: {}\n  token: {default: x}\nruns: {using: node20, main: m}\n").unwrap();
        let with = BTreeMap::from([("node-version".to_owned(), "22".to_owned()), ("extra".to_owned(), "1".to_owned())]);
        let merged = merge_inputs(&manifest, &with);
        assert_eq!(
            merged,
            [("Node-Version".to_owned(), Some("22".to_owned())), ("token".to_owned(), None), ("extra".to_owned(), Some("1".to_owned()))]
        );
        assert_eq!(input_variable("Node-Version"), "INPUT_NODE-VERSION");
        assert_eq!(input_variable("my input"), "INPUT_MY_INPUT");
    }
}
