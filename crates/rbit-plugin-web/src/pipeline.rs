use std::collections::BTreeSet;

use tokio::time::{Instant, timeout_at};

use crate::{
    HookDecision, MAX_HOOK_ANNOTATIONS, MAX_HOOKS, MAX_PIPELINE_DEADLINE, MAX_RESPONSE_ANNOTATIONS, Rejection, RequestHookSnapshot,
    RequestMetadata,
};

impl RequestHookSnapshot {
    /// Run deterministic metadata hooks inside generation admission and bounded time.
    /// Cancellation drops the callback future before releasing its operation lease.
    pub async fn evaluate(&self, request: &RequestMetadata) -> HookDecision {
        if self.inner.entries.len() > MAX_HOOKS {
            return HookDecision::Reject(Rejection::Unavailable);
        }
        let deadline = Instant::now() + MAX_PIPELINE_DEADLINE;
        let mut admitted = Vec::with_capacity(self.len());
        for entry in self.inner.entries.iter() {
            let Ok(lease) = entry.admit() else { return HookDecision::Reject(Rejection::Unavailable) };
            admitted.push((entry.owner.clone(), entry.generation, lease));
        }
        admitted.sort_by(|left, right| {
            left.2
                .value()
                .order()
                .cmp(&right.2.value().order())
                .then_with(|| left.0.cmp(&right.0))
                .then_with(|| left.2.value().id().cmp(right.2.value().id()))
                .then_with(|| left.1.cmp(&right.1))
        });
        let mut annotations = Vec::new();
        let mut names = BTreeSet::new();
        for (_, _, lease) in admitted {
            let (hook, _operation, cancellation) = lease.into_parts();
            if Instant::now() >= deadline {
                return HookDecision::Reject(Rejection::Unavailable);
            }
            let hook_deadline = deadline.min(Instant::now() + hook.deadline());
            let decision = tokio::select! {
                biased;
                () = cancellation.cancelled() => return HookDecision::Reject(Rejection::Unavailable),
                result = timeout_at(hook_deadline, async { hook.callback().call(request).await }) => match result {
                    Ok(decision) => decision,
                    Err(_) => return HookDecision::Reject(Rejection::Unavailable),
                },
            };
            match decision {
                HookDecision::Reject(reason) => return HookDecision::Reject(reason),
                HookDecision::Continue(values) => {
                    if values.len() > MAX_HOOK_ANNOTATIONS
                        || annotations.len() + values.len() > MAX_RESPONSE_ANNOTATIONS
                        || values.iter().any(|value| !names.insert(value.name().to_owned()))
                    {
                        return HookDecision::Reject(Rejection::Unavailable);
                    }
                    annotations.extend(values);
                }
            }
        }
        HookDecision::Continue(annotations)
    }
}
