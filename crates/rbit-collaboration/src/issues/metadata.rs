//! Selected-page metadata preserves the joined single-thread and batched fast paths.
use super::{IssueMetadata, IssueMetadataPort, MetadataNeeds};
use crate::{milestones::MilestoneRef, threads::IssueId};
use std::collections::HashMap;
#[derive(Debug)]
pub struct IssueMetadataReader<'a, P> {
    port: &'a P,
}
impl<'a, P: IssueMetadataPort> IssueMetadataReader<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }
    pub async fn read(&self, issues: &[IssueId]) -> Result<HashMap<IssueId, IssueMetadata>, P::Error> {
        let store = self.port;
        if let [issue] = issues {
            let one = store.one(*issue).await?;
            let milestone = one.milestone.map(|milestone| MilestoneRef { id: milestone.id, title: milestone.title });
            return Ok(HashMap::from([(*issue, IssueMetadata { labels: one.labels, assignees: one.assignees, milestone })]));
        }
        let (labels, assignees, milestones) = store.batch(issues).await?;
        let mut map: HashMap<IssueId, IssueMetadata> = issues.iter().map(|id| (*id, IssueMetadata::default())).collect();
        for (issue, label) in labels {
            map.entry(issue).or_default().labels.push(label);
        }
        for (issue, account) in assignees {
            map.entry(issue).or_default().assignees.push(account);
        }
        for milestone in milestones {
            map.entry(milestone.issue).or_default().milestone = Some(MilestoneRef { id: milestone.id, title: milestone.title });
        }
        Ok(map)
    }

    /// [`Self::read`] limited to what `needs` names.
    pub async fn read_for(&self, issues: &[IssueId], needs: MetadataNeeds) -> Result<HashMap<IssueId, IssueMetadata>, P::Error> {
        if needs == MetadataNeeds::All {
            return self.read(issues).await;
        }
        let labels = self.port.labels(issues).await?;
        let mut map: HashMap<IssueId, IssueMetadata> = issues.iter().map(|id| (*id, IssueMetadata::default())).collect();
        for (issue, label) in labels {
            map.entry(issue).or_default().labels.push(label);
        }
        Ok(map)
    }
}
