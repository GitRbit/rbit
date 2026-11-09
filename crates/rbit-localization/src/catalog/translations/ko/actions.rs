//! Korean (`ko`) entries for `catalog/actions.rs`.

use crate::catalog::actions::ActionsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ActionsMessage<'a> => MessageWriter |text| {
        ActionsMessage::Actions => text.literal("Actions"),
        ActionsMessage::Workflow => text.literal("워크플로"),
        ActionsMessage::Commit => text.literal("커밋"),
        ActionsMessage::Duration => text.literal("소요 시간"),
        ActionsMessage::Attempt => text.literal("시도"),
        ActionsMessage::TriggeredBy => text.literal("트리거한 사용자"),
        ActionsMessage::Jobs => text.literal("작업"),
        ActionsMessage::Artifacts => text.literal("아티팩트"),
        ActionsMessage::RunDetails => text.literal("실행 세부 정보"),
        ActionsMessage::CancelRun => text.literal("실행 취소"),
        ActionsMessage::RerunFailed => text.literal("실패한 작업 다시 실행"),
        ActionsMessage::RerunAll => text.literal("모든 작업 다시 실행"),
        ActionsMessage::DownloadTar => text.literal("tar.gz로 다운로드"),
        ActionsMessage::Expires => text.literal("만료:"),
        ActionsMessage::RerunJob { name } => {
            text.argument(name);
            text.literal(" 다시 실행");
        }
        ActionsMessage::Summary { job } => {
            text.argument(job);
            text.literal(" 요약");
        }
    }
}
