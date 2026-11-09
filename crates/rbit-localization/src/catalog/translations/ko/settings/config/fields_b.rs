//! Korean labels and help of the configuration fields in `catalog/settings/config/fields_b.rs`.

/// The label and help of `key` in `section`, or `None` when this table lacks it.
#[allow(clippy::too_many_lines, reason = "one arm per configuration field")]
pub(super) fn field(section: &str, key: &str) -> Option<(&'static str, &'static str)> {
    Some(match (section, key) {
        ("webhooks", "enabled") => ("이 기능 사용", "웹훅을 전송합니다. 끄면 웹훅을 구성할 수는 있지만 아무것도 전송되지 않습니다."),
        ("webhooks", "allow_private_addresses") => {
            ("사설 네트워크 대상 허용", "루프백, 사설, 링크 로컬 또는 고유 로컬 주소로 확인되는 대상을 허용합니다.")
        }
        ("webhooks", "timeout_seconds") => ("시간 제한(초)", "전송 시도 한 번(연결, 전송, 응답 상태 읽기)의 최대 시간입니다."),
        ("webhooks", "max_attempts") => ("최대 시도 횟수", "전송 하나가 실패로 표시되기 전까지의 시도 횟수이며, 첫 시도를 포함합니다."),
        ("webhooks", "concurrency") => ("동시 실행 수", "인스턴스 전체에서 동시에 진행되는 전송 수입니다."),
        ("webhooks", "delivery_retention_days") => ("전송 기록 보관 기간(일)", "전송 로그에 요청과 응답을 보관하는 기간(일)입니다."),
        ("releases", "max_asset_bytes") => ("최대 에셋 크기(바이트)", "첨부할 수 있는 파일의 최대 크기(바이트)입니다."),
        ("releases", "max_assets_per_release") => ("릴리스당 최대 에셋 수", "릴리스 하나에 첨부할 수 있는 파일 수입니다."),
        ("releases", "upload_timeout_seconds") => ("업로드 시간 제한(초)", "업로드 하나의 최대 시간(초)입니다."),
        ("mirrors", "enabled") => ("이 기능 사용", "미러 생성을 허용하고 예약된 페치를 실행합니다."),
        ("mirrors", "default_interval_minutes") => ("기본 간격(분)", "새 미러의 페치 간격입니다."),
        ("mirrors", "min_interval_minutes") => ("최소 간격(분)", "미러에 설정할 수 있는 가장 짧은 간격입니다."),
        ("mirrors", "fetch_timeout_seconds") => ("페치 시간 제한(초)", "페치 하나의 최대 시간입니다."),
        ("mirrors", "allow_private_addresses") => {
            ("사설 네트워크 대상 허용", "루프백, 사설, 링크 로컬 또는 고유 로컬 주소로 확인되는 업스트림 URL을 허용합니다.")
        }
        ("push_mirrors", "enabled") => ("이 기능 사용", "푸시 미러 구성과 푸시 미러로의 푸시를 허용합니다."),
        ("push_mirrors", "interval_minutes") => ("간격(분)", "모든 미러에 최소한 이 간격마다 푸시합니다."),
        ("push_mirrors", "debounce_seconds") => ("디바운스(초)", "푸시 후 미러에 푸시하기까지의 지연 시간입니다."),
        ("push_mirrors", "push_timeout_seconds") => ("푸시 시간 제한(초)", "git push 하나의 최대 시간입니다."),
        ("push_mirrors", "allow_private_addresses") => {
            ("사설 네트워크 대상 허용", "루프백, 사설, 링크 로컬 또는 고유 로컬 주소로 확인되는 원격을 허용합니다.")
        }
        ("push_mirrors", "allow_local_urls") => {
            ("로컬 파일 원격 허용(테스트 전용)", "file:// 원격을 허용합니다(테스트 전용, 모듈 문서 참조).")
        }
        ("imports", "enabled") => ("이 기능 사용", "“새 저장소” 페이지에서 가져오기를 제공합니다."),
        ("imports", "allow_private_addresses") => {
            ("사설 네트워크 대상 허용", "루프백, 사설, 링크 로컬 또는 고유 로컬 주소로 확인되는 소스를 허용합니다.")
        }
        ("imports", "max_concurrent") => ("최대 동시 실행 수", "동시에 실행되는 가져오기 수입니다."),
        ("imports", "git_timeout_seconds") => ("Git 시간 제한(초)", "Git 복사의 최대 시간입니다."),
        ("imports", "api_timeout_seconds") => ("API 시간 제한(초)", "API 요청 하나의 최대 시간입니다."),
        ("imports", "rate_limit_max_wait_seconds") => {
            ("속도 제한 최대 대기 시간(초)", "소스의 속도 제한이 초기화되기를 기다리는 최대 시간입니다.")
        }
        ("imports", "author_mapping") => ("가져온 작성자 연결", "소스의 작성자를 로컬 계정에 연결하는 방식입니다."),
        ("issues", "scan_commit_limit") => {
            ("커밋 검사 한도", "푸시된 참조 업데이트마다 참조와 닫기 키워드를 찾기 위해 검사하는 커밋 수입니다.")
        }
        ("issues", "notifications") => ("알림", "앱 내 알림을 생성하고 표시할지 여부입니다."),
        ("issues", "notification_retention_days") => ("알림 보관 기간(일)", "읽은 알림을 보관하는 기간(일)입니다."),
        ("pulls", "merge_timeout_seconds") => (
            "병합 시간 제한(초)",
            "병합 또는 시험 병합 하나의 최대 시간(초)입니다. 임시 저장소 생성부터 베이스 저장소의 페치와 참조 업데이트까지 모든 git 작업이 포함됩니다.",
        ),
        ("pulls", "max_concurrent_merges") => {
            ("최대 동시 병합 수", "인스턴스 전체에서 동시에 실행되는 병합 수입니다(베이스 저장소당 항상 하나).")
        }
        ("pulls", "mergeability_concurrency") => {
            ("병합 가능 여부 확인 동시 실행 수", "병합 가능 여부를 확인하는 시험 병합의 동시 실행 수입니다.")
        }
        ("pulls", "max_commits") => ("최대 커밋 수", "풀 리퀘스트 하나에 표시되는 커밋 수입니다."),
        ("pulls", "noreply_domain") => (
            "회신 불가 이메일 도메인",
            "생성된 커밋에서 회신 불가 작성자에 사용하는 도메인입니다. 비워 두면 noreply 뒤에 공개 호스트 이름을 붙여 사용합니다.",
        ),
        ("editor", "max_upload_bytes") => ("최대 업로드 크기(바이트)", "업로드할 수 있는 파일의 최대 크기(바이트)입니다."),
        ("editor", "max_upload_files") => ("최대 업로드 파일 수", "업로드 한 번에 올릴 수 있는 파일 수입니다."),
        ("editor", "upload_timeout_seconds") => ("업로드 시간 제한(초)", "업로드 요청 하나의 최대 시간(초)입니다."),
        ("lfs", "enabled") => ("이 기능 사용", "LFS API와 SSH git-lfs-authenticate 명령을 제공합니다."),
        ("lfs", "max_object_bytes") => ("최대 객체 크기(바이트)", "객체의 최대 크기(바이트)입니다."),
        ("lfs", "token_lifetime_seconds") => ("토큰 유효 기간(초)", "서명된 전송 헤더의 유효 기간(초)입니다."),
        ("actions", "enabled") => (
            "이 기능 사용",
            "rbit-runner에서 워크플로를 실행합니다. 끄면 Actions가 완전히 비활성화되어 페이지, 러너 엔드포인트, 백그라운드 작업이 모두 사라지고 워크플로 파일은 무시됩니다. 다시 시작해야 적용됩니다.",
        ),
        ("actions", "enable_for_new_repositories") => {
            ("새 저장소에 사용", "지금부터 만드는 저장소에 Actions를 켭니다. 기존 저장소는 자체 설정을 유지합니다.")
        }
        ("actions", "secrets_key_file") => (
            "시크릿 키 파일",
            "Actions 시크릿의 암호화 키입니다. 비워 두면 데이터 디렉터리의 actions/secrets.key를 사용합니다. 별도로 백업하세요.",
        ),
        ("actions", "log_retention_days") => ("로그 보관 기간(일)", "작업 로그를 보관하는 기간(일)입니다."),
        ("actions", "run_retention_days") => ("실행 보관 기간(일)", "완료된 실행을 보관하는 기간(일)입니다."),
        ("actions", "max_log_bytes") => ("최대 로그 크기(바이트)", "태스크당 보관하는 로그 크기이며, 이후의 줄은 버려집니다."),
        ("actions", "max_workflow_bytes") => ("최대 워크플로 파일 크기(바이트)", "읽어 들이는 워크플로 파일의 최대 크기(바이트)입니다."),
        ("actions", "zombie_timeout_seconds") => ("하트비트 시간 제한(초)", "이 시간 동안 하트비트가 없는 실행 중 태스크는 실패합니다."),
        ("actions", "endless_timeout_seconds") => ("태스크 시간 제한(초)", "이 시간 동안 실행된 태스크는 실패합니다."),
        ("actions", "abandoned_timeout_seconds") => {
            ("대기열 시간 제한(초)", "이 시간 동안 어떤 러너도 가져가지 않은 대기 중인 작업은 취소됩니다.")
        }
        ("actions", "evaluation_concurrency") => {
            ("평가 동시 실행 수", "동시에 실행되는 워크플로 평가(Git 읽기와 YAML 파싱) 수로, 1–16입니다.")
        }
        ("actions", "instance_runners_for_all") => (
            "인스턴스 러너를 모두에게 제공",
            "인스턴스 러너가 모든 소유자의 작업을 실행하게 합니다. 끄면 관리자가 허용한 소유자의 작업만 실행합니다.",
        ),
        ("actions", "max_log_storage_bytes") => (
            "로그 저장 공간 예산(바이트)",
            "인스턴스 전체에 보관하는 로그 크기입니다. 이를 넘으면 가장 오래된 완료된 실행의 로그부터 삭제됩니다.",
        ),
        ("actions", "max_waiting_jobs_per_repository") => {
            ("저장소당 최대 대기 작업 수", "대기 중인 작업이 이 수를 넘으면 새 실행은 읽을 수 있는 오류와 함께 실패합니다.")
        }
        ("actions", "max_runs_per_event") => ("이벤트당 최대 실행 수", "푸시 또는 풀 리퀘스트 이벤트 하나가 시작할 수 있는 실행 수입니다."),
        ("actions", "max_jobs_per_event") => ("이벤트당 최대 작업 수", "이벤트 하나가 모든 실행에 걸쳐 만들 수 있는 작업 수입니다."),
        ("actions", "max_schedules_per_repository") => ("저장소당 최대 예약 수", "저장소당 예약된 워크플로 항목 수입니다."),
        ("actions", "long_poll_seconds") => ("러너 롱 폴링(초)", "러너의 작업 폴링을 열어 두는 시간으로, 1–25입니다."),
        ("actions", "max_polls") => {
            ("최대 러너 폴링 수", "동시에 열어 두는 러너 폴링 수입니다. 이를 넘으면 하나가 끝날 때까지 폴링이 거부됩니다.")
        }
        ("actions", "max_artifact_bytes") => ("최대 아티팩트 크기(바이트)", "작업이 업로드할 수 있는 아티팩트의 최대 크기입니다."),
        ("actions", "max_artifact_storage_bytes") => (
            "저장소당 아티팩트 저장 공간(바이트)",
            "저장소당 보관하는 아티팩트 크기입니다. 이를 넘는 업로드는 거부됩니다. 백업(형식 5 이상)에는 아티팩트가 포함됩니다.",
        ),
        ("actions", "artifact_retention_days") => {
            ("아티팩트 보관 기간(일)", "아티팩트를 보관하는 기간(일)입니다. 워크플로의 retention-days로는 이 값을 줄이기만 할 수 있습니다.")
        }
        ("actions", "max_cache_storage_bytes") => (
            "저장소당 캐시 저장 공간(바이트)",
            "저장소의 모든 러너가 사용하는 actions/cache 데이터의 저장소당 보관 크기입니다. 가장 오랫동안 사용되지 않은 항목부터 삭제됩니다. 백업(형식 5 이상)에는 캐시 항목이 포함됩니다.",
        ),
        ("actions", "cache_retention_days") => ("캐시 보관 기간(일)", "복원되지 않은 캐시 항목을 보관하는 기간(일)입니다."),
        ("provider", "kind") => ("공급자 유형", "서버를 다시 시작한 후 적용되는 배포 설정입니다."),
        ("provider", "id") => ("공급자 ID", "콜백 URL에 사용하는 고유한 짧은 ID입니다. 변경하면 공급자 시크릿을 다시 입력해야 합니다."),
        ("provider", "name") => ("표시 이름", "버튼과 설정에 표시됩니다."),
        ("provider", "issuer") => ("OIDC 발급자 URL", "OIDC 발급자 URL입니다(kind = \"oidc\" 전용)."),
        ("provider", "base_url") => {
            ("GitHub Enterprise URL", "GitHub Enterprise Server URL입니다(kind = \"github\" 전용). 기본값은 https://github.com입니다.")
        }
        ("provider", "client_id") => ("클라이언트 ID", "서버를 다시 시작한 후 적용되는 배포 설정입니다."),
        ("provider", "client_secret") => ("클라이언트 시크릿", "로그에 기록되거나 표시되지 않습니다(Debug 구현 참조)."),
        ("provider", "scopes") => {
            ("요청 범위", "한 줄에 범위를 하나씩 입력하세요. 공급자가 권장하는 범위를 요청하려면 공급자 기본값을 사용하세요.")
        }
        ("provider", "allow_registration") => ("계정 생성", "서버를 다시 시작한 후 적용되는 배포 설정입니다."),
        ("provider", "trust_email") => (
            "인증된 이메일 주소 신뢰",
            "첫 로그인을 공급자의 인증된 이메일과 같은 인증된 이메일을 가진 단 하나의 계정에 연결합니다. 사용자의 주소를 관리하는 공급자에만 사용하세요.",
        ),
        ("provider", "trust_mfa") => (
            "공급자의 다단계 인증 신뢰",
            "ID 토큰의 amr 클레임에 공급자가 다단계 인증(\"mfa\")을 수행했다고 표시되면 계정 자체의 2단계 인증 단계를 건너뜁니다. OIDC 전용입니다.",
        ),
        _ => return None,
    })
}
