# Korean (ko) glossary

Terms used by the Korean tables under
`crates/rbit-localization/src/catalog/translations/ko/`. One Korean term per concept;
add a row here before introducing a new one. The terminology follows the Korean UIs of
mainstream developer tools (GitHub in particular).

## Style

- Messages addressed to the user use the polite 합니다/하세요 style
  ("저장소를 찾을 수 없습니다.", "다시 시도하세요.").
- Buttons, tabs, headings and labels are concise noun phrases ("저장소 만들기",
  "변경 사항 저장", "브랜치 삭제"); state badges use the 됨/중 forms ("병합됨", "대기 중").
- Normal Korean spacing (띄어쓰기) and Western punctuation. Quotation marks are “…”.
  The ellipsis is the single character `…`.
- After an interpolated name or number, avoid particles whose form depends on the final
  consonant. Rephrase (“{name} 님이 …”, “… {count}개”, “이름: {name}”), or use the
  combined forms 을(를), 이(가), 은(는), 와(과), (으)로.
- People are referred to as “{name} 님”.
- Counts use counters without plural marking: “{count}개”, “{count}명”, “{count}건”,
  “{count}번째”.
- Never translated: rbit, Git, GitHub, Git LFS, SSH, GPG, HTTPS, SHA-256, OIDC,
  OpenID Connect, Markdown, README, YAML, JSON, URL, API, REST, CI/CD, command names,
  file names, keys and other machine values.

## Terms

| English | Korean |
| --- | --- |
| account | 계정 |
| action (Actions feature) | Actions |
| activity | 활동 |
| administrator / site administrator | 관리자 / 사이트 관리자 |
| approve / approved | 승인 / 승인됨 |
| archive / archived | 보관 / 보관됨 |
| artifact | 아티팩트 |
| assignee | 담당자 |
| author | 작성자 |
| avatar | 아바타 |
| base / head (branch) | 베이스 / 헤드 |
| blame | blame |
| branch | 브랜치 |
| branch protection / protected branch | 브랜치 보호 / 보호된 브랜치 |
| cancel / cancelled | 취소 / 취소됨 |
| check (CI) / status check | 검사 / 상태 검사 |
| clone | 클론 |
| close / closed | 닫기 / 닫힘 |
| collaborator | 공동 작업자 |
| comment | 댓글 |
| commit | 커밋 |
| conflict | 충돌 |
| contribution / contributor | 기여 / 기여자 |
| conversation | 대화 |
| create | 만들기 (buttons), 생성 (nouns, “생성됨”) |
| dashboard | 대시보드 |
| default branch | 기본 브랜치 |
| delete | 삭제 |
| deploy key | 배포 키 |
| deployment | 배포 |
| description | 설명 |
| diff | diff |
| directory / folder | 디렉터리 / 폴더 |
| dismiss (review) | 기각 |
| draft | 초안 |
| edit | 편집 |
| email | 이메일 |
| environment | 환경 |
| explore | 탐색 |
| fork | 포크 |
| force-push | 강제 푸시 |
| history | 기록 |
| inbox | 받은 알림함 |
| issue | 이슈 |
| job (workflow) | 작업 |
| label | 레이블 |
| log | 로그 |
| member | 구성원 |
| merge / merged | 병합 / 병합됨 |
| merge commit / squash / rebase | 병합 커밋 / 스쿼시 / 리베이스 |
| milestone | 마일스톤 |
| mirror / pull mirror / push mirror | 미러 / 풀 미러 / 푸시 미러 |
| notification | 알림 |
| open (state) / reopen | 열림 / 다시 열기 |
| organization | 조직 |
| outdated | 오래됨 |
| owner | 소유자 |
| passkey / security key | 패스키 / 보안 키 |
| password | 비밀번호 |
| pending | 대기 중 |
| permission | 권한 |
| personal access token / access token | 개인 액세스 토큰 / 액세스 토큰 |
| private / public / internal | 비공개 / 공개 / 내부 |
| profile | 프로필 |
| pull request | 풀 리퀘스트 |
| push | 푸시 |
| recovery code | 복구 코드 |
| release | 릴리스 |
| repository | 저장소 |
| request changes | 변경 요청 |
| resolve (conversation) | 해결 |
| review / reviewer | 리뷰 / 리뷰어 |
| revision (wiki) | 리비전 |
| run (workflow) | 실행 |
| runner | 러너 |
| save | 저장 |
| scope (token) | 범위 |
| secret | 시크릿 |
| settings | 설정 |
| sign in / sign out / sign up | 로그인 / 로그아웃 / 가입 |
| SSH key / GPG key | SSH 키 / GPG 키 |
| star / starred / stargazers | 스타 / 스타 표시함 / 스타를 준 사용자 |
| step (workflow) | 단계 |
| subscribe / unsubscribe | 구독 / 구독 취소 |
| tag | 태그 |
| team | 팀 |
| theme / appearance | 테마 / 모양 |
| topic | 토픽 |
| transfer (ownership) | 이전 (소유권 이전) |
| two-factor authentication | 2단계 인증 |
| upload / download | 업로드 / 다운로드 |
| username | 사용자 이름 |
| variable | 변수 |
| visibility | 공개 범위 |
| webhook | 웹훅 |
| wiki | 위키 |
| workflow | 워크플로 |

## Further terms

| English | Korean |
| --- | --- |
| ahead / behind | 앞섬 / 뒤처짐 |
| assigned | 할당됨 |
| audit log | 감사 로그 |
| authenticator app / authentication code | 인증 앱 / 인증 코드 |
| bio / website / location | 소개 / 웹사이트 / 위치 |
| bot | 봇 |
| branding / favicon / footer | 브랜딩 / 파비콘 / 바닥글 |
| breadcrumb | 이동 경로 |
| built in (themes) | 기본 제공 |
| bypass | 우회 |
| colour | 색상 |
| commit message / extended description | 커밋 메시지 / 자세한 설명 |
| committer | 커미터 |
| configuration (instance) | 구성 |
| credentials / client secret / issuer | 자격 증명 / 클라이언트 시크릿 / 발급자 |
| danger zone | 위험 구역 |
| delivery (webhook) / redeliver | 전송 / 다시 전송 |
| digest (email) | 요약 |
| display name | 표시 이름 |
| due date | 마감일 |
| enable / disable | 활성화 / 비활성화 |
| ephemeral (runner) | 일회용 |
| expiration / expires | 만료 |
| fetch | 페치 (always; “가져오기” is reserved for import) |
| files changed | 변경된 파일 |
| generate (token, codes) | 생성 |
| identity provider / sign-in provider | ID 공급자 / 로그인 공급자 |
| import | 가져오기 |
| instance | 인스턴스 |
| leave (organization, repository) | 나가기 |
| link / unlink (account) | 연결 / 연결 해제 |
| lock / unlock conversation | 대화 잠그기 / 대화 잠금 해제 |
| maintenance / metrics | 유지 관리 / 메트릭 |
| mark as read / unread | 읽음으로 표시 / 읽지 않음 |
| mention | 멘션 (in timeline sentences: 언급했습니다) |
| merge base | 병합 베이스 |
| not planned | 계획 없음 |
| online / offline | 온라인 / 오프라인 |
| optional | 선택 사항 |
| override (configuration) | 재정의 |
| parent (commit) | 부모 |
| payload / ping | 페이로드 / 핑 |
| pattern (branch) | 패턴 |
| pre-release | 사전 릴리스 |
| preview | 미리 보기 |
| rate limiting / timeout | 속도 제한 / 시간 제한 |
| raw | 원본 |
| read / read and write (permission) | 읽기 / 읽기 및 쓰기 |
| ready for review | 리뷰 준비 완료 |
| ref | 참조 |
| registration token / setup token | 등록 토큰 / 설정 토큰 |
| reject | 거부 |
| remove | 제거 (delete stays 삭제) |
| restart (server) | 다시 시작 |
| restore | 복원 |
| revoke / revoked | 폐기 / 폐기됨 (cancel stays 취소) |
| role | 역할 |
| second factor | 두 번째 인증 수단 |
| self-hosting | 셀프 호스팅 |
| session | 세션 |
| sort | 정렬 (최신순, 오래된순, …) |
| squash and merge / rebase and merge | 스쿼시 후 병합 / 리베이스 후 병합 |
| submodule / binary file | 서브모듈 / 바이너리 파일 |
| suspend / suspended / lift suspension | 정지 / 정지됨 / 정지 해제 |
| thread (notification) | 스레드 |
| trusted authors / stop trusting | 신뢰할 수 있는 작성자 / 신뢰 해제 |
| unarchive | 보관 해제 |
| upstream / remote | 업스트림 / 원격 저장소 |
| wait timer / required reviewers | 대기 타이머 / 필수 리뷰어 |

## Split sentences

Lead/Tail fragments are written so that the markup between them reads naturally in
Korean. A tail that cannot follow a name without a particle uses 을(를); a
“Type … to confirm” pair becomes “확인하려면 다음을 입력하세요:” followed by the name, or
“확인하려면 {name} 입력”, depending on how the template splits it. Fragments never start
or end with a space unless the English one does.
