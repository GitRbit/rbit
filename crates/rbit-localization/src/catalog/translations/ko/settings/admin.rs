//! Korean (`ko`) entries for `catalog/settings/admin.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage};

entries! { Ko:
    AdminMessage => MessageWriter |text| {
        AdminMessage::SiteAdministration => text.literal("사이트 관리"),
        AdminMessage::Administration => text.literal("관리"),
        AdminMessage::Dashboard => text.literal("대시보드"),
        AdminMessage::Users => text.literal("사용자"),
        AdminMessage::Organizations => text.literal("조직"),
        AdminMessage::Repositories => text.literal("저장소"),
        AdminMessage::Settings => text.literal("설정"),
        AdminMessage::AuditLog => text.literal("감사 로그"),
        AdminMessage::Configuration => text.literal("구성"),
        AdminMessage::Runners => text.literal("러너"),
        AdminMessage::Search => text.literal("검색"),
        AdminMessage::Pagination => text.literal("페이지 이동"),
        AdminMessage::Previous => text.literal("이전"),
        AdminMessage::Next => text.literal("다음"),
        AdminMessage::NoResults => text.literal("결과 없음"),
        AdminMessage::Cancel => text.literal("취소"),
        AdminMessage::Actions => text.literal("작업"),
        AdminMessage::Private => text.literal("비공개"),
        AdminMessage::Archived => text.literal("보관됨"),
        AdminMessage::Delete => text.literal("삭제"),
        AdminMessage::TypeToConfirmLead => text.literal("확인하려면 다음을 입력하세요:"),
        AdminMessage::TypeToConfirmTail => {}
        AdminMessage::NewPassword => text.literal("새 비밀번호"),
        AdminMessage::ConfirmPassword => text.literal("새 비밀번호 확인"),
        AdminMessage::Unavailable => text.literal("사용할 수 없음"),
    }
    DashboardMessage => MessageWriter |text| {
        DashboardMessage::Intro => text.literal("이 인스턴스의 개요입니다."),
        DashboardMessage::RestartServer => text.literal("서버 다시 시작"),
        DashboardMessage::RestartUnavailableHint => text.literal("이 서버 호스트에서는 다시 시작할 수 없습니다"),
        DashboardMessage::SoftwareUpdates => text.literal("소프트웨어 업데이트"),
        DashboardMessage::ConfigureUpdates => text.literal("업데이트 확인 및 버전 공개 설정"),
        DashboardMessage::LiveMonitoring => text.literal("실시간 모니터링"),
        DashboardMessage::MonitoringIntro => text.literal("최근 5분 동안의 트래픽, 응답 시간, Git 활동입니다."),
        DashboardMessage::Snapshot => text.literal("스냅샷"),
        DashboardMessage::Pause => text.literal("일시 중지"),
        DashboardMessage::NeedsJavaScript => text.literal("실시간 업데이트에는 JavaScript가 필요합니다. 최신 스냅샷을 보려면 이 페이지를 새로 고치세요."),
        DashboardMessage::System => text.literal("시스템"),
        DashboardMessage::InstanceSettings => text.literal("인스턴스 설정"),
        DashboardMessage::Edit => text.literal("편집"),
        DashboardMessage::Users => text.literal("사용자"),
        DashboardMessage::Organizations => text.literal("조직"),
        DashboardMessage::SharedNamespaces => text.literal("공유 네임스페이스"),
        DashboardMessage::Repositories => text.literal("저장소"),
        DashboardMessage::AccessTokens => text.literal("액세스 토큰"),
        DashboardMessage::AcrossAllUsers => text.literal("모든 사용자 합계"),
        DashboardMessage::Uptime => text.literal("가동 시간"),
        DashboardMessage::DataDirectory => text.literal("데이터 디렉터리"),
        DashboardMessage::DiskUsage => text.literal("디스크 사용량"),
        DashboardMessage::SiteName => text.literal("사이트 이름"),
        DashboardMessage::Registration => text.literal("가입"),
        DashboardMessage::RegistrationOpen => text.literal("누구나 가입 가능"),
        DashboardMessage::RegistrationClosed => text.literal("닫힘(관리자가 계정 생성)"),
        DashboardMessage::OrgPolicy => text.literal("조직"),
        DashboardMessage::AnyUserCreates => text.literal("모든 사용자가 만들 수 있음"),
        DashboardMessage::AdminsCreate => text.literal("관리자만 만들 수 있음"),
        DashboardMessage::NewRepositories => text.literal("새 저장소"),
        DashboardMessage::PublicByDefault => text.literal("기본값 공개"),
        DashboardMessage::PrivateByDefault => text.literal("기본값 비공개"),
        DashboardMessage::EmailDigests => text.literal("이메일 요약"),
        DashboardMessage::Restarting => text.literal("서버 다시 시작 중"),
        DashboardMessage::RestartRequested => text.literal("다시 시작 요청됨"),
        DashboardMessage::RestartingText => text.literal("서버가 다시 시작하기 전에 진행 중인 HTTP 요청을 마무리하고, SSH와 백그라운드 작업을 중지하고, 스토리지를 닫는 중입니다. 오래 걸리는 전송은 끝나기까지 시간이 걸릴 수 있습니다."),
        DashboardMessage::Waiting => text.literal("서버가 돌아오기를 기다리는 중…"),
        DashboardMessage::ReturnToDashboard => text.literal("대시보드로 돌아가기"),
        DashboardMessage::WaitNoScript => text.literal("잠시 기다린 후 대시보드로 돌아가 다시 연결하세요."),
        DashboardMessage::RestartQuestion => text.literal("이 rbit 애플리케이션을 다시 시작할까요?"),
        DashboardMessage::RestartText => text.literal("애플리케이션이 종료된 후 저장된 구성으로 다시 시작되는 동안 사이트를 잠시 사용할 수 없습니다. HTTP 요청은 마무리되지만 SSH 연결은 닫힙니다. 사용자의 저장된 세션은 유지됩니다."),
        DashboardMessage::RestartAudited => text.literal("이 작업은 관리 감사 로그에 기록됩니다."),
        DashboardMessage::RestartUnavailable => text.literal("다시 시작할 수 없음"),
        DashboardMessage::RestartUnsupported => text.literal("이 서버 호스트는 애플리케이션 다시 시작을 지원하지 않습니다. 서비스 관리자를 사용해 rbit을 다시 시작하세요."),
        DashboardMessage::BackToDashboard => text.literal("대시보드로 돌아가기"),
    }
    MonitoringMessage => MessageWriter |text| {
        MonitoringMessage::Requests => text.literal("구간 내 요청"),
        MonitoringMessage::ErrorRate => text.literal("서버 오류율"),
        MonitoringMessage::ProcessUptime => text.literal("프로세스 가동 시간"),
        MonitoringMessage::Samples => text.literal("5초 단위 샘플"),
        MonitoringMessage::FiveMinutesAgo => text.literal("5분 전"),
        MonitoringMessage::LatestSample => text.literal("최신 샘플"),
        MonitoringMessage::Collecting => text.literal("첫 샘플을 수집하는 중입니다. 차트는 5초 후에 표시됩니다."),
        MonitoringMessage::HttpResponses => text.literal("HTTP 응답"),
        MonitoringMessage::CurrentWindow => text.literal("현재 구간"),
        MonitoringMessage::Status => text.literal("상태"),
        MonitoringMessage::RequestsColumn => text.literal("요청"),
        MonitoringMessage::Share => text.literal("비율"),
        MonitoringMessage::GitActivity => text.literal("Git 활동"),
        MonitoringMessage::Operation => text.literal("작업"),
        MonitoringMessage::Exchanges => text.literal("교환"),
        MonitoringMessage::CloneFetch => text.literal("클론 / 페치"),
        MonitoringMessage::Push => text.literal("푸시"),
        MonitoringMessage::GitNote => text.literal("실패를 포함한 스마트 HTTP 교환입니다. SSH와 참조 광고는 제외됩니다. 응답 시간은 헤더까지의 시간이며, 스트리밍 전송은 계속될 수 있습니다."),
        MonitoringMessage::Footnote => text.literal("이 프로세스만 해당하며, 다시 시작하면 기록이 초기화됩니다. 모니터링 요청은 제외됩니다."),
        MonitoringMessage::Informational => text.literal("정보"),
        MonitoringMessage::Successful => text.literal("성공"),
        MonitoringMessage::Redirects => text.literal("리디렉션"),
        MonitoringMessage::ClientErrors => text.literal("클라이언트 오류"),
        MonitoringMessage::ServerErrors => text.literal("서버 오류"),
        MonitoringMessage::RequestTraffic => text.literal("요청 트래픽"),
        MonitoringMessage::ResponseTime => text.literal("응답 시간"),
        MonitoringMessage::RequestsPerSecond => text.literal("요청/초"),
        MonitoringMessage::MillisecondsAverage => text.literal("ms 평균"),
        MonitoringMessage::WaitingFirst => text.literal("첫 샘플을 기다리는 중"),
    }
    AdminText<'a> => MessageWriter |text| {
        AdminText::ReleaseNotes(version) => {
            text.literal("rbit ");
            text.argument(version);
            text.literal(" 릴리스 노트 보기");
        }
        AdminText::UsersDetail(admins, suspended) => {
            text.literal("관리자 ");
            text.argument(admins);
            text.literal("명 · 정지됨 ");
            text.argument(suspended);
            text.literal("명");
        }
        AdminText::UsersDetailOne(suspended) => {
            text.literal("관리자 1명 · 정지됨 ");
            text.argument(suspended);
            text.literal("명");
        }
        AdminText::RepositoriesDetail(public, private) => {
            text.literal("공개 ");
            text.argument(public);
            text.literal("개 · 비공개 ");
            text.argument(private);
            text.literal("개");
        }
        AdminText::Summary(first, last, total) => {
            text.argument(total);
            text.literal("개 중 ");
            text.argument(first);
            text.literal("–");
            text.argument(last);
        }
        AdminText::DigestsAt(hour) => {
            text.literal("사용자가 따로 선택하지 않으면 ");
            text.argument(hour);
            text.literal("에 발송");
        }
        AdminText::DeleteLabel(name) => {
            text.argument(name);
            text.literal(" 삭제");
        }
        AdminText::ActionsFor(name) => {
            text.argument(name);
            text.literal(" 작업");
        }
        AdminText::PasswordLength(length) => {
            text.number(length);
            text.literal("자 이상.");
        }
        AdminText::ChartLabel(title, value, unit) => {
            text.literal("최근 5분 동안의 ");
            text.argument(title);
            text.literal(". 최신 값: ");
            text.argument(value);
            text.literal(" ");
            text.argument(unit);
            text.literal(". 요청이 없으면 응답 시간에 빈 구간이 생깁니다.");
        }
        AdminText::UpdatedAt(time) => {
            text.argument(time);
            text.literal(" UTC 업데이트됨");
        }
        AdminText::Coverage(samples, limit) => {
            text.literal("샘플 ");
            text.number(samples);
            text.literal("/");
            text.number(limit);
            text.literal("개");
        }
        AdminText::InAdministration(page) => {
            text.argument(page);
            text.literal(" · 관리");
        }
        AdminText::SetPasswordFor(name) => {
            text.argument(name);
            text.literal(" 비밀번호 설정 · 관리");
        }
        AdminText::DeletePageTitle(name) => {
            text.argument(name);
            text.literal(" 삭제 · 관리");
        }
    }
}
