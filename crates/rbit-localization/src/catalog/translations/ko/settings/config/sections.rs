//! Korean section titles, section help and choice labels of `catalog/settings/config.rs`.

/// The title and help of the section `key`, or `None` when this table lacks it.
pub(super) fn section(key: &str) -> Option<(&'static str, &'static str)> {
    Some(match key {
        "updates" => (
            "소프트웨어 업데이트 및 개인정보",
            "독립적인 백그라운드 릴리스 확인과 실행 중인 버전 공개 설정입니다. 변경 사항은 다시 시작한 후 적용됩니다.",
        ),
        "server" => ("서버", "공개 주소, 수신 설정, 인스턴스 기본값입니다."),
        "storage" => ("스토리지", "데이터 디렉터리를 확인합니다. 스토리지를 옮기려면 오프라인 상태에서 배포 구성을 사용하세요."),
        "security" => ("보안", "세션, 비밀번호, 속도 제한, 신뢰할 수 있는 리버스 프록시입니다."),
        "repository" => ("저장소", "Git 저장소의 기본값과 한도입니다."),
        "mail" => ("이메일 전송", "보내는 사람 정보, SMTP 연결, 전송 시간 제한을 구성합니다."),
        "ssh" => ("SSH", "내장 Git SSH 서버, 공개 클론 주소, 연결 한도입니다."),
        "auth" => ("로그인 공급자", "OIDC 및 GitHub ID 공급자를 연결합니다."),
        "branding" => ("브랜딩 기본값", "배포 기본값입니다. 인스턴스 전체에 즉시 적용하려면 브랜딩 및 정책을 사용하세요."),
        "notifications" => ("이메일 알림", "알림 이메일을 켜고 기본 구독 정책을 선택합니다."),
        "operations" => ("운영", "메트릭, 유지 관리, 백그라운드 작업, 캐시입니다."),
        "api" => ("REST API", "API 사용 여부와 페이지 한도입니다."),
        "webhooks" => ("웹훅", "외부 전송, 재시도, 보관 설정입니다."),
        "releases" => ("릴리스", "릴리스 에셋 크기, 개수, 업로드 시간 제한입니다."),
        "mirrors" => ("풀 미러", "페치 일정, 시간 제한, 네트워크 접근입니다."),
        "push_mirrors" => ("푸시 미러", "푸시 일정, 시간 제한, 네트워크 접근입니다."),
        "imports" => ("가져오기", "저장소 가져오기, 동시 실행 수, 작성자 연결입니다."),
        "issues" => ("이슈 및 받은 알림함", "커밋 검사와 앱 내 알림 보관입니다."),
        "pulls" => ("풀 리퀘스트", "병합 한도, 백그라운드 검사, 작성자 주소입니다."),
        "editor" => ("브라우저 편집기", "브라우저 업로드 크기, 개수, 시간 제한입니다."),
        "lfs" => ("Git LFS", "대용량 파일 스토리지, 객체 크기, 토큰 유효 기간입니다."),
        "actions" => ("Actions", "외부 러너에서 실행하는 CI/CD, 보관, 한도, 시간 제한입니다."),
        _ => return None,
    })
}

/// The label of `value` in the choice field `key` of `section`, or `None`.
pub(super) fn choice(section: &str, key: &str, value: &str) -> Option<&'static str> {
    Some(match (section, key, value) {
        ("mail", "transport", "disabled") => "사용 안 함",
        ("mail", "transport", "log") => "개발용 로그",
        ("mail", "transport", "smtp") => "SMTP 서버",
        ("mail", "smtp_security", "starttls") => "STARTTLS(권장)",
        ("mail", "smtp_security", "tls") => "암시적 TLS",
        ("mail", "smtp_security", "none") => "암호화 없음",
        ("notifications", "default_email", "off") => "끄기",
        ("notifications", "default_email", "participating") => "참여한 스레드",
        ("notifications", "default_email", "all") => "구독한 모든 스레드",
        ("imports", "author_mapping", "none") => "원래 작성자를 텍스트로 유지",
        ("imports", "author_mapping", "email") => "인증된 이메일로 연결",
        ("imports", "author_mapping", "email_or_username") => "인증된 이메일 또는 사용자 이름으로 연결",
        ("provider", "kind", "oidc") => "OpenID Connect",
        ("provider", "kind", "github") => "GitHub / GitHub Enterprise",
        ("provider", "allow_registration", "true") => "인스턴스 가입 정책 따르기",
        ("provider", "allow_registration", "false") => "기존 계정만",
        ("provider", "allow_registration", "always") => "가입이 닫혀 있어도 계정 생성 허용",
        _ => return None,
    })
}
