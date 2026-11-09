//! Korean (`ko`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};

entries! { Ko:
    RepoSettingsLabel => MessageWriter |text| {
        RepoSettingsLabel::Settings => text.literal("설정"),
        RepoSettingsLabel::AdminsOf => text.literal("이 페이지는 저장소"),
        RepoSettingsLabel::AdminsOfEnd => text.literal("관리자만 볼 수 있습니다."),
        RepoSettingsLabel::SettingsSaved => text.literal("설정이 저장되었습니다."),
        RepoSettingsLabel::General => text.literal("일반"),
        RepoSettingsLabel::Description => text.literal("설명"),
        RepoSettingsLabel::Optional => text.literal("선택 사항"),
        RepoSettingsLabel::Visibility => text.literal("공개 범위"),
        RepoSettingsLabel::Public => text.literal("공개"),
        RepoSettingsLabel::PublicHint => text.literal("누구나 이 저장소를 보고 클론할 수 있습니다."),
        RepoSettingsLabel::Private => text.literal("비공개"),
        RepoSettingsLabel::PrivateHint => text.literal("접근 권한이 있는 사람만 보고 클론할 수 있습니다."),
        RepoSettingsLabel::DefaultBranch => text.literal("기본 브랜치"),
        RepoSettingsLabel::PushABranch => text.literal("기본 브랜치를 선택하려면 브랜치를 푸시하세요."),
        RepoSettingsLabel::DefaultBranchHint => text.literal("탐색할 때 가장 먼저 표시되며 git clone 시 체크아웃됩니다."),
        RepoSettingsLabel::SaveChanges => text.literal("변경 사항 저장"),
        RepoSettingsLabel::RepositoryName => text.literal("저장소 이름"),
        RepoSettingsLabel::Name => text.literal("이름"),
        RepoSettingsLabel::RenameHint => text.literal("이전 이름을 사용하는 링크와 클론 URL은 더 이상 작동하지 않습니다."),
        RepoSettingsLabel::Rename => text.literal("이름 변경"),
        RepoSettingsLabel::DangerZone => text.literal("위험 구역"),
        RepoSettingsLabel::UnarchiveTitle => text.literal("이 저장소 보관 해제"),
        RepoSettingsLabel::UnarchiveHint => text.literal("다시 푸시, 이슈, 풀 리퀘스트, 댓글, 릴리스를 받을 수 있게 되며 미러도 다시 시작됩니다."),
        RepoSettingsLabel::Unarchive => text.literal("저장소 보관 해제"),
        RepoSettingsLabel::ArchiveTitle => text.literal("이 저장소 보관"),
        RepoSettingsLabel::ArchiveHint => text.literal("저장소가 읽기 전용이 됩니다. 클론, 포크, 스타는 계속 할 수 있지만 아무도 푸시하거나 병합할 수 없고 이슈, 풀 리퀘스트, 릴리스를 변경할 수 없으며 미러는 일시 중지됩니다. 언제든지 보관을 해제할 수 있습니다."),
        RepoSettingsLabel::Archive => text.literal("저장소 보관"),
        RepoSettingsLabel::DeleteTitle => text.literal("이 저장소 삭제"),
        RepoSettingsLabel::DeleteHint => text.literal("파일, 기록, 스타가 영구적으로 삭제됩니다. 이 작업은 되돌릴 수 없습니다."),
        RepoSettingsLabel::Type => text.literal("확인하려면"),
        RepoSettingsLabel::ToConfirm => text.literal("입력"),
        RepoSettingsLabel::DeleteRepository => text.literal("저장소 삭제"),
        RepoSettingsLabel::DeployKeys => text.literal("배포 키"),
        RepoSettingsLabel::DeployKeysHint => text.literal("서버나 CI 작업에 이 저장소에 대해서만 SSH 접근 권한을 부여하는 키입니다."),
        RepoSettingsLabel::DeployKeysLeadStart => text.literal("배포 키는 서버나 CI 작업에 다음 저장소에 대해서만 SSH 접근 권한을 부여합니다:"),
        RepoSettingsLabel::DeployKeysLeadEnd => text.literal("푸시를 허용하지 않으면 키는 읽기 전용입니다."),
        RepoSettingsLabel::NewDeployKey => text.literal("새 배포 키"),
        RepoSettingsLabel::Title => text.literal("제목"),
        RepoSettingsLabel::TitlePlaceholder => text.literal("예: 프로덕션 서버"),
        RepoSettingsLabel::TitleHint => text.literal("기본값은 키의 주석입니다."),
        RepoSettingsLabel::Key => text.literal("키"),
        RepoSettingsLabel::KeyHint => text.literal("키 하나는 저장소나 계정 하나에만 속할 수 있으므로 각각 별도의 키를 생성하세요."),
        RepoSettingsLabel::AllowWrite => text.literal("쓰기 권한 허용"),
        RepoSettingsLabel::AllowWriteHint => text.literal("이 키로 저장소를 클론할 뿐만 아니라 푸시할 수도 있습니다."),
        RepoSettingsLabel::AddDeployKey => text.literal("배포 키 추가"),
        RepoSettingsLabel::Keys => text.literal("키"),
        RepoSettingsLabel::NoDeployKeys => text.literal("배포 키 없음"),
        RepoSettingsLabel::NoDeployKeysText => text.literal("서버나 CI 작업이 SSH로 이 저장소를 클론할 수 있도록 키를 추가하세요."),
        RepoSettingsLabel::Delete => text.literal("삭제"),
        RepoSettingsLabel::Mirror => text.literal("미러"),
        RepoSettingsLabel::MirrorHint => text.literal("업스트림, 페치 간격, 마지막 페치 결과입니다."),
        RepoSettingsLabel::MirrorLead => text.literal("이 저장소는 업스트림의 브랜치와 태그를 가져오며 푸시를 받지 않습니다."),
        RepoSettingsLabel::Upstream => text.literal("업스트림"),
        RepoSettingsLabel::LastFetch => text.literal("마지막 페치"),
        RepoSettingsLabel::Failed => text.literal("실패"),
        RepoSettingsLabel::Ok => text.literal("정상"),
        RepoSettingsLabel::NotYet => text.literal("아직 없음"),
        RepoSettingsLabel::NextFetch => text.literal("다음 페치"),
        RepoSettingsLabel::Error => text.literal("오류"),
        RepoSettingsLabel::FetchNow => text.literal("지금 페치"),
        RepoSettingsLabel::FetchEvery => text.literal("페치 간격"),
        RepoSettingsLabel::Minutes => text.literal("분"),
        RepoSettingsLabel::Save => text.literal("저장"),
        RepoSettingsLabel::Saved => text.literal("저장되었습니다."),
        RepoSettingsLabel::FetchQueued => text.literal("페치가 대기열에 추가되었습니다. 잠시 후 새로 고침하면 결과를 볼 수 있습니다."),
        RepoSettingsLabel::PushQueued => text.literal("푸시가 대기열에 추가되었습니다. 잠시 후 새로 고침하면 결과를 볼 수 있습니다."),
        RepoSettingsLabel::StopMirroring => text.literal("미러링 중지"),
        RepoSettingsLabel::StopHint => text.literal("저장소의 내용은 유지되고, 페치를 중지하며 다른 저장소처럼 푸시를 받습니다."),
        RepoSettingsLabel::PushMirrors => text.literal("푸시 미러"),
        RepoSettingsLabel::PushMirrorsHint => text.literal("푸시할 때마다 이 저장소의 브랜치와 태그를 받는 원격 저장소입니다."),
        RepoSettingsLabel::PushMirrorsLead => text.literal("이 저장소의 브랜치와 태그를 받는 원격 저장소입니다. 푸시할 때마다 원격의 브랜치와 태그를 덮어쓰며, 여기에 더 이상 없는 브랜치와 태그는 삭제합니다."),
        RepoSettingsLabel::PushMirrorsDisabled => text.literal("푸시 미러가 비활성화되어 있습니다"),
        RepoSettingsLabel::PushMirrorsDisabledText => text.literal("이 인스턴스의 관리자가 푸시 미러를 비활성화했습니다."),
        RepoSettingsLabel::NoPushMirrors => text.literal("아직 푸시 미러가 없습니다"),
        RepoSettingsLabel::NoPushMirrorsText => text.literal("이 저장소의 사본을 다른 곳에 유지하려면 아래에서 원격 저장소를 추가하세요."),
        RepoSettingsLabel::Paused => text.literal("일시 중지됨"),
        RepoSettingsLabel::OnPush => text.literal("푸시 시"),
        RepoSettingsLabel::LastPush => text.literal("마지막 푸시"),
        RepoSettingsLabel::NotPushedYet => text.literal("아직 푸시되지 않음"),
        RepoSettingsLabel::As => text.literal("사용자:"),
        RepoSettingsLabel::SecretConfigured => text.literal("시크릿 구성됨"),
        RepoSettingsLabel::PushNow => text.literal("지금 푸시"),
        RepoSettingsLabel::Edit => text.literal("편집"),
        RepoSettingsLabel::Remove => text.literal("제거"),
        RepoSettingsLabel::AddPushMirrorTitle => text.literal("푸시 미러 추가"),
        RepoSettingsLabel::AddPushMirror => text.literal("푸시 미러 추가"),
        RepoSettingsLabel::PushMirrorAdded => text.literal("푸시 미러를 추가했습니다. 첫 번째 푸시가 진행 중이니 잠시 후 새로 고침하면 결과를 볼 수 있습니다."),
        RepoSettingsLabel::PushMirrorRemoved => text.literal("푸시 미러를 제거했습니다. 원격 저장소의 내용은 그대로 유지됩니다."),
        RepoSettingsLabel::EditPushMirror => text.literal("푸시 미러 편집"),
        RepoSettingsLabel::PushMirror => text.literal("푸시 미러"),
        RepoSettingsLabel::Enabled => text.literal("사용"),
        RepoSettingsLabel::EnabledHint => text.literal("일시 중지된 미러는 설정을 유지하며 푸시되지 않습니다."),
        RepoSettingsLabel::Cancel => text.literal("취소"),
        RepoSettingsLabel::RemoteUrl => text.literal("원격 URL"),
        RepoSettingsLabel::UrlHintStart => text.literal("사용자 이름이나 비밀번호가 포함되지 않은"),
        RepoSettingsLabel::UrlHintOr => text.literal("또는"),
        RepoSettingsLabel::UrlHintEnd => text.literal("URL입니다."),
        RepoSettingsLabel::UserName => text.literal("사용자 이름"),
        RepoSettingsLabel::PasswordOrToken => text.literal("비밀번호 또는 토큰"),
        RepoSettingsLabel::SecretPlaceholder => text.literal("구성됨 — 유지하려면 비워 두세요"),
        RepoSettingsLabel::SecretHint => text.literal("HTTP Basic 인증으로 원격 저장소에 전송됩니다. 다시 표시할 수 없습니다."),
        RepoSettingsLabel::SecretHintKept => text.literal("HTTP Basic 인증으로 원격 저장소에 전송됩니다. 다시 표시할 수 없으며, 원격 저장소가 다른 호스트로 옮겨지면 다시 입력해야 합니다."),
        RepoSettingsLabel::PushAfterPush => text.literal("푸시할 때마다 푸시"),
        RepoSettingsLabel::PushAfterPushHint => text.literal("이 저장소가 변경되고 몇 초 후에 푸시합니다. 모든 미러는 일정에 따라서도 푸시됩니다."),
    }
    RepoSettingsMessage<'a> => MessageWriter |text| {
        RepoSettingsMessage::AtLeastMinutes { minutes, .. } => {
            text.literal("최소 ");
            text.number(minutes);
            text.literal("분입니다.");
        }
        RepoSettingsMessage::PushSchedule { minutes, .. } => {
            text.literal("모든 미러는 최소 ");
            text.number(minutes);
            text.literal("분마다 푸시됩니다.");
        }
        RepoSettingsMessage::DeleteKey { title, .. } => {
            text.argument(title);
            text.literal(" 삭제");
        }
    }
}
