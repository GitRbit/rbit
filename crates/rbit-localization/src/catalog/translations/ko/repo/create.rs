//! Korean (`ko`) entries for `catalog/repo/create.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::create::{RepoCreateLabel, RepoCreateMessage};

entries! { Ko:
    RepoCreateLabel => MessageWriter |text| {
        RepoCreateLabel::NewRepository => text.literal("새 저장소"),
        RepoCreateLabel::RepositoryType => text.literal("저장소 유형"),
        RepoCreateLabel::Empty => text.literal("빈 저장소"),
        RepoCreateLabel::Mirror => text.literal("미러"),
        RepoCreateLabel::Import => text.literal("가져오기"),
        RepoCreateLabel::NewLead => text.literal("빈 저장소를 만들거나, 프로젝트를 가져오거나, 원본과 동기화되는 미러를 유지하세요."),
        RepoCreateLabel::Owner => text.literal("소유자"),
        RepoCreateLabel::Name => text.literal("이름"),
        RepoCreateLabel::Description => text.literal("설명"),
        RepoCreateLabel::Optional => text.literal("선택 사항"),
        RepoCreateLabel::Visibility => text.literal("공개 범위"),
        RepoCreateLabel::Public => text.literal("공개"),
        RepoCreateLabel::Private => text.literal("비공개"),
        RepoCreateLabel::PublicRepository => text.literal("누구나 이 저장소를 보고 클론할 수 있습니다."),
        RepoCreateLabel::PublicMirror => text.literal("누구나 미러를 보고 클론할 수 있습니다."),
        RepoCreateLabel::PublicImport => text.literal("누구나 저장소를 보고 클론할 수 있습니다."),
        RepoCreateLabel::PrivateHint => text.literal("소유자만 볼 수 있습니다. 조직의 경우 조직 구성원이 볼 수 있습니다."),
        RepoCreateLabel::Cancel => text.literal("취소"),
        RepoCreateLabel::CreateRepository => text.literal("저장소 만들기"),
        RepoCreateLabel::MirrorLead => text.literal("미러는 다른 곳에 있는 공개 저장소의 브랜치와 태그를 일정에 따라 가져옵니다. 여기서는 읽기 전용이며, 변경 사항은 업스트림에서 가져옵니다."),
        RepoCreateLabel::MirrorsDisabled => text.literal("미러가 비활성화되어 있습니다"),
        RepoCreateLabel::MirrorsDisabledText => text.literal("이 인스턴스의 관리자가 풀 미러를 비활성화했습니다."),
        RepoCreateLabel::UpstreamUrl => text.literal("업스트림 URL"),
        RepoCreateLabel::UrlHintStart => text.literal("공개 저장소의"),
        RepoCreateLabel::UrlHintOr => text.literal("또는"),
        RepoCreateLabel::UrlHintEnd => text.literal("URL입니다."),
        RepoCreateLabel::FromTheUrl => text.literal("URL에서 가져옴"),
        RepoCreateLabel::FetchEvery => text.literal("가져오기 주기"),
        RepoCreateLabel::Minutes => text.literal("분"),
        RepoCreateLabel::CreateMirror => text.literal("미러 만들기"),
        RepoCreateLabel::MirrorARepository => text.literal("저장소 미러링"),
        RepoCreateLabel::ImportLead => text.literal("GitHub 또는 Gitea에서 저장소를 기록과 함께 복사하고, 원하면 이슈, 풀 리퀘스트, 레이블, 마일스톤, 릴리스, Git LFS 파일도 함께 가져옵니다. 이슈와 풀 리퀘스트 번호는 그대로 유지됩니다."),
        RepoCreateLabel::ImportingDisabled => text.literal("가져오기가 비활성화되어 있습니다"),
        RepoCreateLabel::ImportingDisabledText => text.literal("이 인스턴스의 관리자가 가져오기를 비활성화했습니다."),
        RepoCreateLabel::ImportFrom => text.literal("가져올 위치"),
        RepoCreateLabel::ServerAddress => text.literal("서버 주소"),
        RepoCreateLabel::NotNeededForGitHub => text.literal("github.com에는 필요하지 않음"),
        RepoCreateLabel::Repository => text.literal("저장소"),
        RepoCreateLabel::OwnerNameOrUrl => text.literal("owner/name 또는 해당 URL"),
        RepoCreateLabel::AccessToken => text.literal("액세스 토큰"),
        RepoCreateLabel::OptionalForPublic => text.literal("공개 저장소는 선택 사항"),
        RepoCreateLabel::TokenHint => text.literal("저장소를 읽을 수 있는 토큰을 사용하면 원본의 요청 한도가 높아지며, 비공개 저장소에는 토큰이 필요합니다. 토큰은 가져오는 동안에만 사용되며 저장되지 않습니다."),
        RepoCreateLabel::AsOnTheSource => text.literal("원본과 동일"),
        RepoCreateLabel::BringAlong => text.literal("함께 가져올 항목"),
        RepoCreateLabel::Wiki => text.literal("위키"),
        RepoCreateLabel::WikiNotSupported => text.literal("지원되지 않음: 이 인스턴스에는 위키가 없습니다."),
        RepoCreateLabel::StartImport => text.literal("가져오기 시작"),
        RepoCreateLabel::ImportARepository => text.literal("저장소 가져오기"),
        RepoCreateLabel::ImportTitle => text.literal("가져오기"),
        RepoCreateLabel::FailedNote => text.literal("그때까지 가져온 내용은 모두 저장소에 있습니다."),
        RepoCreateLabel::NotReached => text.literal("진행되지 않음"),
        RepoCreateLabel::Started => text.literal("시작됨"),
        RepoCreateLabel::WaitingForSlot => text.literal("빈 슬롯을 기다리는 중"),
        RepoCreateLabel::Ended => text.literal("종료됨"),
        RepoCreateLabel::UpdatesByItself => text.literal("이 페이지는 자동으로 업데이트됩니다."),
        RepoCreateLabel::SkippedOrIncomplete => text.literal("건너뛰었거나 완료되지 않음"),
        RepoCreateLabel::GoToRepository => text.literal("저장소로 이동"),
        RepoCreateLabel::OpensAutomatically => text.literal("첫 번째 미러 가져오기가 완료되면 저장소가 자동으로 열립니다."),
        RepoCreateLabel::FetchInProgress => text.literal("미러 가져오는 중"),
        RepoCreateLabel::TransferSizeNote => text.literal("전송 크기는 업스트림에 따라 결정됩니다. 이 페이지에는 작업이 진행되는 동안 현재 단계가 표시됩니다."),
        RepoCreateLabel::RefreshStatus => text.literal("상태 새로 고침"),
        RepoCreateLabel::SettingsAndRetry => text.literal("미러 설정 및 재시도"),
        RepoCreateLabel::NoScriptHint => text.literal("상태 새로 고침을 사용해 진행 상황을 확인하고, 준비되면 저장소를 여세요."),
        RepoCreateLabel::Connecting => text.literal("업스트림 저장소에 연결하는 중입니다."),
        RepoCreateLabel::Fetching => text.literal("브랜치, 태그, Git 객체를 다운로드하는 중입니다. 저장소가 크면 몇 분 정도 걸릴 수 있습니다."),
        RepoCreateLabel::Finalizing => text.literal("저장소 콘텐츠와 기본 브랜치를 준비하는 중입니다."),
        RepoCreateLabel::FetchFailedRetry => text.literal("미러를 가져오지 못했습니다. 저장소 관리자가 미러 설정에서 다시 시도할 수 있습니다."),
        RepoCreateLabel::Complete => text.literal("미러링이 완료되었습니다. 저장소를 여는 중…"),
        RepoCreateLabel::Queued => text.literal("사용 가능한 미러 작업자를 기다리는 중입니다."),
        RepoCreateLabel::Stopped => text.literal("미러링이 중지되었습니다. 저장소를 여는 중…"),
        RepoCreateLabel::Mirroring => text.literal("미러링 중"),
    }
    RepoCreateMessage<'a> => MessageWriter |text| {
        RepoCreateMessage::AtLeastMinutes { minutes, .. } => {
            text.literal("최소 ");
            text.number(minutes);
            text.literal("분입니다.");
        }
        RepoCreateMessage::ImportFrom { source, .. } => {
            text.argument(source);
            text.literal("에서 가져옴:");
        }
        RepoCreateMessage::MoreWarnings { count, .. } => {
            text.literal("외 ");
            text.number(count);
            text.literal("건(서버 로그 참조)");
        }
        RepoCreateMessage::MirroringName { name, .. } => {
            text.argument(name);
            text.literal(" 미러링 중");
        }
        RepoCreateMessage::FetchFailed { error, .. } => {
            text.literal("미러를 가져오지 못했습니다: ");
            text.argument(error);
        }
    }
}
