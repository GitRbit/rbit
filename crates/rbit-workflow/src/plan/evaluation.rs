//! Evaluate job fields with their original diagnostics, coercions and bounds.
use crate::error::WorkflowError;
use crate::expr::{Contexts, Template, to_number, to_text};
use crate::model::Job;
use serde_json::Value;

/// Evaluates one job's server-side fields, with errors that name the job and the field.
pub(super) struct Evaluator<'a> {
    pub(super) job: &'a Job,
    pub(super) contexts: &'a dyn Contexts,
}

impl Evaluator<'_> {
    pub(super) fn refuse(&self, field: &str, problem: &dyn std::fmt::Display) -> WorkflowError {
        WorkflowError::plan(format!("job `{}`: `{field}`: {problem}", self.job.id), Some(self.job.at))
    }

    pub(super) fn value(&self, field: &str, text: &str) -> Result<Value, WorkflowError> {
        Template::parse(text).and_then(|template| template.evaluate(self.contexts)).map_err(|problem| self.refuse(field, &problem))
    }

    pub(super) fn text(&self, field: &str, text: &str) -> Result<String, WorkflowError> {
        Template::parse(text).and_then(|template| template.render(self.contexts)).map_err(|problem| self.refuse(field, &problem))
    }

    /// `options` of `field` (`container`, `services.<id>`), evaluated and checked.
    pub(super) fn options(&self, field: &str, text: Option<&str>, service: bool) -> Result<Vec<String>, WorkflowError> {
        let Some(text) = text else { return Ok(Vec::new()) };
        let field = format!("{field}.options");
        let text = self.text(&field, text)?;
        crate::options::docker_options(&text, service).map_err(|problem| self.refuse(&field, &problem))
    }

    /// `volumes` of `field`, evaluated and checked.
    pub(super) fn volumes(&self, field: &str, volumes: &[String]) -> Result<Vec<String>, WorkflowError> {
        let field = format!("{field}.volumes");
        volumes
            .iter()
            .map(|volume| {
                let volume = self.text(&field, volume)?;
                crate::options::volume(&volume).map_err(|problem| self.refuse(&field, &problem))?;
                Ok(volume.trim().to_owned())
            })
            .collect()
    }

    pub(super) fn image(&self, field: &str, text: &str) -> Result<String, WorkflowError> {
        let image = self.text(field, text)?;
        if image.trim().is_empty() || image.chars().any(char::is_whitespace) {
            return Err(self.refuse(field, &format!("`{image}` is not an image name")));
        }
        Ok(image)
    }

    /// `true`, `false`, or an expression giving one of them.
    pub(super) fn flag(&self, field: &str, text: &str) -> Result<bool, WorkflowError> {
        match self.value(field, text)? {
            Value::Bool(value) => Ok(value),
            Value::String(value) if value.eq_ignore_ascii_case("true") => Ok(true),
            Value::String(value) if value.eq_ignore_ascii_case("false") => Ok(false),
            other => Err(self.refuse(field, &format!("must be true or false, not `{}`", to_text(&other)))),
        }
    }

    /// A positive number of minutes, rounded up.
    pub(super) fn minutes(&self, field: &str, text: &str) -> Result<u32, WorkflowError> {
        let value = self.value(field, text)?;
        let minutes = to_number(&value);
        if !(minutes > 0.0 && minutes.is_finite()) {
            return Err(self.refuse(field, &format!("must be a positive number, not `{}`", to_text(&value))));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "positive, and saturated at u32::MAX")]
        Ok(minutes.ceil().min(f64::from(u32::MAX)) as u32)
    }

    /// A whole number of at least 1.
    pub(super) fn whole_number(&self, field: &str, text: &str) -> Result<u32, WorkflowError> {
        let value = self.value(field, text)?;
        let number = to_number(&value);
        if !(number >= 1.0 && number.fract() == 0.0 && number <= f64::from(u32::MAX)) {
            return Err(self.refuse(field, &format!("must be a whole number of at least 1, not `{}`", to_text(&value))));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "checked to be a whole number in range")]
        Ok(number as u32)
    }

    /// `runs-on`, evaluated. A label that is exactly one expression whose value is a list
    /// (`${{ matrix.labels }}`) contributes every item.
    pub(super) fn runs_on(&self) -> Result<Vec<String>, WorkflowError> {
        let mut labels = Vec::new();
        for label in &self.job.runs_on {
            match self.value("runs-on", label)? {
                Value::Array(items) => {
                    for item in items {
                        let Value::String(item) = item else {
                            return Err(self.refuse("runs-on", &format!("`{label}` must give strings")));
                        };
                        labels.push(item);
                    }
                }
                Value::Object(_) => return Err(self.refuse("runs-on", &format!("`{label}` must give a string or a list of strings"))),
                other => labels.push(to_text(&other)),
            }
        }
        if labels.is_empty() || labels.iter().any(String::is_empty) {
            return Err(self.refuse("runs-on", &"evaluates to an empty label"));
        }
        Ok(labels)
    }
}
