//! Japanese (`ja`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};

entries! { Ja:
    WebhookMessage => MessageWriter |text| {
        WebhookMessage::TickToConfirm => text.literal("確認のためチェックボックスをオンにしてください。"),
        WebhookMessage::Settings => text.literal("設定"),
        WebhookMessage::Webhooks => text.literal("Webhook"),
        WebhookMessage::IntroSigned => text.literal("は、署名付きの"),
        WebhookMessage::IntroPayload => text.literal("リクエストを JSON ペイロード付きで各 URL に送信します。送信されるのは、購読しているイベントが次で発生したときです："),
        WebhookMessage::IntroOrgTail => text.literal("およびそのすべてのリポジトリ"),
        WebhookMessage::IntroTail => text.literal("。"),
        WebhookMessage::AddWebhook => text.literal("Webhook を追加"),
        WebhookMessage::NoWebhooks => text.literal("Webhook はありません"),
        WebhookMessage::NoWebhooksText => text.literal("Webhook を追加すると、プッシュやリリースなどを CI サーバー、チャットボット、その他のサービスに通知できます。"),
        WebhookMessage::Inactive => text.literal("無効"),
        WebhookMessage::NoDeliveries => text.literal("配信はまだありません"),
        WebhookMessage::SendPing => text.literal("Ping を送信"),
        WebhookMessage::NewWebhook => text.literal("新しい Webhook"),
        WebhookMessage::PingLead => text.literal("Webhook を保存すると、すぐに"),
        WebhookMessage::PingTail => text.literal("イベントが送信されます。"),
        WebhookMessage::Saved => text.literal("Webhook を保存しました。"),
        WebhookMessage::PingQueued => text.literal("Ping をキューに追加しました。まもなく最近の配信に表示されます。"),
        WebhookMessage::PayloadUrl => text.literal("ペイロード URL"),
        WebhookMessage::UrlPlaceholder => text.literal("https://example.com/hooks/rbit"),
        WebhookMessage::PayloadsAre => text.literal("ペイロードの形式は"),
        WebhookMessage::NoRedirects => text.literal("です。リダイレクトはたどりません。"),
        WebhookMessage::Secret => text.literal("シークレット"),
        WebhookMessage::Optional => text.literal("任意"),
        WebhookMessage::SecretConfigured => text.literal("設定済み — 変更しない場合は空欄のままにしてください"),
        WebhookMessage::SignsLead => text.literal("すべてのペイロードに署名します："),
        WebhookMessage::SignsAnd => text.literal("と"),
        WebhookMessage::SignsCarry => text.literal("には"),
        WebhookMessage::SignsTail => text.literal("に続けて本文の HMAC-SHA256 が入ります。シークレットは再表示できません。"),
        WebhookMessage::RemoveSecret => text.literal("シークレットを削除"),
        WebhookMessage::Events => text.literal("イベント"),
        WebhookMessage::Active => text.literal("有効"),
        WebhookMessage::ActiveText => text.literal("この URL にイベントを配信します。無効な Webhook も設定とログは保持されます。"),
        WebhookMessage::SaveWebhook => text.literal("Webhook を保存"),
        WebhookMessage::RecentDeliveries => text.literal("最近の配信"),
        WebhookMessage::NoDeliveriesText => text.literal("イベントが発生すると、ここに配信が表示されます。URL を試すには Ping を送信してください。"),
        WebhookMessage::DangerZone => text.literal("危険な操作"),
        WebhookMessage::DeleteTitle => text.literal("この Webhook を削除"),
        WebhookMessage::DeleteConfirm => text.literal("この Webhook が削除されることを理解しました"),
        WebhookMessage::DeleteWebhook => text.literal("Webhook を削除"),
        WebhookMessage::Webhook => text.literal("Webhook"),
        WebhookMessage::Redeliver => text.literal("再配信"),
        WebhookMessage::Request => text.literal("リクエスト"),
        WebhookMessage::NotSent => text.literal("まだ送信されていません。"),
        WebhookMessage::Response => text.literal("レスポンス"),
        WebhookMessage::NoResponse => text.literal("レスポンスを受信しませんでした。"),
        WebhookMessage::EmptyBody => text.literal("本文は空でした。"),
        WebhookMessage::Retrying => text.literal("再試行中"),
        WebhookMessage::Now => text.literal("今すぐ"),
    }
    WebhookText<'a> => MessageWriter |text| {
        WebhookText::MaxRepository(max) => {
            text.literal("このリポジトリの Webhook は上限の ");
            text.number(max);
            text.literal(" 件に達しています。追加するには、いずれかを削除してください。");
        }
        WebhookText::MaxOrganization(max) => {
            text.literal("この組織の Webhook は上限の ");
            text.number(max);
            text.literal(" 件に達しています。追加するには、いずれかを削除してください。");
        }
        WebhookText::StopsDeliveries(host) => {
            text.argument(host);
            text.literal(" への配信を停止し、配信ログを削除します。元に戻すことはできません。");
        }
        WebhookText::RetryingCode(code) => {
            text.literal("再試行中 · ");
            text.number(code);
        }
        WebhookText::Created(when) => {
            text.literal("作成：");
            text.argument(when);
        }
        WebhookText::OneAttempt(count) => {
            text.number(count);
            text.literal(" 回試行");
        }
        WebhookText::Attempts(count) => {
            text.number(count);
            text.literal(" 回試行");
        }
        WebhookText::NextAttempt(when) => {
            text.literal("次回の試行：");
            text.argument(when);
        }
        WebhookText::ListTitle(name) => {
            text.literal("Webhook · ");
            text.argument(name);
        }
        WebhookText::NewTitle(name) => {
            text.literal("新しい Webhook · ");
            text.argument(name);
        }
        WebhookText::EditTitle(name) => {
            text.literal("Webhook · ");
            text.argument(name);
        }
        WebhookText::DeliveryTitle(guid, name) => {
            text.literal("配信 ");
            text.argument(guid);
            text.literal(" · ");
            text.argument(name);
        }
    }
}
