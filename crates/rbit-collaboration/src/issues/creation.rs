//! Ordered plain-issue creation; SQL and cross-feature effects remain behind static ports.

use std::collections::BTreeSet;

use rbit_contracts::AccountId;
use rbit_identity::User;

use super::{IssueCreationPort, MAX_ASSIGNEES, MAX_LABELS_PER_ISSUE, MetadataCreated, NewIssueInput};
use crate::{labels::LabelId, threads::Issue};

#[derive(Debug, Clone, Copy)]
pub struct IssueCreationAccess {
    pub can_read: bool,
    pub can_triage: bool,
    pub archived: bool,
}

#[derive(Debug)]
pub enum IssueCreationError<E> {
    Port(E),
    NotFound,
    Archived,
    Invalid(String),
}

#[derive(Debug)]
pub struct IssueCreation<'a, P> {
    port: &'a P,
    access: IssueCreationAccess,
}

impl<'a, P: IssueCreationPort> IssueCreation<'a, P> {
    pub fn new(port: &'a P, access: IssueCreationAccess) -> Self {
        Self { port, access }
    }

    pub async fn create(&self, input: NewIssueInput) -> Result<Issue, IssueCreationError<P::Error>> {
        if !self.access.can_read {
            return Err(IssueCreationError::NotFound);
        }
        if self.access.archived {
            return Err(IssueCreationError::Archived);
        }
        let (labels, assignees, milestone) = if self.access.can_triage {
            let labels = self.port.labels(&distinct(&input.labels)).await.map_err(IssueCreationError::Port)?;
            if labels.len() > MAX_LABELS_PER_ISSUE {
                return Err(IssueCreationError::Invalid(format!("an issue can have at most {MAX_LABELS_PER_ISSUE} labels")));
            }
            let assignees = self.port.assignees(&distinct(&input.assignees)).await.map_err(IssueCreationError::Port)?;
            if assignees.len() > MAX_ASSIGNEES {
                return Err(IssueCreationError::Invalid(format!("an issue can have at most {MAX_ASSIGNEES} assignees")));
            }
            let milestone = match input.milestone {
                Some(id) => Some(
                    self.port
                        .milestone(id)
                        .await
                        .map_err(IssueCreationError::Port)?
                        .ok_or_else(|| IssueCreationError::Invalid("unknown milestone".into()))?,
                ),
                None => None,
            };
            (labels, assignees, milestone)
        } else {
            (Vec::new(), Vec::new(), None)
        };

        let issue = self.port.create_thread(&input.title, &input.body).await.map_err(IssueCreationError::Port)?;
        if !labels.is_empty() || !assignees.is_empty() || milestone.is_some() {
            let label_ids: Vec<LabelId> = labels.iter().map(|label| label.id).collect();
            let assignee_ids: Vec<AccountId> = assignees.iter().map(User::id).collect();
            self.port
                .attach(issue.id, &label_ids, &assignee_ids, milestone.as_ref().map(|value| value.id))
                .await
                .map_err(IssueCreationError::Port)?;
            for label in &labels {
                self.port.record(issue.id, MetadataCreated::Labeled(label.snapshot())).await.map_err(IssueCreationError::Port)?;
            }
            for assignee in &assignees {
                self.port.record(issue.id, MetadataCreated::Assigned(assignee.id())).await.map_err(IssueCreationError::Port)?;
            }
            if let Some(milestone) = &milestone {
                self.port
                    .record(issue.id, MetadataCreated::Milestoned { id: milestone.id, title: milestone.title.clone() })
                    .await
                    .map_err(IssueCreationError::Port)?;
            }
        }
        self.port.publish_opened(&issue);
        Ok(issue)
    }
}

fn distinct<T: Ord + Copy>(items: &[T]) -> Vec<T> {
    let mut seen = BTreeSet::new();
    items.iter().copied().filter(|item| seen.insert(*item)).collect()
}
