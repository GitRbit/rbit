//! Korean (`ko`) entries for `catalog/overview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview::ActionsOverviewMessage;

entries! { Ko:
    ActionsOverviewMessage => MessageWriter |text| {
        ActionsOverviewMessage::Workflows => text.literal("워크플로"),
        ActionsOverviewMessage::AllWorkflows => text.literal("모든 워크플로"),
        ActionsOverviewMessage::AllRuns => text.literal("모든 워크플로 실행"),
        ActionsOverviewMessage::Filters => text.literal("워크플로 실행 필터"),
        ActionsOverviewMessage::Branch => text.literal("브랜치"),
        ActionsOverviewMessage::Actor => text.literal("사용자"),
        ActionsOverviewMessage::Event => text.literal("이벤트"),
        ActionsOverviewMessage::Status => text.literal("상태"),
        ActionsOverviewMessage::AnyEvent => text.literal("모든 이벤트"),
        ActionsOverviewMessage::AnyStatus => text.literal("모든 상태"),
        ActionsOverviewMessage::Filter => text.literal("필터"),
        ActionsOverviewMessage::Clear => text.literal("지우기"),
        ActionsOverviewMessage::EnableWorkflow => text.literal("워크플로 활성화"),
        ActionsOverviewMessage::DisableWorkflow => text.literal("워크플로 비활성화"),
        ActionsOverviewMessage::WorkflowDisabled => text.literal("이 워크플로는 비활성화되어 있습니다. 활성화하기 전까지 어떤 이벤트로도 실행이 시작되지 않습니다."),
        ActionsOverviewMessage::NoMatchingRuns => text.literal("일치하는 실행이 없습니다"),
        ActionsOverviewMessage::NoMatchingRunsDescription => text.literal("이 필터와 일치하는 워크플로 실행이 없습니다."),
        ActionsOverviewMessage::NoRunsYet => text.literal("아직 실행이 없습니다"),
        ActionsOverviewMessage::DispatchEmptyDescription => text.literal("이 워크플로는 직접 시작할 때 실행됩니다. 위의 워크플로 실행을 사용하세요."),
        ActionsOverviewMessage::NoWorkflowRunsYet => text.literal("아직 워크플로 실행이 없습니다"),
        ActionsOverviewMessage::WorkflowEmptyDescription => text.literal(".rbit/workflows 아래에 워크플로 파일을 추가하고 푸시하세요. 러너가 실행을 가져가면 여기에 표시됩니다."),
        ActionsOverviewMessage::Pagination => text.literal("페이지 매기기"),
        ActionsOverviewMessage::OlderRuns => text.literal("이전 실행"),
        ActionsOverviewMessage::RunsOnPage { count } => {
            text.literal("이 페이지의 실행 ");
            text.number(count);
            text.literal("개");
        }
    }
}
