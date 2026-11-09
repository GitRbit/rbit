//! Korean (`ko`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};

entries! { Ko:
    OrgMessage => MessageWriter |text| {
        OrgMessage::ChoosePermission => text.literal("권한을 선택하세요."),
        OrgMessage::Organization => text.literal("조직"),
        OrgMessage::Repositories => text.literal("저장소"),
        OrgMessage::Members => text.literal("구성원"),
        OrgMessage::Teams => text.literal("팀"),
        OrgMessage::Settings => text.literal("설정"),
        OrgMessage::NewOrganization => text.literal("새 조직"),
        OrgMessage::NewIntro => text.literal("조직은 팀을 대신해 저장소를 소유합니다. 내가 첫 번째 소유자가 됩니다."),
        OrgMessage::Name => text.literal("이름"),
        OrgMessage::NameHint => text.literal("URL에 사용됩니다. 영문자, 숫자, “-”, “_”, “.”을 사용할 수 있습니다. 사용자와 조직은 하나의 이름 공간을 공유합니다."),
        OrgMessage::DisplayName => text.literal("표시 이름"),
        OrgMessage::Optional => text.literal("선택 사항"),
        OrgMessage::Description => text.literal("설명"),
        OrgMessage::Cancel => text.literal("취소"),
        OrgMessage::CreateOrganization => text.literal("조직 만들기"),
        OrgMessage::New => text.literal("새로 만들기"),
        OrgMessage::NoRepositories => text.literal("아직 저장소가 없습니다"),
        OrgMessage::NoRepositoriesText => text.literal("이 조직이 소유한 저장소가 여기에 표시됩니다."),
        OrgMessage::Profile => text.literal("프로필"),
        OrgMessage::Website => text.literal("웹사이트"),
        OrgMessage::WebsitePlaceholder => text.literal("https://"),
        OrgMessage::SaveProfile => text.literal("프로필 저장"),
        OrgMessage::MemberPermission => text.literal("구성원 권한"),
        OrgMessage::PermissionLegend => text.literal("모든 구성원이 모든 저장소에 대해 갖는 권한"),
        OrgMessage::PermissionHint => text.literal("팀과 공동 작업자 설정으로 권한을 더할 수는 있지만 뺄 수는 없습니다. 소유자는 항상 모든 저장소를 관리합니다."),
        OrgMessage::SavePermission => text.literal("권한 저장"),
        OrgMessage::NoPermission => text.literal("권한 없음"),
        OrgMessage::NoPermissionText => text.literal("구성원은 공개 저장소와 소속 팀이 액세스할 수 있는 저장소만 볼 수 있습니다."),
        OrgMessage::Read => text.literal("읽기"),
        OrgMessage::ReadText => text.literal("구성원이 모든 저장소를 탐색하고 클론할 수 있습니다."),
        OrgMessage::Write => text.literal("쓰기"),
        OrgMessage::WriteText => text.literal("구성원이 모든 저장소를 탐색하고 클론하고 푸시할 수 있습니다."),
        OrgMessage::DangerZone => text.literal("위험 구역"),
        OrgMessage::DeleteOrganization => text.literal("이 조직 삭제"),
        OrgMessage::DeleteText => text.literal("조직과 모든 구성원 정보를 영구적으로 삭제합니다. 되돌릴 수 없습니다."),
        OrgMessage::TypeToConfirmLead => text.literal("확인하려면"),
        OrgMessage::TypeToConfirmTail => text.literal("을(를) 입력하세요"),
        OrgMessage::DeleteOrganizationButton => text.literal("조직 삭제"),
        OrgMessage::AddMember => text.literal("구성원 추가"),
        OrgMessage::Username => text.literal("사용자 이름"),
        OrgMessage::Role => text.literal("역할"),
        OrgMessage::Member => text.literal("구성원"),
        OrgMessage::Owner => text.literal("소유자"),
        OrgMessage::Add => text.literal("추가"),
        OrgMessage::MembersHint => text.literal("구성원은 모든 저장소에 대해 조직의 기본 권한과 소속 팀이 부여한 권한을 갖습니다. 소유자는 구성원, 팀, 설정도 관리할 수 있습니다."),
        OrgMessage::You => text.literal("(나)"),
        OrgMessage::Leave => text.literal("나가기"),
        OrgMessage::Remove => text.literal("제거"),
        OrgMessage::MakeOwner => text.literal("소유자로 지정"),
        OrgMessage::MakeMember => text.literal("구성원으로 지정"),
    }
    OrgText<'a> => MessageWriter |text| {
        OrgText::StillOwnsOne(org) => {
            text.argument(org);
            text.literal(" 조직이 아직 저장소 1개를 소유하고 있습니다. 먼저 저장소를 삭제하거나 이전하세요.");
        }
        OrgText::StillOwns(org, count) => {
            text.argument(org);
            text.literal(" 조직이 아직 저장소 ");
            text.number(count);
            text.literal("개를 소유하고 있습니다. 먼저 저장소를 삭제하거나 이전하세요.");
        }
        OrgText::RemoveLabel(name) => {
            text.argument(name);
            text.literal(" 제거");
        }
        OrgText::RoleLabel(action, name) => {
            text.argument(action);
            text.literal(": ");
            text.argument(name);
        }
        OrgText::MembersTitle(org) => {
            text.literal("구성원 · ");
            text.argument(org);
        }
        OrgText::SettingsTitle(org) => {
            text.literal("설정 · ");
            text.argument(org);
        }
    }
}
