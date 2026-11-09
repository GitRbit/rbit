//! Korean (`ko`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { Ko:
    AccessMessage => MessageWriter |text| {
        AccessMessage::Settings => text.literal("설정"),
        AccessMessage::Collaborators => text.literal("공동 작업자"),
        AccessMessage::IntroLead => text.literal("여기에 추가한 사람은 "),
        AccessMessage::IntroDirectly => text.literal(" 저장소에 직접 액세스할 수 있습니다"),
        AccessMessage::IntroOrganization => text.literal("(조직 소속 여부와 관계없음)"),
        AccessMessage::IntroTail => text.literal("."),
        AccessMessage::AddCollaborator => text.literal("공동 작업자 추가"),
        AccessMessage::Username => text.literal("사용자 이름"),
        AccessMessage::Permission => text.literal("권한"),
        AccessMessage::Add => text.literal("추가"),
        AccessMessage::People => text.literal("사람"),
        AccessMessage::NoCollaborators => text.literal("공동 작업자 없음"),
        AccessMessage::NoCollaboratorsText => text.literal("사용자 이름으로 사람을 추가하여 이 저장소에 대한 액세스 권한을 부여하세요."),
        AccessMessage::You => text.literal("(나)"),
        AccessMessage::Save => text.literal("저장"),
        AccessMessage::Leave => text.literal("나가기"),
        AccessMessage::Remove => text.literal("제거"),
        AccessMessage::Teams => text.literal("팀"),
        AccessMessage::ManageTeams => text.literal("팀 관리"),
        AccessMessage::NoTeams => text.literal("팀 없음"),
        AccessMessage::NoTeamsText => text.literal("팀 페이지에서 팀에 액세스 권한을 부여하면 팀 구성원이 여기에서 그 권한을 갖게 됩니다."),
        AccessMessage::Transfer => text.literal("소유권 이전"),
        AccessMessage::TransferLead => text.literal("저장소"),
        AccessMessage::TransferTail => text.literal("소유권을 다른 계정으로 이전합니다. 기록, 공동 작업자, 스타는 함께 이동합니다. 이전 주소를 사용하는 링크와 클론 URL은 더 이상 작동하지 않으며, 현재 조직의 팀은 액세스 권한을 잃습니다."),
        AccessMessage::NewOwner => text.literal("새 소유자"),
        AccessMessage::AdminEntry => text.literal("사이트 관리자는 모든 사용자 또는 조직을 입력할 수 있습니다."),
        AccessMessage::Organization => text.literal("조직"),
        AccessMessage::OwnerHint => text.literal("내 계정 또는 내가 속한 조직."),
        AccessMessage::TypeLead => text.literal("확인하려면"),
        AccessMessage::TypeTail => text.literal("을(를) 입력하세요"),
        AccessMessage::TransferRepository => text.literal("저장소 이전"),
        AccessMessage::Nowhere => text.literal("이전할 대상 없음"),
        AccessMessage::NowhereText => text.literal("저장소를 만들 수 있는 조직에 속해 있지 않고, 이 저장소는 이미 내 소유입니다."),
    }
    AccessText<'a> => MessageWriter |text| {
        AccessText::PermissionFor(name) => {
            text.argument(name);
            text.literal(" 님의 권한");
        }
        AccessText::SavePermissionFor(name) => {
            text.argument(name);
            text.literal(" 님의 권한 저장");
        }
        AccessText::RemoveLabel(name) => {
            text.argument(name);
            text.literal(" 님 제거");
        }
        AccessText::Added(date) => {
            text.literal("추가됨: ");
            text.argument(date);
        }
        AccessText::CollaboratorsTitle(repository) => {
            text.literal("공동 작업자 · ");
            text.argument(repository);
        }
        AccessText::TransferTitle(repository) => {
            text.literal("이전 · ");
            text.argument(repository);
        }
    }
}
