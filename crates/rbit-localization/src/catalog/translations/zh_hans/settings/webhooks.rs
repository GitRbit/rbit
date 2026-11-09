//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};

entries! { ZhHans:
    WebhookMessage => MessageWriter |text| {
        WebhookMessage::TickToConfirm => text.literal("请勾选复选框以确认。"),
        WebhookMessage::Settings => text.literal("设置"),
        WebhookMessage::Webhooks => text.literal("Webhook"),
        WebhookMessage::IntroSigned => text.literal("会在所订阅的事件发生时，向每个 URL 发送一个已签名的"),
        WebhookMessage::IntroPayload => text.literal("请求（附带 JSON 负载），事件来源为"),
        WebhookMessage::IntroOrgTail => text.literal("或其任一仓库"),
        WebhookMessage::IntroTail => text.literal("。"),
        WebhookMessage::AddWebhook => text.literal("添加 Webhook"),
        WebhookMessage::NoWebhooks => text.literal("没有 Webhook"),
        WebhookMessage::NoWebhooksText => {
            text.literal("添加一个 Webhook，在推送、发布发行版等事件发生时通知 CI 服务器、聊天机器人或其他任何服务。");
        }
        WebhookMessage::Inactive => text.literal("未启用"),
        WebhookMessage::NoDeliveries => text.literal("还没有投递"),
        WebhookMessage::SendPing => text.literal("发送 ping"),
        WebhookMessage::NewWebhook => text.literal("新建 Webhook"),
        WebhookMessage::PingLead => text.literal("保存 Webhook 后会立即发送一个"),
        WebhookMessage::PingTail => text.literal("事件。"),
        WebhookMessage::Saved => text.literal("Webhook 已保存。"),
        WebhookMessage::PingQueued => text.literal("ping 已加入队列，稍后会出现在最近的投递中。"),
        WebhookMessage::PayloadUrl => text.literal("负载 URL"),
        WebhookMessage::UrlPlaceholder => text.literal("https://example.com/hooks/rbit"),
        WebhookMessage::PayloadsAre => text.literal("负载格式为"),
        WebhookMessage::NoRedirects => text.literal("。不会跟随重定向。"),
        WebhookMessage::Secret => text.literal("密钥"),
        WebhookMessage::Optional => text.literal("可选"),
        WebhookMessage::SecretConfigured => text.literal("已配置——留空则保持不变"),
        WebhookMessage::SignsLead => text.literal("对每个负载签名："),
        WebhookMessage::SignsAnd => text.literal("和"),
        WebhookMessage::SignsCarry => text.literal("包含"),
        WebhookMessage::SignsTail => text.literal("以及请求体的 HMAC-SHA256。保存后无法再次显示。"),
        WebhookMessage::RemoveSecret => text.literal("移除密钥"),
        WebhookMessage::Events => text.literal("事件"),
        WebhookMessage::Active => text.literal("启用"),
        WebhookMessage::ActiveText => text.literal("将事件投递到此 URL。未启用的 Webhook 会保留其设置和日志。"),
        WebhookMessage::SaveWebhook => text.literal("保存 Webhook"),
        WebhookMessage::RecentDeliveries => text.literal("最近的投递"),
        WebhookMessage::NoDeliveriesText => text.literal("事件发生时，投递记录会显示在这里。发送一个 ping 来测试此 URL。"),
        WebhookMessage::DangerZone => text.literal("危险区域"),
        WebhookMessage::DeleteTitle => text.literal("删除此 Webhook"),
        WebhookMessage::DeleteConfirm => text.literal("我了解此 Webhook 将被删除"),
        WebhookMessage::DeleteWebhook => text.literal("删除 Webhook"),
        WebhookMessage::Webhook => text.literal("Webhook"),
        WebhookMessage::Redeliver => text.literal("重新投递"),
        WebhookMessage::Request => text.literal("请求"),
        WebhookMessage::NotSent => text.literal("尚未发送。"),
        WebhookMessage::Response => text.literal("响应"),
        WebhookMessage::NoResponse => text.literal("没有收到响应。"),
        WebhookMessage::EmptyBody => text.literal("响应体为空。"),
        WebhookMessage::Retrying => text.literal("正在重试"),
        WebhookMessage::Now => text.literal("现在"),
    }
    WebhookText<'a> => MessageWriter |text| {
        WebhookText::MaxRepository(max) => {
            text.literal("此仓库的 Webhook 已达上限 ");
            text.number(max);
            text.literal(" 个。请先删除一个再添加。");
        }
        WebhookText::MaxOrganization(max) => {
            text.literal("此组织的 Webhook 已达上限 ");
            text.number(max);
            text.literal(" 个。请先删除一个再添加。");
        }
        WebhookText::StopsDeliveries(host) => {
            text.literal("将停止向 ");
            text.argument(host);
            text.literal(" 投递，并移除投递日志。此操作无法撤销。");
        }
        WebhookText::RetryingCode(code) => {
            text.literal("正在重试 · ");
            text.number(code);
        }
        WebhookText::Created(when) => {
            text.literal("创建于 ");
            text.argument(when);
        }
        WebhookText::OneAttempt(count) | WebhookText::Attempts(count) => {
            text.number(count);
            text.literal(" 次尝试");
        }
        WebhookText::NextAttempt(when) => {
            text.literal("下次尝试 ");
            text.argument(when);
        }
        WebhookText::ListTitle(name) => {
            text.literal("Webhook · ");
            text.argument(name);
        }
        WebhookText::NewTitle(name) => {
            text.literal("新建 Webhook · ");
            text.argument(name);
        }
        WebhookText::EditTitle(name) => {
            text.literal("Webhook · ");
            text.argument(name);
        }
        WebhookText::DeliveryTitle(guid, name) => {
            text.literal("投递 ");
            text.argument(guid);
            text.literal(" · ");
            text.argument(name);
        }
    }
}
