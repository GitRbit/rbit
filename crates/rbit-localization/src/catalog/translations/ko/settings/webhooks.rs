//! Korean (`ko`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::WebhookSentence;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};

entries! { Ko:
    WebhookMessage => MessageWriter |text| {
        WebhookMessage::TickToConfirm => text.literal("확인하려면 체크 상자를 선택하세요."),
        WebhookMessage::Settings => text.literal("설정"),
        WebhookMessage::Webhooks => text.literal("웹훅"),
        WebhookMessage::AddWebhook => text.literal("웹훅 추가"),
        WebhookMessage::NoWebhooks => text.literal("웹훅 없음"),
        WebhookMessage::NoWebhooksText => text.literal("웹훅을 추가하여 푸시, 릴리스 등의 이벤트를 CI 서버, 채팅 봇 또는 다른 서비스에 알리세요."),
        WebhookMessage::Inactive => text.literal("비활성"),
        WebhookMessage::NoDeliveries => text.literal("아직 전송 내역이 없습니다"),
        WebhookMessage::SendPing => text.literal("핑 보내기"),
        WebhookMessage::NewWebhook => text.literal("새 웹훅"),
        WebhookMessage::PingLead => text.literal("웹훅을 저장하면 바로"),
        WebhookMessage::PingTail => text.literal("이벤트가 전송됩니다."),
        WebhookMessage::Saved => text.literal("웹훅을 저장했습니다."),
        WebhookMessage::PingQueued => text.literal("핑을 대기열에 추가했습니다. 잠시 후 최근 전송 내역에 표시됩니다."),
        WebhookMessage::PayloadUrl => text.literal("페이로드 URL"),
        WebhookMessage::UrlPlaceholder => text.literal("https://example.com/hooks/rbit"),
        WebhookMessage::PayloadsAre => text.literal("페이로드 형식은"),
        WebhookMessage::NoRedirects => text.literal("입니다. 리디렉션은 따르지 않습니다."),
        WebhookMessage::Secret => text.literal("시크릿"),
        WebhookMessage::Optional => text.literal("선택 사항"),
        WebhookMessage::SecretConfigured => text.literal("설정됨 — 유지하려면 비워 두세요"),
        WebhookMessage::SignsLead => text.literal("모든 페이로드에 서명합니다:"),
        WebhookMessage::SignsAnd => text.literal("및"),
        WebhookMessage::SignsCarry => text.literal("헤더에"),
        WebhookMessage::SignsTail => text.literal("접두사와 본문의 HMAC-SHA256 값이 담깁니다. 시크릿은 다시 표시되지 않습니다."),
        WebhookMessage::RemoveSecret => text.literal("시크릿 제거"),
        WebhookMessage::Events => text.literal("이벤트"),
        WebhookMessage::Active => text.literal("활성"),
        WebhookMessage::ActiveText => text.literal("이 URL로 이벤트를 전송합니다. 비활성 웹훅은 설정과 로그를 유지합니다."),
        WebhookMessage::SaveWebhook => text.literal("웹훅 저장"),
        WebhookMessage::RecentDeliveries => text.literal("최근 전송 내역"),
        WebhookMessage::NoDeliveriesText => text.literal("이벤트가 발생하면 전송 내역이 여기에 표시됩니다. URL을 시험하려면 핑을 보내세요."),
        WebhookMessage::DangerZone => text.literal("위험 구역"),
        WebhookMessage::DeleteTitle => text.literal("이 웹훅 삭제"),
        WebhookMessage::DeleteConfirm => text.literal("이 웹훅이 삭제된다는 것을 이해했습니다"),
        WebhookMessage::DeleteWebhook => text.literal("웹훅 삭제"),
        WebhookMessage::Webhook => text.literal("웹훅"),
        WebhookMessage::Redeliver => text.literal("다시 전송"),
        WebhookMessage::Request => text.literal("요청"),
        WebhookMessage::NotSent => text.literal("아직 전송되지 않았습니다."),
        WebhookMessage::Response => text.literal("응답"),
        WebhookMessage::NoResponse => text.literal("응답을 받지 못했습니다."),
        WebhookMessage::EmptyBody => text.literal("본문이 비어 있습니다."),
        WebhookMessage::Retrying => text.literal("재시도 중"),
        WebhookMessage::Now => text.literal("지금"),
    }
    WebhookText<'a> => MessageWriter |text| {
        WebhookText::MaxRepository(max) => {
            text.literal("이 저장소는 최대 웹훅 수(");
            text.number(max);
            text.literal("개)에 도달했습니다. 다른 웹훅을 추가하려면 하나를 삭제하세요.");
        }
        WebhookText::MaxOrganization(max) => {
            text.literal("이 조직은 최대 웹훅 수(");
            text.number(max);
            text.literal("개)에 도달했습니다. 다른 웹훅을 추가하려면 하나를 삭제하세요.");
        }
        WebhookText::StopsDeliveries(host) => {
            text.literal("호스트 ");
            text.argument(host);
            text.literal("(으)로의 전송을 중단하고 전송 로그를 삭제합니다. 되돌릴 수 없습니다.");
        }
        WebhookText::RetryingCode(code) => {
            text.literal("재시도 중 · ");
            text.number(code);
        }
        WebhookText::Created(when) => {
            text.literal("생성: ");
            text.argument(when);
        }
        WebhookText::OneAttempt(count) => {
            text.number(count);
            text.literal("회 시도");
        }
        WebhookText::Attempts(count) => {
            text.number(count);
            text.literal("회 시도");
        }
        WebhookText::NextAttempt(when) => {
            text.literal("다음 시도: ");
            text.argument(when);
        }
        WebhookText::ListTitle(name) => {
            text.literal("웹훅 · ");
            text.argument(name);
        }
        WebhookText::NewTitle(name) => {
            text.literal("새 웹훅 · ");
            text.argument(name);
        }
        WebhookText::EditTitle(name) => {
            text.literal("웹훅 · ");
            text.argument(name);
        }
        WebhookText::DeliveryTitle(guid, name) => {
            text.literal("전송 ");
            text.argument(guid);
            text.literal(" · ");
            text.argument(name);
        }
    }
    WebhookSentence => RichWriter |text| {
        WebhookSentence::Intro { organization } => {
            text.slot(0);
            text.literal(" 인스턴스는 구독한 이벤트가 발생하면 각 URL로 서명된 ");
            text.slot(1);
            text.literal(" 요청(JSON 페이로드 포함)을 보냅니다. 대상: ");
            text.slot(2);
            if organization {
                text.literal(" 및 이 조직의 모든 저장소");
            }
            text.literal(".");
        }
    }
}
