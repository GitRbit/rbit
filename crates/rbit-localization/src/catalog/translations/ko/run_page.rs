//! Korean (`ko`) entries for `catalog/run_page.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_page::RunPageMessage;

entries! { Ko:
    RunPageMessage<'a> => MessageWriter |text| {
        RunPageMessage::ApprovalTitle => text.literal("이 실행은 승인을 기다리고 있습니다"),
        RunPageMessage::ApprovalExplanation => text.literal("포크나 쓰기 권한이 없는 사람에게서 온 실행입니다. 워크플로가 리뷰하지 않은 코드를 실행할 수 있으므로 쓰기 권한이 있는 사람이 먼저 승인해야 합니다. 승인되더라도 읽기 전용 토큰으로 실행되며 시크릿과 변수는 받지 않습니다."),
        RunPageMessage::TrustAuthor => text.literal("이 작성자의 풀 리퀘스트를 항상 승인 없이 실행"),
        RunPageMessage::ApproveAndRun => text.literal("승인 후 실행"),
        RunPageMessage::UntrustedNote => text.literal("포크나 외부 작성자에게서 온 실행입니다. 읽기 전용 토큰을 사용하며 시크릿과 변수는 받지 않습니다."),
        RunPageMessage::UntrustedApprovedNote { approver, .. } => {
            text.literal("포크나 외부 작성자에게서 온 실행입니다. 읽기 전용 토큰을 사용하며 시크릿과 변수는 받지 않습니다. 승인자: ");
            text.argument(approver);
            text.literal(".");
        }
        RunPageMessage::DeploymentWaiting => text.literal("리뷰 대기 중:"),
        RunPageMessage::DeploymentJobs { jobs, environment, .. } => {
            text.argument(jobs);
            text.literal(" 작업이 ");
            text.argument(environment);
            text.literal(" 환경에 배포되며, 먼저 이 환경의 리뷰어가 승인해야 합니다.");
        }
        RunPageMessage::Comment => text.literal("댓글"),
        RunPageMessage::Optional => text.literal("(선택 사항)"),
        RunPageMessage::ApproveAndDeploy => text.literal("승인 후 배포"),
        RunPageMessage::Reject => text.literal("거부"),
        RunPageMessage::Needs { jobs, .. } => {
            text.literal("선행 작업: ");
            text.argument(jobs);
        }
    }
}
