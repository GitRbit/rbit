//! Native workflow dispatch input presentation, with stable form field names.

// Actions: dispatch form.
/// The form that runs a workflow by hand. Choosing the branch or tag reloads the page with
/// `?ref=`, so the inputs are always those of the workflow at that ref; no JavaScript needed.
#[derive(Debug, Clone)]
pub struct DispatchFormView {
    /// Where the form posts.
    pub action: String,
    /// The workflow's page, which the ref form reloads.
    pub page_url: String,
    /// The branch or tag, as short as it was given.
    pub reference: String,
    /// Branches, then tags, offered while typing a ref.
    pub refs: Vec<String>,
    pub inputs: Vec<DispatchInputView>,
    /// Why the last dispatch, or reading the workflow, failed.
    pub error: Option<String>,
    /// Whether the workflow was read and can run (not only its error shown).
    pub can_run: bool,
    /// Open when there is something to fix or a ref was just chosen.
    pub open: bool,
    /// The workflow's `name`, when it was read: a workflow that never ran is otherwise
    /// known only by its file.
    pub name: Option<String>,
}

/// One `workflow_dispatch` input, rendered by its type.
#[derive(Debug, Clone)]
pub struct DispatchInputView {
    pub name: String,
    /// The form field: `inputs[name]`.
    pub field: String,
    /// The element id: `dispatch-input-<n>`.
    pub id: String,
    pub description: Option<String>,
    pub required: bool,
    /// `text`, `number`, `boolean` or `choice`.
    pub kind: &'static str,
    /// The value shown: what was submitted, else the default.
    pub value: String,
    pub options: Vec<DispatchOption>,
}

#[derive(Debug, Clone)]
pub struct DispatchOption {
    pub value: String,
    pub selected: bool,
}

impl DispatchInputView {
    /// The `index`th input, showing `submitted` when the form is shown again.
    pub fn new(index: usize, input: &rbit_workflow::model::DispatchInput, submitted: Option<&str>) -> Self {
        use rbit_workflow::model::InputKind;
        let value = submitted.map(str::to_owned).or_else(|| input.default.clone()).unwrap_or_default();
        let (kind, options) = match &input.kind {
            InputKind::String | InputKind::Environment => ("text", Vec::new()),
            InputKind::Number => ("number", Vec::new()),
            InputKind::Boolean => ("boolean", Vec::new()),
            InputKind::Choice(options) => {
                ("choice", options.iter().map(|option| DispatchOption { value: option.clone(), selected: *option == value }).collect())
            }
        };
        Self {
            name: input.name.clone(),
            field: format!("inputs[{}]", input.name),
            id: format!("dispatch-input-{index}"),
            description: input.description.clone().filter(|description| !description.trim().is_empty()),
            required: input.required,
            kind,
            value,
            options,
        }
    }

    /// A boolean input is ticked when its value is `true`.
    pub fn checked(&self) -> bool {
        self.value.eq_ignore_ascii_case("true")
    }
}
