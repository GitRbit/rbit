//! Korean (`ko`) entries for `catalog/shell/home.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::home::{Feature, HomeMessage, Involvement};

entries! { Ko:
    HomeMessage<'a> => MessageWriter |text| {
        HomeMessage::SearchPublic => text.literal("공개 저장소 검색"),
        HomeMessage::Search => text.literal("검색"),
        HomeMessage::ExploreAll => text.literal("모든 저장소 탐색"),
        HomeMessage::ThisInstance => text.literal("이 인스턴스"),
        HomeMessage::RepositoryCount(_) => text.literal("저장소"),
        HomeMessage::UserCount(_) => text.literal("사용자"),
        HomeMessage::OrganizationCount(_) => text.literal("조직"),
        HomeMessage::RecentlyUpdated => text.literal("최근 업데이트됨"),
        HomeMessage::ViewAll => text.literal("모두 보기"),
        HomeMessage::NoPublicRepositories => text.literal("아직 공개 저장소가 없습니다"),
        HomeMessage::NoPublicRepositoriesText => text.literal("이 서버에서 공개된 저장소가 여기에 표시됩니다."),
        HomeMessage::FreshServer => text.literal("이 서버에는 아직 계정이 없습니다. 처음 가입하는 계정이 관리자가 됩니다."),
        HomeMessage::BuiltForSelfHosting => text.literal("셀프 호스팅을 위한 설계"),
        HomeMessage::FeatureTitle(feature) => text.literal(match feature {
            Feature::Binary => "빠른 단일 바이너리",
            Feature::Protocols => "HTTPS와 SSH를 통한 Git",
            Feature::Teams => "조직과 팀",
            Feature::Releases => "릴리스, 미러, 웹훅",
            Feature::Security => "기본적으로 안전",
            Feature::Screens => "모든 화면에서",
        }),
        HomeMessage::FeatureText(feature) => text.literal(match feature {
            Feature::Binary => "SQLite와 내장 저장소를 사용합니다. 따로 설치하거나 실행할 서비스가 없습니다.",
            Feature::Protocols => "프로토콜 v2를 지원하는 스마트 HTTP, 내장 SSH 서버, 배포 키를 제공합니다.",
            Feature::Teams => "역할별로 팀과 공동 작업자에게 저장소를 공유합니다.",
            Feature::Releases => "릴리스를 게시하고, 업스트림 프로젝트를 미러링하고, 다른 서비스에 알립니다.",
            Feature::Security => "2단계 로그인, 범위가 지정된 액세스 토큰, 감사 로그를 제공합니다.",
            Feature::Screens => "휴대폰 크기까지 반응형으로 표시되며 앱으로 설치할 수 있습니다.",
        }),
        HomeMessage::Dashboard => text.literal("대시보드"),
        HomeMessage::ViewProfile => text.literal("프로필 보기"),
        HomeMessage::New => text.literal("새로 만들기"),
        HomeMessage::NewRepository => text.literal("새 저장소"),
        HomeMessage::PersonalDashboards => text.literal("개인 대시보드"),
        HomeMessage::Issues => text.literal("이슈"),
        HomeMessage::PullRequests => text.literal("풀 리퀘스트"),
        HomeMessage::Repositories => text.literal("저장소"),
        HomeMessage::PrivateCount(count) => {
            text.literal("비공개 ");
            text.number(u64::try_from(count).unwrap_or(u64::MAX));
            text.literal("개");
        }
        HomeMessage::FindRepository => text.literal("저장소 찾기…"),
        HomeMessage::FilterRepositories => text.literal("내 저장소 필터링"),
        HomeMessage::NoRepositories => text.literal("아직 저장소가 없습니다"),
        HomeMessage::NoRepositoriesText => text.literal("저장소를 만들어 이 서버에서 코드 호스팅을 시작하세요."),
        HomeMessage::CreateRepository => text.literal("저장소 만들기"),
        HomeMessage::NoFilterMatch => text.literal("필터와 일치하는 저장소가 없습니다."),
        HomeMessage::Organizations => text.literal("조직"),
        HomeMessage::NewOrganization => text.literal("새 조직"),
        HomeMessage::NotAMember => text.literal("소속된 조직이 없습니다."),
        HomeMessage::CreateOne => text.literal("조직을 만들어"),
        HomeMessage::ShareWithTeam => text.literal("팀과 저장소를 공유하세요."),
        HomeMessage::Explore => text.literal("탐색"),
        HomeMessage::NothingPublic => text.literal("아직 공개된 항목이 없습니다."),
        HomeMessage::YourInvolvement => text.literal("내 참여"),
        HomeMessage::InvolvementTab(tab) => text.literal(match tab {
            Involvement::Created => "내가 만든 항목",
            Involvement::Assigned => "할당됨",
            Involvement::Mentioned => "멘션됨",
            Involvement::ReviewRequested => "리뷰 요청됨",
        }),
        HomeMessage::SearchIn(title) => {
            text.argument(title);
            text.literal(" 검색");
        }
        HomeMessage::SearchHelp => text.literal("검색 도움말"),
        HomeMessage::SearchHelpText => text.literal("is:open, is:closed, author:@me, assignee:@me, label:bug, milestone:\"v1\", no:assignee 또는 sort:updated-desc를 사용하세요. 텍스트는 제목과 설명에서 검색합니다. 결과에는 현재 읽을 수 있는 저장소만 포함됩니다. 멘션됨에는 본문, 댓글, 제출된 리뷰에서 알려진 멘션이 알림을 읽은 후에도 포함됩니다."),
        HomeMessage::Results => text.literal("결과"),
        HomeMessage::OpenCount(count) => {
            text.literal("열림 ");
            text.number(count);
            text.literal("개");
        }
        HomeMessage::ClosedCount(count) => {
            text.literal("닫힘 ");
            text.number(count);
            text.literal("개");
        }
        HomeMessage::NoMatchingThreads => text.literal("일치하는 스레드가 없습니다"),
        HomeMessage::NoMatchingThreadsText => text.literal("다른 필터나 참여 탭을 사용해 보세요."),
        HomeMessage::CommentCount(count) => {
            text.literal("댓글 ");
            text.number(u64::from(count));
            text.literal("개");
        }
    }
}
