//! Korean labels and help of the configuration fields in `catalog/settings/config/fields_a.rs`.

/// The label and help of `key` in `section`, or `None` when this table lacks it.
#[allow(clippy::too_many_lines, reason = "one arm per configuration field")]
pub(super) fn field(section: &str, key: &str) -> Option<(&'static str, &'static str)> {
    Some(match (section, key) {
        ("updates", "check_for_updates") => (
            "소프트웨어 업데이트 확인",
            "확인에 성공한 후에는 최대 하루에 한 번 공개 GitHub 릴리스 피드에 접속합니다. 실행 중인 버전이나 인스턴스 정보는 전송되지 않습니다. 다시 시작해야 적용됩니다.",
        ),
        ("updates", "report_version") => (
            "실행 중인 버전 공개",
            "공개 바닥글, 버전 API, SSH 배너, 외부 연동의 User-Agent에 소프트웨어 버전을 표시합니다. 숨기려면 끄세요. 관리자는 로컬 진단 정보를 계속 볼 수 있습니다. 다시 시작해야 적용됩니다.",
        ),
        ("server", "bind") => ("수신 주소", "HTTP 서버가 바인딩할 주소입니다."),
        ("server", "public_url") => ("공개 URL", "클론 URL과 CSRF 출처 확인에 사용하는 공개 기본 URL입니다."),
        ("server", "site_name") => {
            ("기본 사이트 이름", "실시간 브랜딩으로 덮어쓰기 전의 기본 이름입니다. 현재 이름은 브랜딩 및 정책에서 변경하세요.")
        }
        ("server", "open_registration") => ("기본적으로 가입 허용", "기본 가입 정책입니다. 브랜딩 및 정책의 실시간 정책이 우선합니다."),
        ("storage", "data_dir") => {
            ("데이터 디렉터리", "여기서는 읽기 전용입니다. 서버를 중지하고 데이터를 옮긴 다음 배포 파일이나 환경 변수를 업데이트하세요.")
        }
        ("storage", "secret_key_file") => (
            "비밀 키 파일",
            "여기서는 읽기 전용입니다. 저장된 자격 증명을 암호화하는 키이며, 비어 있으면 데이터 디렉터리의 secret.key를 사용합니다. 데이터베이스와 함께 백업하세요.",
        ),
        ("security", "session_lifetime_days") => ("세션 유효 기간(일)", "활동이 없을 때 세션이 유효하게 유지되는 기간(일)입니다."),
        ("security", "session_max_age_days") => {
            ("최대 세션 기간(일)", "활동 여부와 관계없이 세션이 유효하게 유지되는 최대 기간(일)입니다. 0이면 제한이 없습니다.")
        }
        ("security", "min_password_length") => ("최소 비밀번호 길이", "가입할 때 허용되는 최소 비밀번호 길이입니다."),
        ("security", "max_password_jobs") => (
            "최대 비밀번호 작업 수",
            "공유 비밀번호 작업 예산에서 동시에 실행되는 해싱 또는 검증 작업 수입니다(1–64, 기본값 2). 다시 시작해야 적용됩니다.",
        ),
        ("security", "trusted_proxies") => (
            "신뢰할 수 있는 프록시 네트워크",
            "X-Forwarded-For, Forwarded, X-Forwarded-Proto 헤더를 신뢰할 리버스 프록시(CIDR 네트워크, 예: 127.0.0.1/32, 10.0.0.0/8, ::1/128)입니다. 비워 두면(기본값) 항상 TCP 피어를 클라이언트로 간주합니다.",
        ),
        ("security", "rate_limit") => (
            "속도 제한 사용",
            "클라이언트별로 로그인, 가입, 2단계 인증 코드, Git 자격 증명, 양식 제출을 제한합니다. 부하 테스트를 할 때만 끄는 것이 좋습니다.",
        ),
        ("security", "hsts") => (
            "HSTS 보내기",
            "공개 URL이 https://이면 Strict-Transport-Security(1년)를 보내 브라우저가 이 호스트에 일반 HTTP로 접속하지 않도록 합니다. HTTPS 사용을 중단하는 경우에만 끄세요.",
        ),
        ("security", "hsts_include_subdomains") => {
            ("HSTS에 하위 도메인 포함", "해당 헤더에 includeSubDomains를 추가해 호스트의 모든 하위 도메인에도 HTTPS를 강제합니다.")
        }
        ("security", "external_images") => (
            "외부 이미지 허용",
            "다른 사이트의 이미지를 페이지에 표시합니다. 이 경우 해당 사이트에 누가 페이지를 봤는지도 알려집니다. 끄면 이 인스턴스의 이미지와 인라인 data: 이미지만 표시합니다.",
        ),
        ("repository", "default_branch") => ("기본 브랜치", "새로 만든 저장소에서 HEAD가 가리키는 브랜치입니다."),
        ("repository", "git_binary") => ("Git 실행 파일", "스마트 HTTP 전송에 사용하는 git 실행 파일의 경로입니다."),
        ("repository", "max_render_bytes") => ("최대 렌더링 크기(바이트)", "이보다 큰 파일은 브라우저에서 인라인으로 렌더링하지 않습니다."),
        ("repository", "max_git_processes") => (
            "최대 Git 프로세스 수",
            "동시에 실행되는 Git 전송 프로세스(HTTP 및 SSH를 통한 클론, 페치, 푸시) 수입니다. 이를 넘는 요청은 거부됩니다(HTTP에서는 503).",
        ),
        ("repository", "max_git_requests_per_ip") => (
            "IP별 최대 Git 요청 수",
            "한 IP 주소에서 동시에 보낼 수 있는 스마트 HTTP Git 요청 수입니다(초과 시 429). 리버스 프록시 뒤에서는 신뢰할 수 있는 프록시 네트워크 설정이 필요합니다.",
        ),
        ("repository", "git_idle_timeout_seconds") => {
            ("Git 유휴 시간 제한(초)", "스마트 HTTP Git 교환이 양방향 모두 트래픽 없이 유지될 수 있는 시간(초)입니다.")
        }
        ("repository", "git_max_seconds") => {
            ("Git 최대 실행 시간(초)", "스마트 HTTP Git 교환 하나(매우 큰 클론이나 푸시)의 최대 시간(초)입니다.")
        }
        ("repository", "max_push_bytes") => {
            ("최대 푸시 크기(바이트)", "HTTP와 SSH에서 한 번의 푸시로 보낼 수 있는 최대 팩 크기입니다. 0이면 제한이 없습니다.")
        }
        ("repository", "max_archives_per_ip") => {
            ("IP별 최대 아카이브 수", "한 IP 주소에서 동시에 진행할 수 있는 소스 아카이브 다운로드 수입니다.")
        }
        ("repository", "archive_idle_timeout_seconds") => (
            "아카이브 유휴 시간 제한(초)",
            "아카이브 다운로드가 멈춰 있을 수 있는 시간(초)입니다(클라이언트가 읽지 않거나 git이 쓰지 않는 경우).",
        ),
        ("repository", "archive_max_seconds") => ("아카이브 최대 실행 시간(초)", "아카이브 다운로드 하나의 최대 시간(초)입니다."),
        ("mail", "transport") => ("전송 방식", "서버를 다시 시작한 후 적용되는 배포 설정입니다."),
        ("mail", "from_address") => ("보내는 사람 이메일 주소", "모든 메시지의 From: 주소입니다."),
        ("mail", "from_name") => ("보내는 사람 표시 이름", "From:에 표시되는 이름입니다. 비워 두면 사이트 이름을 사용합니다."),
        ("mail", "smtp_host") => ("SMTP 서버", "SMTP 릴레이의 호스트 이름입니다(transport = \"smtp\")."),
        ("mail", "smtp_port") => ("SMTP 포트", "비워 두면 STARTTLS 또는 암호화 없음에는 587 포트를, 암시적 TLS에는 465 포트를 사용합니다."),
        ("mail", "smtp_security") => ("SMTP 암호화", "서버를 다시 시작한 후 적용되는 배포 설정입니다."),
        ("mail", "smtp_username") => ("SMTP 사용자 이름", "릴레이의 자격 증명입니다. 비워 두면 인증하지 않습니다."),
        ("mail", "smtp_password") => ("SMTP 비밀번호", "서버를 다시 시작한 후 적용되는 배포 설정입니다."),
        ("mail", "timeout_seconds") => ("시간 제한(초)", "이 시간(초) 안에 전달되지 않은 메시지는 포기합니다."),
        ("ssh", "enabled") => ("이 기능 사용", "내장 SSH 서버를 실행합니다."),
        ("ssh", "bind") => ("수신 주소", "SSH 서버가 바인딩할 주소입니다."),
        ("ssh", "public_host") => ("공개 호스트", "SSH 클론 URL에 표시되는 호스트 이름입니다. 기본값은 server.public_url의 호스트입니다."),
        ("ssh", "public_port") => ("공개 포트", "프록시나 포트 매핑 때문에 바인딩 포트와 다를 때 SSH 클론 URL에 표시되는 포트입니다."),
        ("ssh", "host_key_path") => (
            "호스트 키 경로",
            "SSH 호스트 키 파일의 경로입니다. 비워 두면 데이터 디렉터리 안의 ssh/host_ed25519를 사용하며, 없으면 처음 시작할 때 생성됩니다.",
        ),
        ("ssh", "max_connections") => ("최대 연결 수", "모든 클라이언트의 동시 연결 수입니다. 이를 넘는 연결은 즉시 닫힙니다."),
        ("ssh", "max_connections_per_ip") => ("IP별 최대 연결 수", "한 IP 주소의 동시 연결 수입니다."),
        ("ssh", "auth_timeout_seconds") => ("인증 시간 제한(초)", "클라이언트가 연결 후 인증을 마쳐야 하는 시간(초)입니다."),
        ("ssh", "idle_timeout_seconds") => ("유휴 시간 제한(초)", "트래픽이 전혀 없을 때 연결을 닫기까지의 시간(초)입니다."),
        ("ssh", "max_session_seconds") => ("최대 세션 시간(초)", "Git 작업 하나(매우 큰 클론이나 푸시)의 최대 시간(초)입니다."),
        ("auth", "timeout_seconds") => ("시간 제한(초)", "공급자에 대한 요청 하나의 최대 시간입니다."),
        ("auth", "allow_private_addresses") => (
            "사설 네트워크 대상 허용",
            "주소가 루프백, 사설, 링크 로컬 또는 고유 로컬인 공급자와, 일반 http를 쓰는 발급자 또는 기본 URL을 허용합니다.",
        ),
        ("branding", "primary_color") => {
            ("기본 색상", "#RRGGBB 형식의 기본 버튼 배경색입니다. 두 색상을 모두 설정하거나 모두 비워 두세요.")
        }
        ("branding", "primary_text_color") => {
            ("기본 텍스트 색상", "#RRGGBB 형식의 기본 버튼 텍스트 색상입니다. 읽기 쉬운 대비를 선택하세요.")
        }
        ("branding", "description") => ("설명", VISITORS),
        ("branding", "welcome_title") => ("환영 제목", VISITORS),
        ("branding", "welcome_message") => ("환영 메시지", VISITORS),
        ("branding", "logo_url") => ("로고 URL", VISITORS),
        ("branding", "favicon_url") => ("파비콘 URL", VISITORS),
        ("branding", "app_icon_url") => ("앱 아이콘 URL", VISITORS),
        ("branding", "footer_text") => ("바닥글 텍스트", VISITORS),
        ("branding", "support_url") => ("지원 URL", VISITORS),
        ("branding", "terms_url") => ("이용 약관 URL", VISITORS),
        ("branding", "privacy_url") => ("개인정보 처리방침 URL", VISITORS),
        ("branding", "show_powered_by") => ("Powered by 표시", VISITORS),
        ("branding", "show_version") => ("버전 표시", VISITORS),
        ("branding", "show_features") => ("기능 표시", VISITORS),
        ("notifications", "email") => ("알림 이메일 보내기", "알림을 이메일로도 보낼지 여부입니다."),
        ("notifications", "default_email") => ("기본 이메일 구독", "직접 선택한 적이 없는 사용자에게 적용되는 설정입니다."),
        ("notifications", "digest_hour") => (
            "기본 요약 발송 시각(UTC)",
            "시간을 선택하지 않은 사용자에게 일간 및 주간 이메일 요약을 보내는 시각으로, UTC 기준 0–23입니다. 사이트 관리 → 설정의 값이 실행 중에 우선합니다.",
        ),
        ("operations", "metrics") => ("메트릭 엔드포인트 사용", "/metrics에서 Prometheus 메트릭을 제공합니다."),
        ("operations", "metrics_token") => (
            "메트릭 액세스 토큰",
            "설정하면 /metrics에 Authorization: Bearer <token>이 필요합니다. 신뢰할 수 없는 네트워크에서 엔드포인트에 접근할 수 없는 경우에만 비워 두세요.",
        ),
        ("operations", "warm_caches_on_push") => (
            "푸시 시 캐시 미리 채우기",
            "푸시할 때마다 기본 브랜치의 언어 및 마지막 커밋 캐시를 미리 채워, 푸시 후 첫 방문자가 계산 비용을 부담하지 않게 합니다.",
        ),
        ("operations", "maintenance_interval_minutes") => (
            "유지 관리 간격(분)",
            "유지 관리 실행 사이의 간격(분)입니다(마지막 실행 이후 푸시된 저장소에 git gc --auto를 실행한 다음 인사이트 캐시를 정리). 0이면 주기적 유지 관리를 하지 않습니다.",
        ),
        ("operations", "job_concurrency") => {
            ("작업 동시 실행 수", "동시에 실행되는 백그라운드 작업(캐시 미리 채우기, git gc)의 최대 수입니다.")
        }
        ("operations", "insights_cache_max_age_days") => (
            "인사이트 캐시 최대 보관 기간(일)",
            "이보다 오래된 인사이트 캐시 항목은 유지 관리에서 삭제됩니다. 0이면 기간과 관계없이 보관합니다.",
        ),
        ("operations", "insights_cache_max_entries") => (
            "인사이트 캐시 최대 항목 수",
            "인사이트 캐시 항목이 이 수를 넘으면 유지 관리에서 가장 오래된 항목부터 삭제합니다. 0이면 제한이 없습니다.",
        ),
        ("api", "enabled") => ("이 기능 사용", "/api/v1을 제공합니다. 끄면 모든 API 경로가 404를 반환합니다."),
        ("api", "default_page_size") => ("기본 페이지 크기", "요청에서 지정하지 않았을 때 페이지당 항목 수입니다."),
        ("api", "max_page_size") => ("최대 페이지 크기", "?per_page=의 상한입니다."),
        _ => return None,
    })
}

/// The help shared by the branding fields.
const VISITORS: &str = "이 인스턴스의 방문자에게 표시됩니다.";
