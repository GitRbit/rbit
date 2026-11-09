//! Korean (`ko`) entries for `catalog/actions_ui/settings.rs`.

use crate::catalog::actions_ui::settings::ActionsSettingsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ActionsSettingsMessage<'a> => MessageWriter |text| {
        ActionsSettingsMessage::RepositoryIntro { directory, repository, .. } => {
            text.argument(directory);
            text.literal(" 아래의 워크플로는 ");
            text.argument(repository);
            text.literal("에서 이벤트가 발생하면 러너에서 실행됩니다.");
        }
        ActionsSettingsMessage::OrganizationIntro { organization, .. } => {
            text.argument(organization);
            text.literal(" 저장소의 워크플로에 사용할 시크릿과 변수입니다. 저장소에 같은 이름의 시크릿이나 변수가 있으면 저장소의 값이 우선합니다.");
        }
        ActionsSettingsMessage::AccountIntro => text.literal("내 저장소의 워크플로에 사용할 시크릿과 변수입니다. 저장소에 같은 이름의 시크릿이나 변수가 있으면 저장소의 값이 우선합니다."),
        ActionsSettingsMessage::General => text.literal("일반"),
        ActionsSettingsMessage::RunWorkflows => text.literal("이 저장소에서 워크플로 실행"),
        ActionsSettingsMessage::RunWorkflowsHint => text.literal("끄면 새 실행이 시작되지 않고 Actions 탭이 숨겨집니다. 이전 실행과 로그는 유지됩니다."),
        ActionsSettingsMessage::DefaultPermission => text.literal("기본 워크플로 토큰 권한"),
        ActionsSettingsMessage::PermissionHint { key, .. } => {
            text.literal("워크플로의 ");
            text.argument(key);
            text.literal(" 설정으로는 권한을 낮추기만 할 수 있습니다. 포크와 외부 작성자의 실행은 항상 읽기 전용 토큰을 받습니다.");
        }
        ActionsSettingsMessage::PermissionRead => text.literal("읽기"),
        ActionsSettingsMessage::PermissionReadHint => text.literal("워크플로가 저장소의 콘텐츠, 이슈, 풀 리퀘스트를 읽을 수 있습니다."),
        ActionsSettingsMessage::PermissionWrite => text.literal("읽기 및 쓰기"),
        ActionsSettingsMessage::PermissionWriteHint => text.literal("워크플로가 푸시하고 이슈와 풀 리퀘스트에 댓글을 달 수도 있습니다. 브랜치 보호는 계속 적용됩니다."),
        ActionsSettingsMessage::SaveChanges => text.literal("변경 사항 저장"),
        ActionsSettingsMessage::RepositoryRunnersHint => text.literal("이 저장소의 러너, 상속받은 러너, 등록 토큰입니다."),
        ActionsSettingsMessage::OrganizationRunnersHint => text.literal("조직의 모든 저장소에서 사용하는 러너와 등록 토큰입니다."),
        ActionsSettingsMessage::AccountRunnersHint => text.literal("내 계정의 모든 저장소에서 사용하는 러너와 등록 토큰입니다."),
        ActionsSettingsMessage::EnvironmentsHint => text.literal("배포 환경: 필수 리뷰어, 허용된 브랜치, 환경 고유의 시크릿과 변수입니다."),
        ActionsSettingsMessage::TrustedTitle => text.literal("신뢰할 수 있는 풀 리퀘스트 작성자"),
        ActionsSettingsMessage::TrustedIntro => text.literal("이 작성자들이 포크에서 연 풀 리퀘스트는 승인을 기다리지 않고 실행되지만, 여전히 읽기 전용 토큰을 사용하며 시크릿과 변수는 받지 않습니다. 작성자는 실행 승인 시 “이 작성자의 풀 리퀘스트를 항상 실행”을 선택하면 추가됩니다."),
        ActionsSettingsMessage::NoTrustedAuthors => text.literal("신뢰할 수 있는 작성자 없음"),
        ActionsSettingsMessage::NoTrustedAuthorsHint => text.literal("포크에서 온 풀 리퀘스트와 쓰기 권한이 없는 사람의 풀 리퀘스트는 실행할 때마다 쓰기 권한이 있는 사람의 승인을 기다립니다."),
        ActionsSettingsMessage::TrustedSince { since, .. } => {
            text.literal("신뢰 시작: ");
            text.argument(since);
        }
        ActionsSettingsMessage::TrustedSinceBy { since, approver, .. } => {
            text.literal("신뢰 시작: ");
            text.argument(since);
            text.literal(", 승인자: ");
            text.argument(approver);
        }
        ActionsSettingsMessage::StopTrusting { name, .. } => {
            text.argument(name);
            text.literal(" 신뢰 해제");
        }
        ActionsSettingsMessage::Remove => text.literal("제거"),
        ActionsSettingsMessage::Saved => text.literal("설정을 저장했습니다."),
        ActionsSettingsMessage::SecretSaved => text.literal("시크릿을 저장했습니다."),
        ActionsSettingsMessage::SecretDeleted => text.literal("시크릿을 삭제했습니다."),
        ActionsSettingsMessage::VariableSaved => text.literal("변수를 저장했습니다."),
        ActionsSettingsMessage::VariableDeleted => text.literal("변수를 삭제했습니다."),
        ActionsSettingsMessage::Untrusted => text.literal("이 작성자의 풀 리퀘스트는 다시 승인이 필요합니다."),
        ActionsSettingsMessage::RepositoryAccess => text.literal("저장소 액세스"),
        ActionsSettingsMessage::AllRepositories => text.literal("모든 저장소"),
        ActionsSettingsMessage::AllRepositoriesHint => text.literal("나중에 만드는 저장소를 포함하여 이 소유자의 모든 저장소입니다."),
        ActionsSettingsMessage::SelectedRepositories => text.literal("선택한 저장소"),
        ActionsSettingsMessage::SelectedRepositoriesHint => text.literal("아래에서 선택한 저장소만 해당합니다."),
        ActionsSettingsMessage::NoRepositoriesYet => text.literal("이 소유자에게는 아직 저장소가 없습니다."),
    }
}
