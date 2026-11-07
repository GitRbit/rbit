//! Declared job dependency existence and cycle validation.
use super::syntax::error;
use crate::error::WorkflowError;
use crate::model::Job;
use std::collections::{BTreeMap, BTreeSet};

/// Every `needs` names another job, and the jobs form no cycle.
pub(super) fn check_needs(jobs: &[Job]) -> Result<(), WorkflowError> {
    let ids: BTreeSet<&str> = jobs.iter().map(|job| job.id.as_str()).collect();
    for job in jobs {
        if let Some(missing) = job.needs.iter().find(|need| !ids.contains(need.as_str())) {
            return Err(error(format!("job `{}` needs `{missing}`, which does not exist", job.id), job.at));
        }
        if job.needs.contains(&job.id) {
            return Err(error(format!("job `{}` needs itself", job.id), job.at));
        }
    }
    // Kahn's algorithm: whatever cannot be ordered is on a cycle.
    let mut waiting: BTreeMap<&str, usize> = jobs.iter().map(|job| (job.id.as_str(), job.needs.len())).collect();
    let mut ready: Vec<&str> = waiting.iter().filter(|(_, count)| **count == 0).map(|(id, _)| *id).collect();
    while let Some(done) = ready.pop() {
        waiting.remove(done);
        for job in jobs.iter().filter(|job| job.needs.iter().any(|need| need == done)) {
            if let Some(count) = waiting.get_mut(job.id.as_str()) {
                *count -= job.needs.iter().filter(|need| *need == done).count();
                if *count == 0 {
                    ready.push(&job.id);
                }
            }
        }
    }
    match jobs.iter().find(|job| waiting.contains_key(job.id.as_str())) {
        Some(job) => Err(error(format!("job `{}` is part of a `needs` cycle", job.id), job.at)),
        None => Ok(()),
    }
}
