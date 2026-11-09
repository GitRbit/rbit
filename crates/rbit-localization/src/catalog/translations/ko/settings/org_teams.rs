//! Korean (`ko`) entries for `catalog/settings/org_teams.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org_teams::TeamSentence;
use crate::catalog::settings::org_teams::{TeamMessage, TeamText};

entries! { Ko:
    TeamMessage => MessageWriter |text| {
        TeamMessage::NewTeam => text.literal("새 팀"),
        TeamMessage::NoTeams => text.literal("아직 팀이 없습니다"),
        TeamMessage::NoTeamsText => text.literal("팀은 구성원을 묶고 각 그룹에 저장소에 대한 별도의 액세스 권한을 부여합니다."),
        TeamMessage::OneMember => text.literal("구성원 1명"),
        TeamMessage::OneRepository => text.literal("저장소 1개"),
        TeamMessage::CreateTeam => text.literal("팀 만들기"),
        TeamMessage::TeamSettings => text.literal("팀 설정"),
        TeamMessage::RenameHint => text.literal("이름을 바꾸면 팀 주소가 바뀝니다."),
        TeamMessage::SaveChanges => text.literal("변경 사항 저장"),
        TeamMessage::DeleteTeam => text.literal("이 팀 삭제"),
        TeamMessage::DeleteTeamText => text.literal("구성원은 조직에 남지만 이 팀이 부여한 액세스 권한을 잃습니다."),
        TeamMessage::DeleteTeamButton => text.literal("팀 삭제"),
        TeamMessage::UsernamePlaceholder => text.literal("사용자 이름"),
        TeamMessage::OnlyMembers => text.literal("조직의 구성원만 조직의 팀에 참여할 수 있습니다."),
        TeamMessage::NoMembers => text.literal("구성원 없음"),
        TeamMessage::NoMembersText => text.literal("구성원은 이 팀이 가진 저장소 액세스 권한을 얻습니다."),
        TeamMessage::AddRepository => text.literal("저장소 추가"),
        TeamMessage::RepositoryPlaceholder => text.literal("저장소 이름"),
        TeamMessage::Permission => text.literal("권한"),
        TeamMessage::RepositoryHint => text.literal("조직의 모든 저장소를 추가할 수 있습니다. 팀 구성원은 해당 저장소에 이 권한을 갖습니다."),
        TeamMessage::NoRepositories => text.literal("저장소 없음"),
        TeamMessage::NoRepositoriesText => text.literal("이 팀에는 볼 수 있는 저장소에 대한 액세스 권한이 부여되지 않았습니다."),
        TeamMessage::Private => text.literal("비공개"),
        TeamMessage::Save => text.literal("저장"),
    }
    TeamText<'a> => MessageWriter |text| {
        TeamText::Members(count) => {
            text.literal("구성원 ");
            text.number(count);
            text.literal("명");
        }
        TeamText::Repositories(count) => {
            text.literal("저장소 ");
            text.number(count);
            text.literal("개");
        }
        TeamText::PermissionOn(repository) => {
            text.argument(repository);
            text.literal("에 대한 권한");
        }
        TeamText::SavePermissionOn(repository) => {
            text.argument(repository);
            text.literal("에 대한 권한 저장");
        }
        TeamText::TeamsTitle(org) => {
            text.literal("팀 · ");
            text.argument(org);
        }
        TeamText::NewTeamTitle(org) => {
            text.literal("새 팀 · ");
            text.argument(org);
        }
        TeamText::SettingsTitle(team) => {
            text.literal("설정 · ");
            text.argument(team);
        }
    }
    TeamSentence => RichWriter |text| {
        TeamSentence::NameHint => {
            text.literal("영문자, 숫자, ");
            text.slot(0);
            text.literal(", ");
            text.slot(1);
            text.literal(" 및 ");
            text.slot(2);
            text.literal("을(를) 사용할 수 있으며, 팀 주소에 사용됩니다.");
        }
    }
}
