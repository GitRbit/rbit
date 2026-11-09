//! How a push refused by the push gate is answered, over HTTP and SSH alike.
//!
//! Git never starts for a refused push; like a declining `pre-receive` hook the whole push
//! is refused. Clients that asked for `report-status` get the report as the receive-pack
//! result; others get a text explanation (`403` over HTTP, stderr over SSH).

use crate::receive::{self, CommandList, Refusal};

/// The answer to a refused push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushRefusal {
    /// The report (`ng <ref> <reason>`), framed as the client requested.
    Report(Vec<u8>),
    /// The client asked for no report: the explanation to show instead.
    Explanation(String),
}

/// Decides the answer to a refused push from its command list and the refusals.
pub fn push_refusal(commands: &CommandList, refusals: &[Refusal]) -> PushRefusal {
    let report = receive::refusal(commands, refusals);
    if report.is_empty() { PushRefusal::Explanation(receive::refusal_message(refusals)) } else { PushRefusal::Report(report) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receive::{PushCommand, ZERO_ID};

    fn commands(capabilities: &[&str]) -> CommandList {
        let command = PushCommand { old: ZERO_ID.to_owned(), new: "1".repeat(40), name: "refs/heads/main".to_owned() };
        CommandList::new(vec![command], capabilities.iter().map(|&c| c.to_owned()).collect(), Vec::new())
    }

    #[test]
    fn refused_pushes_are_reported_only_when_the_client_asked() {
        let refusals = [Refusal { name: "refs/heads/main".to_owned(), reason: "protected".to_owned() }];
        let with_report = commands(&["report-status"]);
        assert_eq!(push_refusal(&with_report, &refusals), PushRefusal::Report(receive::refusal(&with_report, &refusals)));
        assert_eq!(push_refusal(&commands(&[]), &refusals), PushRefusal::Explanation(receive::refusal_message(&refusals)));
    }
}
