//! Korean (`ko`) entries for `catalog/settings/cards.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::cards::CardMessage;

entries! { Ko:
    CardMessage => MessageWriter |text| {
        CardMessage::Actions => text.literal("Actions"),
        CardMessage::RepoActions => text.literal("푸시와 풀 리퀘스트에서 워크플로를 실행하고, 워크플로가 사용하는 시크릿과 변수를 관리합니다."),
        CardMessage::OrgActions => text.literal("이 조직의 모든 저장소 워크플로에서 사용하는 시크릿과 변수입니다."),
        CardMessage::Collaborators => text.literal("공동 작업자"),
        CardMessage::CollaboratorsText => text.literal("이 저장소에 액세스할 수 있는 사람과 팀, 그리고 각각의 권한입니다."),
        CardMessage::Mirror => text.literal("미러"),
        CardMessage::MirrorText => text.literal("업스트림, 가져오기 간격, 마지막 가져오기 결과입니다."),
        CardMessage::Protection => text.literal("브랜치 보호"),
        CardMessage::ProtectionText => text.literal("중요한 브랜치가 강제 푸시되거나 삭제되거나 풀 리퀘스트 없이 푸시되지 않도록 보호합니다."),
        CardMessage::PushMirrors => text.literal("푸시 미러"),
        CardMessage::PushMirrorsText => text.literal("푸시할 때마다 이 저장소의 브랜치와 태그를 받는 원격 저장소입니다."),
        CardMessage::DeployKeys => text.literal("배포 키"),
        CardMessage::DeployKeysText => text.literal("서버나 CI 작업이 이 저장소에만 액세스할 수 있게 하는 SSH 키입니다."),
        CardMessage::Teams => text.literal("팀"),
        CardMessage::TeamsText => text.literal("구성원을 그룹으로 묶고 그룹마다 저장소 액세스 권한을 부여합니다."),
        CardMessage::Transfer => text.literal("소유권 이전"),
        CardMessage::TransferText => text.literal("이 저장소를 다른 사용자나 조직으로 옮깁니다. URL이 바뀝니다."),
        CardMessage::Webhooks => text.literal("웹훅"),
        CardMessage::RepoWebhooks => text.literal("코드가 푸시되거나 릴리스가 게시되는 등의 이벤트가 발생하면 다른 서비스에 알립니다."),
        CardMessage::OrgWebhooks => text.literal("이 조직의 모든 저장소에서 발생하는 이벤트를 다른 서비스에 알립니다."),
    }
}
