//! Runner claim and replay decisions; no polling, timers or persistence implementation.
use super::{Claim, ClaimQueue, LeaseReplay, Resumed};

#[derive(Debug)]
pub struct RunnerClaims;

impl RunnerClaims {
    /// Delay the store factory until after the original empty-label fast path. An
    /// unpolled command constructs nothing; a retry loop retains exactly one handle.
    pub async fn claim<P: ClaimQueue>(has_labels: bool, open: impl FnOnce() -> P, key: Option<&str>) -> Result<Claim<P::Spec>, P::Error> {
        if !has_labels {
            return Ok(Claim::Nothing);
        }
        let port = open();
        loop {
            let token = port.new_token();
            let Some(claimed) = port.claim(&token, key).await? else { return Ok(Claim::Nothing) };
            if let Some(key) = key {
                port.remember_key(key, &claimed.task);
            }
            port.claimed(&claimed);
            match port.specification(&claimed.job, &claimed.task, &token).await? {
                Ok(spec) => {
                    port.remember_masks(&spec);
                    port.create_steps(&claimed.task, &spec).await?;
                    port.report_statuses(&claimed.job).await?;
                    return Ok(Claim::Task(Box::new(spec)));
                }
                Err(reason) => {
                    port.fail_unrunnable(&claimed.task, &reason).await?;
                    if key.is_some() {
                        return Ok(Claim::Spent);
                    }
                }
            }
        }
    }

    pub async fn resume<P: LeaseReplay>(port: &P, key: &str) -> Result<Resumed<P::Spec>, P::Error> {
        let Some(task) = port.lookup(key).await?.filter(|task| port.belongs_to_runner(task)) else { return Ok(Resumed::Unknown) };
        if !port.is_running(&task) {
            return Ok(Resumed::Spent);
        }
        let token = port.new_token();
        if !port.replace_token(&task, &token).await? {
            return Ok(Resumed::Spent);
        }
        let Some(job) = port.job(&task).await? else { return Ok(Resumed::Spent) };
        port.resumed(&task);
        match port.specification(&job, &task, &token).await? {
            Ok(spec) => {
                port.remember_masks(&spec);
                Ok(Resumed::Task(Box::new(spec)))
            }
            Err(reason) => {
                port.fail_unrunnable(&task, &reason).await?;
                Ok(Resumed::Spent)
            }
        }
    }
}
