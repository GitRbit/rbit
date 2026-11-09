//! Korean (`ko`) entries for `catalog/settings/protection.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::protection::{ProtectionMessage, ProtectionText, RuleBadge};

entries! { Ko:
    ProtectionMessage => MessageWriter |text| {
        ProtectionMessage::TickToConfirm => text.literal("확인하려면 체크 상자를 선택하세요."),
        ProtectionMessage::Settings => text.literal("설정"),
        ProtectionMessage::Title => text.literal("브랜치 보호"),
        ProtectionMessage::IntroLead => text.literal("규칙은"),
        ProtectionMessage::IntroTail => text.literal("에 대한 모든 푸시(HTTP 및 SSH)에 적용되며, 규칙에서 우회를 허용하지 않는 한 관리자에게도 적용됩니다."),
        ProtectionMessage::AddRule => text.literal("규칙 추가"),
        ProtectionMessage::Deleted => text.literal("규칙을 삭제했습니다."),
        ProtectionMessage::Empty => text.literal("보호된 브랜치 없음"),
        ProtectionMessage::EmptyText => text.literal("main 같은 브랜치가 강제 푸시되거나 삭제되거나 풀 리퀘스트 없이 변경되지 않도록 규칙을 추가하세요."),
        ProtectionMessage::NoMatchYet => text.literal("아직 일치하는 브랜치가 없습니다."),
        ProtectionMessage::MatchesLabel => text.literal("일치:"),
        ProtectionMessage::NewRule => text.literal("새 브랜치 보호 규칙"),
        ProtectionMessage::NewRuleIntro => text.literal("이 규칙은 패턴과 일치하는 모든 브랜치에 현재와 이후 모두 적용됩니다."),
        ProtectionMessage::Saved => text.literal("규칙을 저장했습니다."),
        ProtectionMessage::Rule => text.literal("규칙"),
        ProtectionMessage::Pattern => text.literal("브랜치 이름 패턴"),
        ProtectionMessage::PatternPlaceholder => text.literal("main"),
        ProtectionMessage::PatternLead => text.literal("브랜치 이름 또는 패턴:"),
        ProtectionMessage::PatternOne => text.literal("= 이름의 한 부분 안에서 일치,"),
        ProtectionMessage::PatternAcross => text.literal("= 여러 부분에 걸쳐 일치("),
        ProtectionMessage::PatternChar => text.literal("),\x20"),
        ProtectionMessage::PatternTail => text.literal("= 한 글자. 대소문자를 구분합니다."),
        ProtectionMessage::Protect => text.literal("일치하는 브랜치 보호"),
        ProtectionMessage::BlockForcePush => text.literal("강제 푸시 차단"),
        ProtectionMessage::BlockForcePushText => text.literal("푸시는 커밋을 추가만 할 수 있으며 기록을 다시 쓸 수 없습니다."),
        ProtectionMessage::BlockDeletion => text.literal("삭제 차단"),
        ProtectionMessage::BlockDeletionText => text.literal("일치하는 브랜치를 삭제할 수 없습니다."),
        ProtectionMessage::RequirePullRequest => text.literal("풀 리퀘스트 필수"),
        ProtectionMessage::RequirePullRequestText => text.literal("변경 사항은 풀 리퀘스트 병합으로만 반영되며, 기존 브랜치로의 직접 푸시는 거부됩니다."),
        ProtectionMessage::RequiredApprovals => text.literal("필수 승인 수"),
        ProtectionMessage::DismissStale => text.literal("새 커밋이 푸시되면 승인 기각"),
        ProtectionMessage::BlockOnChanges => text.literal("변경 요청이 있는 동안 병합 차단"),
        ProtectionMessage::RestrictPushers => text.literal("푸시할 수 있는 사람 제한"),
        ProtectionMessage::RestrictPeople => text.literal("아래 사람만 푸시하거나 병합할 수 있습니다. 배포 키는 항상 할 수 없습니다."),
        ProtectionMessage::RestrictPeopleTeams => text.literal("아래 사람과 팀만 푸시하거나 병합할 수 있습니다. 배포 키는 항상 할 수 없습니다."),
        ProtectionMessage::People => text.literal("사람"),
        ProtectionMessage::PeoplePlaceholder => text.literal("alice bob"),
        ProtectionMessage::PeopleHint => text.literal("공백이나 쉼표로 구분한 사용자 이름입니다. 각 사용자에게 쓰기 권한이 있어야 합니다."),
        ProtectionMessage::Teams => text.literal("팀"),
        ProtectionMessage::NoTeams => text.literal("이 조직에는 아직 팀이 없습니다."),
        ProtectionMessage::RequireChecks => text.literal("상태 검사 통과 필수"),
        ProtectionMessage::RequireChecksText => text.literal("아래 모든 검사의 최신 상태가 헤드 커밋에서 성공일 때만 풀 리퀘스트를 병합할 수 있습니다."),
        ProtectionMessage::RequiredChecks => text.literal("필수 검사"),
        ProtectionMessage::ChecksPlaceholder => text.literal("ci/build"),
        ProtectionMessage::AdminBypass => text.literal("관리자 우회 허용"),
        ProtectionMessage::AdminBypassText => text.literal("저장소 관리자에게는 이 규칙이 적용되지 않습니다. 모든 우회는 감사 로그에 기록됩니다."),
        ProtectionMessage::SaveRule => text.literal("규칙 저장"),
        ProtectionMessage::DangerZone => text.literal("위험 구역"),
        ProtectionMessage::DeleteTitle => text.literal("이 규칙 삭제"),
        ProtectionMessage::DeleteLead => text.literal("삭제하면"),
        ProtectionMessage::DeleteTail => text.literal("패턴과 일치하는 브랜치의 보호가 즉시 해제됩니다."),
        ProtectionMessage::DeleteConfirm => text.literal("이 규칙이 삭제된다는 것을 이해했습니다"),
        ProtectionMessage::DeleteRule => text.literal("규칙 삭제"),
        ProtectionMessage::NoMatches => text.literal("아직 일치하는 브랜치 없음"),
    }
    ProtectionText<'a> => MessageWriter |text| {
        ProtectionText::MaxRules(max) => {
            text.literal("이 저장소는 최대 규칙 수(");
            text.number(max);
            text.literal("개)에 도달했습니다. 다른 규칙을 추가하려면 하나를 삭제하세요.");
        }
        ProtectionText::ChecksHint(max) => {
            text.literal("한 줄에 컨텍스트 하나씩, CI가 보고하는 그대로 입력하세요(대소문자 구분). 최대 ");
            text.number(max);
            text.literal("개.");
        }
        ProtectionText::Matches(names) => {
            text.literal("일치: ");
            text.argument(names);
        }
        ProtectionText::MatchesMore(names, more) => {
            text.literal("일치: ");
            text.argument(names);
            text.literal(" 외 ");
            text.number(more);
            text.literal("개");
        }
        ProtectionText::ListTitle(repository) => {
            text.literal("브랜치 보호 · ");
            text.argument(repository);
        }
        ProtectionText::NewTitle(repository) => {
            text.literal("새 브랜치 보호 규칙 · ");
            text.argument(repository);
        }
        ProtectionText::EditTitle(pattern, repository) => {
            text.argument(pattern);
            text.literal(" · 브랜치 보호 · ");
            text.argument(repository);
        }
    }
    RuleBadge => MessageWriter |text| {
        RuleBadge::NoForcePushes => text.literal("강제 푸시 금지"),
        RuleBadge::NoDeletion => text.literal("삭제 금지"),
        RuleBadge::PullRequest => text.literal("풀 리퀘스트"),
        RuleBadge::Approvals(count) => {
            text.literal("승인 ");
            text.number(count);
            text.literal("개");
        }
        RuleBadge::StatusChecks => text.literal("상태 검사"),
        RuleBadge::Restricted => text.literal("제한됨"),
        RuleBadge::AdminsBypass => text.literal("관리자 우회"),
    }
}
