//! Simplified Chinese labels and help of the configuration fields in
//! `catalog/settings/config/fields_a.rs`: updates to repository.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

config_table! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "updates", key: "check_for_updates" } => text.literal("检查软件更新"),
        ConfigText::Help { section: "updates", key: "check_for_updates" } => text.literal(
            "成功检查后，最多每天访问一次 GitHub 公开发行版源。不会发送正在运行的版本或实例标识。需要重启。",
        ),
        ConfigText::Label { section: "updates", key: "report_version" } => text.literal("公开正在运行的版本"),
        ConfigText::Help { section: "updates", key: "report_version" } => text.literal(
            "在公开页脚、版本 API、SSH 横幅和对外集成的 User-Agent 中公开软件版本。禁用即可隐藏；管理员仍保留本地诊断信息。需要重启。",
        ),
        ConfigText::Label { section: "server", key: "bind" } => text.literal("监听地址"),
        ConfigText::Help { section: "server", key: "bind" } => text.literal("HTTP 服务器绑定的地址。"),
        ConfigText::Label { section: "server", key: "public_url" } => text.literal("公开 URL"),
        ConfigText::Help { section: "server", key: "public_url" } => text.literal("公开的基础 URL，用于克隆 URL 和 CSRF 来源检查。"),
        ConfigText::Label { section: "server", key: "site_name" } => text.literal("默认站点名称"),
        ConfigText::Help { section: "server", key: "site_name" } => {
            text.literal("实时品牌覆盖前使用的默认名称。请在“品牌与策略”中修改当前名称。");
        }
        ConfigText::Label { section: "server", key: "explore_enabled" } => text.literal("默认提供探索页面"),
        ConfigText::Help { section: "server", key: "explore_enabled" } => text.literal("列出全部公开仓库的探索页面的默认设置。“品牌与策略”中的实时设置优先。"),
        ConfigText::Label { section: "server", key: "open_registration" } => text.literal("默认开放注册"),
        ConfigText::Help { section: "server", key: "open_registration" } => {
            text.literal("默认注册策略。“品牌与策略”中的实时策略优先。");
        }
        ConfigText::Label { section: "storage", key: "data_dir" } => text.literal("数据目录"),
        ConfigText::Help { section: "storage", key: "data_dir" } => {
            text.literal("此处只读。请停止服务器、移动数据，然后更新部署文件或环境变量。");
        }
        ConfigText::Label { section: "storage", key: "secret_key_file" } => text.literal("密钥文件"),
        ConfigText::Help { section: "storage", key: "secret_key_file" } => text.literal(
            "此处只读。用于加密已存储凭据的密钥；留空表示数据目录中的 secret.key。请将其与数据库一起备份。",
        ),
        ConfigText::Label { section: "security", key: "session_lifetime_days" } => text.literal("会话有效期（天）"),
        ConfigText::Help { section: "security", key: "session_lifetime_days" } => text.literal("会话在无活动时保持有效的天数。"),
        ConfigText::Label { section: "security", key: "session_max_age_days" } => text.literal("会话最长期限（天）"),
        ConfigText::Help { section: "security", key: "session_max_age_days" } => {
            text.literal("无论是否活跃，会话最多保持有效的天数。0 表示不限制。");
        }
        ConfigText::Label { section: "security", key: "min_password_length" } => text.literal("最短密码长度"),
        ConfigText::Help { section: "security", key: "min_password_length" } => text.literal("注册时接受的最短密码长度。"),
        ConfigText::Label { section: "security", key: "max_password_jobs" } => text.literal("最大密码任务数"),
        ConfigText::Help { section: "security", key: "max_password_jobs" } => text.literal(
            "共享密码计算预算中可同时进行的哈希或验证任务数（1–64；默认 2）。需要重启。",
        ),
        ConfigText::Label { section: "security", key: "trusted_proxies" } => text.literal("受信任的代理网络"),
        ConfigText::Help { section: "security", key: "trusted_proxies" } => text.literal(
            "其 X-Forwarded-For、Forwarded 和 X-Forwarded-Proto 头会被信任的反向代理（CIDR 网络，例如 127.0.0.1/32、\
             10.0.0.0/8、::1/128）。留空（默认）表示 TCP 对端始终就是客户端。",
        ),
        ConfigText::Label { section: "security", key: "client_ip_header" } => text.literal("客户端地址标头"),
        ConfigText::Help { section: "security", key: "client_ip_header" } => text.literal("受信任代理用来报告客户端地址的标头，例如 Cloudflare 或 Cloudflare Tunnel 之后的 CF-Connecting-IP。仅接受来自上方受信任代理网络的值，优先于 X-Forwarded-For。留空则只使用 X-Forwarded-For。"),
        ConfigText::Label { section: "security", key: "rate_limit" } => text.literal("启用速率限制"),
        ConfigText::Help { section: "security", key: "rate_limit" } => text.literal(
            "按客户端限制登录、注册、双重身份验证码、Git 凭据和表单提交的频率。只有在负载测试时才值得关闭。",
        ),
        ConfigText::Label { section: "security", key: "hsts" } => text.literal("发送 HSTS"),
        ConfigText::Help { section: "security", key: "hsts" } => text.literal(
            "当公开 URL 为 https:// 时发送 Strict-Transport-Security（一年），使浏览器拒绝以纯 HTTP 访问此主机。\
             仅在停用 HTTPS 的迁移期间关闭。",
        ),
        ConfigText::Label { section: "security", key: "hsts_include_subdomains" } => text.literal("HSTS 包含子域名"),
        ConfigText::Help { section: "security", key: "hsts_include_subdomains" } => {
            text.literal("在该响应头中添加 includeSubDomains，使此主机的所有子域名同样只能使用 HTTPS。");
        }
        ConfigText::Label { section: "security", key: "external_images" } => text.literal("允许外部图片"),
        ConfigText::Help { section: "security", key: "external_images" } => text.literal(
            "允许页面显示来自其他网站的图片，这也会让那些网站知道谁查看了页面。关闭后仅允许此实例的图片和内联 data: 图片。",
        ),
        ConfigText::Label { section: "repository", key: "default_branch" } => text.literal("默认分支"),
        ConfigText::Help { section: "repository", key: "default_branch" } => text.literal("新建仓库中 HEAD 指向的分支。"),
        ConfigText::Label { section: "repository", key: "git_binary" } => text.literal("Git 可执行文件"),
        ConfigText::Help { section: "repository", key: "git_binary" } => text.literal("智能 HTTP 传输所用 git 可执行文件的路径。"),
        ConfigText::Label { section: "repository", key: "max_render_bytes" } => text.literal("最大渲染大小（字节）"),
        ConfigText::Help { section: "repository", key: "max_render_bytes" } => text.literal("大于此值的文件不会在浏览器中内联渲染。"),
        ConfigText::Label { section: "repository", key: "max_git_processes" } => text.literal("最大 Git 进程数"),
        ConfigText::Help { section: "repository", key: "max_git_processes" } => text.literal(
            "可同时运行的 Git 传输进程数（通过 HTTP 和 SSH 的克隆、获取和推送）；超出的请求会被拒绝（HTTP 返回 503）。",
        ),
        ConfigText::Label { section: "repository", key: "max_git_requests_per_ip" } => text.literal("每个 IP 的最大 Git 请求数"),
        ConfigText::Help { section: "repository", key: "max_git_requests_per_ip" } => text.literal(
            "单个 IP 地址的并发智能 HTTP Git 请求数（超出返回 429）。位于反向代理之后时，需要配置受信任的代理网络。",
        ),
        ConfigText::Label { section: "repository", key: "git_idle_timeout_seconds" } => text.literal("Git 空闲超时（秒）"),
        ConfigText::Help { section: "repository", key: "git_idle_timeout_seconds" } => {
            text.literal("一次智能 HTTP Git 交互在双向均无流量时可持续的秒数。");
        }
        ConfigText::Label { section: "repository", key: "git_max_seconds" } => text.literal("Git 最长持续时间（秒）"),
        ConfigText::Help { section: "repository", key: "git_max_seconds" } => {
            text.literal("单次智能 HTTP Git 交互（如非常大的克隆或推送）的时长上限，以秒为单位。");
        }
        ConfigText::Label { section: "repository", key: "max_push_bytes" } => text.literal("最大推送大小（字节）"),
        ConfigText::Help { section: "repository", key: "max_push_bytes" } => {
            text.literal("通过 HTTP 和 SSH 单次推送可发送的最大包大小。0 表示不限制。");
        }
        ConfigText::Label { section: "repository", key: "max_archives_per_ip" } => text.literal("每个 IP 的最大归档下载数"),
        ConfigText::Help { section: "repository", key: "max_archives_per_ip" } => {
            text.literal("单个 IP 地址可同时进行的源代码归档下载数。");
        }
        ConfigText::Label { section: "repository", key: "archive_idle_timeout_seconds" } => text.literal("归档空闲超时（秒）"),
        ConfigText::Help { section: "repository", key: "archive_idle_timeout_seconds" } => {
            text.literal("归档下载可停滞（客户端未读取或 git 未写入）的秒数。");
        }
        ConfigText::Label { section: "repository", key: "archive_max_seconds" } => text.literal("归档最长持续时间（秒）"),
        ConfigText::Help { section: "repository", key: "archive_max_seconds" } => text.literal("单次归档下载的时长上限，以秒为单位。"),
    }
}
