//! Korean (`ko`) entries for `catalog/actions_ui/runners.rs`.

use crate::catalog::actions_ui::runners::ActionsRunnersMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ActionsRunnersMessage<'a> => MessageWriter |text| {
        ActionsRunnersMessage::Title => text.literal("러너"),
        ActionsRunnersMessage::AdminIntro { setting, .. } => {
            text.literal("인스턴스 러너는 관리자가 허용한 소유자의 작업을 가져오며, ");
            text.argument(setting);
            text.literal("을(를) 설정하면 모든 소유자의 작업을 가져옵니다.");
        }
        ActionsRunnersMessage::TokenCreated => text.literal("등록 토큰이 생성되었습니다"),
        ActionsRunnersMessage::TokenCreatedHint { command, .. } => {
            text.literal("지금 복사하세요. 다시 표시되지 않습니다. 작업을 실행할 머신에서 아래 명령을 실행하고 입력을 기다리면 토큰을 붙여 넣은 다음, ");
            text.argument(command);
            text.literal(" 명령으로 러너를 시작하세요.");
        }
        ActionsRunnersMessage::Token => text.literal("토큰"),
        ActionsRunnersMessage::CopyToken => text.literal("토큰 복사"),
        ActionsRunnersMessage::Copy => text.literal("복사"),
        ActionsRunnersMessage::Command => text.literal("명령"),
        ActionsRunnersMessage::CopyCommand => text.literal("명령 복사"),
        ActionsRunnersMessage::LabelsHint { container, host, .. } => {
            text.literal("레이블에 따라 작업 실행 방식이 정해집니다. ");
            text.argument(container);
            text.literal("은(는) Docker나 Podman에서 실행되고, ");
            text.argument(host);
            text.literal("은(는) 격리 없이 머신에서 직접 실행되므로 신뢰할 수 있는 저장소에만 사용하세요.");
        }
        ActionsRunnersMessage::RepositoryIntro { program, .. } => {
            text.argument(program);
            text.literal("을(를) 실행하며 이 저장소의 작업을 가져오는 머신입니다.");
        }
        ActionsRunnersMessage::OwnerIntro { program, owner, .. } => {
            text.argument(program);
            text.literal("을(를) 실행하며 ");
            text.argument(owner);
            text.literal("의 작업을 가져오는 머신입니다.");
        }
        ActionsRunnersMessage::AccountIntro { program, .. } => {
            text.argument(program);
            text.literal("을(를) 실행하며 내 저장소의 작업을 가져오는 머신입니다.");
        }
        ActionsRunnersMessage::InstanceIntro { program, .. } => {
            text.argument(program);
            text.literal("을(를) 실행하며 인스턴스 러너가 허용된 모든 소유자의 작업을 가져오는 머신입니다.");
        }
        ActionsRunnersMessage::NoRunners => text.literal("아직 러너가 없습니다"),
        ActionsRunnersMessage::NoRunnersHint => text.literal("아래에서 등록 토큰을 만들고 그 토큰으로 러너를 등록하세요. 작업은 레이블이 일치하는 러너가 온라인이 될 때까지 기다립니다."),
        ActionsRunnersMessage::Online => text.literal("온라인"),
        ActionsRunnersMessage::Offline => text.literal("오프라인"),
        ActionsRunnersMessage::Disabled => text.literal("비활성화됨"),
        ActionsRunnersMessage::NeverSeen => text.literal("연결된 적 없음"),
        ActionsRunnersMessage::OwnerScope => text.literal("소유자"),
        ActionsRunnersMessage::InstanceScope => text.literal("인스턴스"),
        ActionsRunnersMessage::Ephemeral => text.literal("일회용"),
        ActionsRunnersMessage::HostNoIsolation => text.literal("호스트 — 격리 없음"),
        ActionsRunnersMessage::HostWarning => text.literal("호스트 레이블의 작업은 러너의 사용자 권한으로 격리 없이 실행됩니다"),
        ActionsRunnersMessage::Capacity { count, .. } => {
            text.literal("동시 ");
            text.number(count);
            text.literal("개");
        }
        ActionsRunnersMessage::Seen { when, .. } => {
            text.literal("마지막 연결: ");
            text.argument(when);
        }
        ActionsRunnersMessage::Enable => text.literal("활성화"),
        ActionsRunnersMessage::Disable => text.literal("비활성화"),
        ActionsRunnersMessage::Remove => text.literal("삭제"),
        ActionsRunnersMessage::RemoveRunner { name, .. } => {
            text.argument(name);
            text.literal(" 삭제");
        }
        ActionsRunnersMessage::TokensTitle => text.literal("등록 토큰"),
        ActionsRunnersMessage::TokensIntro => text.literal("러너는 등록 토큰을 한 번 사용하여 자체 자격 증명을 받습니다. 따로 선택하지 않으면 토큰은 일회용이며 만료됩니다."),
        ActionsRunnersMessage::ExpiresAfter => text.literal("만료 기간"),
        ActionsRunnersMessage::OnlyTheseLabels => text.literal("다음 레이블만 허용"),
        ActionsRunnersMessage::LabelsPlaceholder => text.literal("ubuntu-latest, arm64"),
        ActionsRunnersMessage::Optional => text.literal("(선택 사항)"),
        ActionsRunnersMessage::Reusable => text.literal("만료될 때까지 이 토큰으로 러너를 몇 개든 등록할 수 있도록 허용"),
        ActionsRunnersMessage::NewToken => text.literal("새 등록 토큰"),
        ActionsRunnersMessage::Created { when, .. } => {
            text.literal("생성: ");
            text.argument(when);
        }
        ActionsRunnersMessage::Expires { when, .. } => {
            text.literal("만료: ");
            text.argument(when);
        }
        ActionsRunnersMessage::NoLongerUsable => text.literal("더 이상 사용할 수 없음"),
        ActionsRunnersMessage::Revoked => text.literal("취소됨"),
        ActionsRunnersMessage::Expired => text.literal("만료됨"),
        ActionsRunnersMessage::UnlimitedUses => text.literal("무제한 사용"),
        ActionsRunnersMessage::UsedUp => text.literal("모두 사용됨"),
        ActionsRunnersMessage::UsesLeft { count, .. } => {
            text.number(count);
            text.literal("회 남음");
        }
        ActionsRunnersMessage::Revoke => text.literal("취소"),
        ActionsRunnersMessage::Hours { count, .. } => {
            text.number(count);
            text.literal("시간");
        }
        ActionsRunnersMessage::Days { count, .. } => {
            text.number(count);
            text.literal("일");
        }
    }
}
