//! Simplified Chinese section titles and help, and choice labels, of
//! `catalog/settings/config.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

config_table! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::SectionTitle("updates") => text.literal("软件更新与隐私"),
        ConfigText::SectionHelp("updates") => text.literal("独立的后台发行版检查和运行版本公开控制。更改在重启后生效。"),
        ConfigText::SectionTitle("server") => text.literal("服务器"),
        ConfigText::SectionHelp("server") => text.literal("公开地址、监听器和实例默认值。"),
        ConfigText::SectionTitle("storage") => text.literal("存储"),
        ConfigText::SectionHelp("storage") => text.literal("查看数据目录。如需迁移存储，请离线修改部署配置。"),
        ConfigText::SectionTitle("security") => text.literal("安全"),
        ConfigText::SectionHelp("security") => text.literal("会话、密码、速率限制和受信任的反向代理。"),
        ConfigText::SectionTitle("repository") => text.literal("仓库"),
        ConfigText::SectionHelp("repository") => text.literal("Git 仓库的默认值和限制。"),
        ConfigText::SectionTitle("mail") => text.literal("邮件投递"),
        ConfigText::SectionHelp("mail") => text.literal("配置发件人身份、SMTP 连接和投递超时。"),
        ConfigText::SectionTitle("ssh") => text.literal("SSH"),
        ConfigText::SectionHelp("ssh") => text.literal("内置 Git SSH 监听器、公开克隆地址和连接限制。"),
        ConfigText::SectionTitle("auth") => text.literal("登录提供方"),
        ConfigText::SectionHelp("auth") => text.literal("连接 OIDC 和 GitHub 身份提供方。"),
        ConfigText::SectionTitle("branding") => text.literal("品牌默认值"),
        ConfigText::SectionHelp("branding") => text.literal("部署默认值。如需立即应用于整个实例，请使用“品牌与策略”。"),
        ConfigText::SectionTitle("notifications") => text.literal("邮件通知"),
        ConfigText::SectionHelp("notifications") => text.literal("启用通知邮件并选择默认订阅策略。"),
        ConfigText::SectionTitle("operations") => text.literal("运维"),
        ConfigText::SectionHelp("operations") => text.literal("指标、维护、后台作业和缓存。"),
        ConfigText::SectionTitle("api") => text.literal("REST API"),
        ConfigText::SectionHelp("api") => text.literal("API 可用性和分页限制。"),
        ConfigText::SectionTitle("webhooks") => text.literal("Webhook"),
        ConfigText::SectionHelp("webhooks") => text.literal("外发投递、重试和保留设置。"),
        ConfigText::SectionTitle("releases") => text.literal("发行版"),
        ConfigText::SectionHelp("releases") => text.literal("发行版附件的大小、数量和上传超时。"),
        ConfigText::SectionTitle("mirrors") => text.literal("拉取镜像"),
        ConfigText::SectionHelp("mirrors") => text.literal("获取计划、超时和网络访问。"),
        ConfigText::SectionTitle("push_mirrors") => text.literal("推送镜像"),
        ConfigText::SectionHelp("push_mirrors") => text.literal("推送计划、超时和网络访问。"),
        ConfigText::SectionTitle("imports") => text.literal("导入"),
        ConfigText::SectionHelp("imports") => text.literal("仓库导入、并发和作者匹配。"),
        ConfigText::SectionTitle("issues") => text.literal("议题与收件箱"),
        ConfigText::SectionHelp("issues") => text.literal("提交扫描和应用内通知的保留。"),
        ConfigText::SectionTitle("pulls") => text.literal("拉取请求"),
        ConfigText::SectionHelp("pulls") => text.literal("合并限制、后台检查和作者地址。"),
        ConfigText::SectionTitle("editor") => text.literal("浏览器编辑器"),
        ConfigText::SectionHelp("editor") => text.literal("浏览器上传的大小、数量和超时限制。"),
        ConfigText::SectionTitle("lfs") => text.literal("Git LFS"),
        ConfigText::SectionHelp("lfs") => text.literal("大文件存储、对象大小和令牌有效期。"),
        ConfigText::SectionTitle("actions") => text.literal("Actions"),
        ConfigText::SectionHelp("actions") => text.literal("在外部运行器上运行的 CI/CD，以及保留、限制和超时。"),
        ConfigText::Choice { section: "mail", key: "transport", value: "disabled" } => text.literal("已禁用"),
        ConfigText::Choice { section: "mail", key: "transport", value: "log" } => text.literal("开发日志"),
        ConfigText::Choice { section: "mail", key: "transport", value: "smtp" } => text.literal("SMTP 服务器"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "starttls" } => text.literal("STARTTLS（推荐）"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "tls" } => text.literal("隐式 TLS"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "none" } => text.literal("不加密"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "off" } => text.literal("关闭"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "participating" } => {
            text.literal("用户参与的讨论");
        }
        ConfigText::Choice { section: "notifications", key: "default_email", value: "all" } => text.literal("所有已订阅的讨论"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "none" } => text.literal("以文本形式保留原作者"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email" } => text.literal("匹配已验证的邮箱地址"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email_or_username" } => {
            text.literal("匹配已验证的邮箱地址或用户名");
        }
        ConfigText::Choice { section: "provider", key: "kind", value: "oidc" } => text.literal("OpenID Connect"),
        ConfigText::Choice { section: "provider", key: "kind", value: "github" } => text.literal("GitHub / GitHub Enterprise"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "true" } => text.literal("遵循实例的注册策略"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "false" } => text.literal("仅限现有账户"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "always" } => {
            text.literal("即使注册已关闭也允许创建账户");
        }
    }
}
