//! Simplified Chinese labels and help of the configuration fields in
//! `catalog/settings/config/fields_a.rs`: mail to api.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

/// The help text shared by the branding fields.
const SHOWN_TO_VISITORS: &str = "向此实例的访客显示。";
/// The help text of mail fields that only take effect after a restart.
const AFTER_RESTART: &str = "部署设置，重启服务器后生效。";

config_table! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "mail", key: "transport" } => text.literal("传输方式"),
        ConfigText::Help { section: "mail", key: "transport" } => text.literal(AFTER_RESTART),
        ConfigText::Label { section: "mail", key: "from_address" } => text.literal("发件人邮箱地址"),
        ConfigText::Help { section: "mail", key: "from_address" } => text.literal("每封邮件的 From: 地址。"),
        ConfigText::Label { section: "mail", key: "from_name" } => text.literal("发件人显示名称"),
        ConfigText::Help { section: "mail", key: "from_name" } => text.literal("From: 中的显示名称。留空则使用站点名称。"),
        ConfigText::Label { section: "mail", key: "smtp_host" } => text.literal("SMTP 服务器"),
        ConfigText::Help { section: "mail", key: "smtp_host" } => text.literal("SMTP 中继的主机名（transport = \"smtp\"）。"),
        ConfigText::Label { section: "mail", key: "smtp_port" } => text.literal("SMTP 端口"),
        ConfigText::Help { section: "mail", key: "smtp_port" } => {
            text.literal("留空时，STARTTLS 或不加密使用端口 587，隐式 TLS 使用端口 465。");
        }
        ConfigText::Label { section: "mail", key: "smtp_security" } => text.literal("SMTP 加密"),
        ConfigText::Help { section: "mail", key: "smtp_security" } => text.literal(AFTER_RESTART),
        ConfigText::Label { section: "mail", key: "smtp_username" } => text.literal("SMTP 用户名"),
        ConfigText::Help { section: "mail", key: "smtp_username" } => text.literal("中继的凭据。留空表示不进行身份验证。"),
        ConfigText::Label { section: "mail", key: "smtp_password" } => text.literal("SMTP 密码"),
        ConfigText::Help { section: "mail", key: "smtp_password" } => text.literal(AFTER_RESTART),
        ConfigText::Label { section: "mail", key: "timeout_seconds" } => text.literal("超时（秒）"),
        ConfigText::Help { section: "mail", key: "timeout_seconds" } => text.literal("邮件在这么多秒后仍未送达则放弃。"),
        ConfigText::Label { section: "ssh", key: "enabled" } => text.literal("启用此功能"),
        ConfigText::Help { section: "ssh", key: "enabled" } => text.literal("运行内置 SSH 服务器。"),
        ConfigText::Label { section: "ssh", key: "bind" } => text.literal("监听地址"),
        ConfigText::Help { section: "ssh", key: "bind" } => text.literal("SSH 服务器绑定的地址。"),
        ConfigText::Label { section: "ssh", key: "public_host" } => text.literal("公开主机"),
        ConfigText::Help { section: "ssh", key: "public_host" } => {
            text.literal("SSH 克隆 URL 中显示的主机名。默认为 server.public_url 的主机。");
        }
        ConfigText::Label { section: "ssh", key: "public_port" } => text.literal("公开端口"),
        ConfigText::Help { section: "ssh", key: "public_port" } => {
            text.literal("SSH 克隆 URL 中显示的端口，用于代理或端口映射与 bind 不同的情况。");
        }
        ConfigText::Label { section: "ssh", key: "host_key_path" } => text.literal("主机密钥路径"),
        ConfigText::Help { section: "ssh", key: "host_key_path" } => text.literal(
            "SSH 主机密钥文件的路径。留空则使用数据目录中的 ssh/host_ed25519；若不存在，首次启动时会自动生成。",
        ),
        ConfigText::Label { section: "ssh", key: "max_connections" } => text.literal("最大连接数"),
        ConfigText::Help { section: "ssh", key: "max_connections" } => text.literal("所有客户端的并发连接总数；超出的连接会被立即关闭。"),
        ConfigText::Label { section: "ssh", key: "max_connections_per_ip" } => text.literal("每个 IP 的最大连接数"),
        ConfigText::Help { section: "ssh", key: "max_connections_per_ip" } => text.literal("单个 IP 地址的并发连接数。"),
        ConfigText::Label { section: "ssh", key: "auth_timeout_seconds" } => text.literal("身份验证超时（秒）"),
        ConfigText::Help { section: "ssh", key: "auth_timeout_seconds" } => text.literal("客户端连接后完成身份验证的时限（秒）。"),
        ConfigText::Label { section: "ssh", key: "idle_timeout_seconds" } => text.literal("空闲超时（秒）"),
        ConfigText::Help { section: "ssh", key: "idle_timeout_seconds" } => text.literal("连接在没有任何流量多少秒后被关闭。"),
        ConfigText::Label { section: "ssh", key: "max_session_seconds" } => text.literal("最长会话时间（秒）"),
        ConfigText::Help { section: "ssh", key: "max_session_seconds" } => {
            text.literal("单次 Git 操作（如非常大的克隆或推送）的时长上限，以秒为单位。");
        }
        ConfigText::Label { section: "auth", key: "timeout_seconds" } => text.literal("超时（秒）"),
        ConfigText::Help { section: "auth", key: "timeout_seconds" } => text.literal("对提供方单次请求的时长上限。"),
        ConfigText::Label { section: "auth", key: "allow_private_addresses" } => text.literal("允许私有网络目标"),
        ConfigText::Help { section: "auth", key: "allow_private_addresses" } => text.literal(
            "允许地址为回环、私有、链路本地或唯一本地地址的提供方，以及使用纯 http 的签发者或基础 URL。",
        ),
        ConfigText::Label { section: "branding", key: "primary_color" } => text.literal("主色"),
        ConfigText::Help { section: "branding", key: "primary_color" } => {
            text.literal("主按钮背景色，格式为 #RRGGBB。两个颜色要么都设置，要么都留空。");
        }
        ConfigText::Label { section: "branding", key: "primary_text_color" } => text.literal("主文字颜色"),
        ConfigText::Help { section: "branding", key: "primary_text_color" } => {
            text.literal("主按钮文字颜色，格式为 #RRGGBB。请选择易于阅读的对比度。");
        }
        ConfigText::Label { section: "branding", key: "description" } => text.literal("描述"),
        ConfigText::Label { section: "branding", key: "welcome_title" } => text.literal("欢迎标题"),
        ConfigText::Label { section: "branding", key: "welcome_message" } => text.literal("欢迎信息"),
        ConfigText::Label { section: "branding", key: "logo_url" } => text.literal("徽标 URL"),
        ConfigText::Label { section: "branding", key: "favicon_url" } => text.literal("网站图标 URL"),
        ConfigText::Label { section: "branding", key: "app_icon_url" } => text.literal("应用图标 URL"),
        ConfigText::Label { section: "branding", key: "footer_text" } => text.literal("页脚文本"),
        ConfigText::Label { section: "branding", key: "support_url" } => text.literal("支持 URL"),
        ConfigText::Label { section: "branding", key: "terms_url" } => text.literal("服务条款 URL"),
        ConfigText::Label { section: "branding", key: "privacy_url" } => text.literal("隐私政策 URL"),
        ConfigText::Label { section: "branding", key: "show_powered_by" } => text.literal("显示“Powered by rbit”"),
        ConfigText::Label { section: "branding", key: "show_version" } => text.literal("显示版本"),
        ConfigText::Label { section: "branding", key: "show_features" } => text.literal("展示功能"),
        ConfigText::Help { section: "branding", key: "description" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "welcome_title" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "welcome_message" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "logo_url" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "favicon_url" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "app_icon_url" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "footer_text" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "support_url" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "terms_url" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "privacy_url" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "show_powered_by" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "show_version" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Help { section: "branding", key: "show_features" } => text.literal(SHOWN_TO_VISITORS),
        ConfigText::Label { section: "notifications", key: "email" } => text.literal("发送通知邮件"),
        ConfigText::Help { section: "notifications", key: "email" } => text.literal("是否同时通过邮件发送通知。"),
        ConfigText::Label { section: "notifications", key: "default_email" } => text.literal("默认邮件订阅"),
        ConfigText::Help { section: "notifications", key: "default_email" } => text.literal("从未自行选择的用户所使用的偏好。"),
        ConfigText::Label { section: "notifications", key: "digest_hour" } => text.literal("默认摘要发送时间（UTC）"),
        ConfigText::Help { section: "notifications", key: "digest_hour" } => text.literal(
            "为未自行选择时间的用户发送每日和每周邮件摘要的时刻，取值 0 到 23（UTC）。“站点管理 → 设置”会在运行时覆盖此值。",
        ),
        ConfigText::Label { section: "operations", key: "metrics" } => text.literal("启用指标端点"),
        ConfigText::Help { section: "operations", key: "metrics" } => text.literal("在 /metrics 公开 Prometheus 指标。"),
        ConfigText::Label { section: "operations", key: "metrics_token" } => text.literal("指标访问令牌"),
        ConfigText::Help { section: "operations", key: "metrics_token" } => text.literal(
            "设置后，访问 /metrics 需要 Authorization: Bearer <token>。仅当该端点无法从不受信任的网络访问时才可不设置。",
        ),
        ConfigText::Label { section: "operations", key: "warm_caches_on_push" } => text.literal("推送时预热缓存"),
        ConfigText::Help { section: "operations", key: "warm_caches_on_push" } => text.literal(
            "每次推送后预热默认分支的语言和最新提交缓存，这样推送后的第一位访客就不必等待计算。",
        ),
        ConfigText::Label { section: "operations", key: "maintenance_interval_minutes" } => text.literal("维护间隔（分钟）"),
        ConfigText::Help { section: "operations", key: "maintenance_interval_minutes" } => text.literal(
            "两次维护运行之间的分钟数（对上次运行以来有推送的仓库执行 git gc --auto，然后清理洞察缓存）。0 表示禁用定期维护。",
        ),
        ConfigText::Label { section: "operations", key: "job_concurrency" } => text.literal("作业并发数"),
        ConfigText::Help { section: "operations", key: "job_concurrency" } => {
            text.literal("可同时运行的后台作业（缓存预热、git gc）数量上限。");
        }
        ConfigText::Label { section: "operations", key: "insights_cache_max_age_days" } => text.literal("洞察缓存最长保留时间（天）"),
        ConfigText::Help { section: "operations", key: "insights_cache_max_age_days" } => {
            text.literal("维护时会移除早于此期限的洞察缓存条目。0 表示无论多旧都保留。");
        }
        ConfigText::Label { section: "operations", key: "insights_cache_max_entries" } => text.literal("洞察缓存最大条目数"),
        ConfigText::Help { section: "operations", key: "insights_cache_max_entries" } => {
            text.literal("当洞察缓存的条目数超过此值时，维护会移除最旧的条目。0 表示不限制。");
        }
        ConfigText::Label { section: "api", key: "enabled" } => text.literal("启用此功能"),
        ConfigText::Help { section: "api", key: "enabled" } => text.literal("提供 /api/v1。关闭后，所有 API 路径都返回 404。"),
        ConfigText::Label { section: "api", key: "default_page_size" } => text.literal("默认每页条数"),
        ConfigText::Help { section: "api", key: "default_page_size" } => text.literal("请求未指定时每页返回的条目数。"),
        ConfigText::Label { section: "api", key: "max_page_size" } => text.literal("最大每页条数"),
        ConfigText::Help { section: "api", key: "max_page_size" } => text.literal("?per_page= 的上限。"),
    }
}
