//! Korean (`ko`) entries for `catalog/actions_ui/environments.rs`.

use crate::catalog::actions_ui::environments::ActionsEnvironmentMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ActionsEnvironmentMessage<'a> => MessageWriter |text| {
        ActionsEnvironmentMessage::Title => text.literal("환경"),
        ActionsEnvironmentMessage::Intro { key, .. } => {
            text.argument(key);
            text.literal("(으)로 환경을 지정한 작업은 그 환경의 시크릿과 변수를 받고, 리뷰어가 있으면 그중 한 명을 기다리며, 허용된 브랜치에서만 실행됩니다. 워크플로가 환경을 처음 지정하면 여기에 표시됩니다.");
        }
        ActionsEnvironmentMessage::NoEnvironments => text.literal("환경 없음"),
        ActionsEnvironmentMessage::NoEnvironmentsHint { key, .. } => {
            text.literal("아래에서 추가하거나, ");
            text.argument(key);
            text.literal("(으)로 환경을 지정한 작업이 있는 워크플로를 실행하세요.");
        }
        ActionsEnvironmentMessage::NewEnvironment => text.literal("새 환경"),
        ActionsEnvironmentMessage::Name => text.literal("이름"),
        ActionsEnvironmentMessage::NamePlaceholder => text.literal("production"),
        ActionsEnvironmentMessage::AddEnvironment => text.literal("환경 추가"),
        ActionsEnvironmentMessage::Reviewers { count, .. } => {
            text.literal("리뷰어 ");
            text.number(count);
            text.literal("명");
        }
        ActionsEnvironmentMessage::ProtectedBranches => text.literal("보호된 브랜치"),
        ActionsEnvironmentMessage::Branches { patterns, .. } => {
            text.literal("브랜치 ");
            text.argument(patterns);
        }
        ActionsEnvironmentMessage::NoProtection => text.literal("보호 없음"),
        ActionsEnvironmentMessage::ProtectionRules => text.literal("보호 규칙"),
        ActionsEnvironmentMessage::RequiredReviewers => text.literal("필수 리뷰어"),
        ActionsEnvironmentMessage::ReviewersPlaceholder => text.literal("한 줄에 사용자 이름 하나"),
        ActionsEnvironmentMessage::ReviewersHint => text.literal("이 저장소를 읽을 수 있는 사람을 최대 6명까지 지정할 수 있습니다. 이 환경의 작업은 그중 한 명이 승인할 때까지 기다리며, 거부된 작업은 실패합니다. 지정하지 않으면 작업이 리뷰 없이 실행됩니다."),
        ActionsEnvironmentMessage::PreventSelfReview => text.literal("자체 리뷰 방지"),
        ActionsEnvironmentMessage::PreventSelfReviewHint => text.literal("실행을 시작한 사람은 그 배포를 승인할 수 없습니다."),
        ActionsEnvironmentMessage::WaitTimer => text.literal("대기 타이머"),
        ActionsEnvironmentMessage::WaitTimerHint => text.literal("이 환경의 작업이 준비되고 승인된 후 실행되기 전까지 기다리는 시간(분)입니다. 기다리지 않으려면 0, 최대 43200(30일)입니다."),
        ActionsEnvironmentMessage::DeploymentBranches => text.literal("배포 브랜치 및 태그"),
        ActionsEnvironmentMessage::AnyBranch => text.literal("모든 브랜치 또는 태그"),
        ActionsEnvironmentMessage::ProtectedOnly => text.literal("보호된 브랜치만"),
        ActionsEnvironmentMessage::ProtectedOnlyHint => text.literal("브랜치 보호 규칙이 있는 브랜치입니다."),
        ActionsEnvironmentMessage::SelectedBranches => text.literal("선택한 브랜치 및 태그"),
        ActionsEnvironmentMessage::PatternsLabel => text.literal("브랜치 및 태그 패턴"),
        ActionsEnvironmentMessage::PatternsPlaceholder => text.literal("main"),
        ActionsEnvironmentMessage::PatternsHint { branch, tags, tag, exclude, .. } => {
            text.literal("워크플로 필터처럼 한 줄에 패턴 하나를 입력합니다. 브랜치는 이름으로(");
            text.argument(branch);
            text.literal("), 태그는 ");
            text.argument(tags);
            text.literal(" 형식으로(");
            text.argument(tag);
            text.literal(") 지정하며, ");
            text.argument(exclude);
            text.literal("은(는) 제외합니다.");
        }
        ActionsEnvironmentMessage::SaveRules => text.literal("보호 규칙 저장"),
        ActionsEnvironmentMessage::DeleteTitle => text.literal("이 환경 삭제"),
        ActionsEnvironmentMessage::DeleteHint => text.literal("시크릿, 변수, 규칙도 함께 삭제됩니다. 이 환경의 리뷰어를 기다리던 작업은 즉시 실행됩니다."),
        ActionsEnvironmentMessage::DeleteEnvironment => text.literal("환경 삭제"),
    }
}
