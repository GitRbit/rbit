//! Simplified Chinese labels and help of the configuration fields in
//! `catalog/settings/config/fields_b.rs`: webhooks to editor.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

/// The label of every section's on/off switch.
const ENABLE: &str = "启用此功能";
/// The label of every section's private-network switch.
const PRIVATE: &str = "允许私有网络目标";

config_table! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "webhooks", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "webhooks", key: "enabled" } => {
            text.literal("是否投递 Webhook。关闭后仍可配置 Webhook，但不会发送任何内容。");
        }
        ConfigText::Label { section: "webhooks", key: "allow_private_addresses" } => text.literal(PRIVATE),
        ConfigText::Help { section: "webhooks", key: "allow_private_addresses" } => {
            text.literal("允许解析为回环、私有、链路本地或唯一本地地址的目标。");
        }
        ConfigText::Label { section: "webhooks", key: "timeout_seconds" } => text.literal("超时（秒）"),
        ConfigText::Help { section: "webhooks", key: "timeout_seconds" } => {
            text.literal("单次投递尝试（连接、发送、读取响应状态）的时长上限。");
        }
        ConfigText::Label { section: "webhooks", key: "max_attempts" } => text.literal("最大尝试次数"),
        ConfigText::Help { section: "webhooks", key: "max_attempts" } => {
            text.literal("每次投递在标记为失败前的尝试次数，包括第一次。");
        }
        ConfigText::Label { section: "webhooks", key: "concurrency" } => text.literal("并发数"),
        ConfigText::Help { section: "webhooks", key: "concurrency" } => text.literal("整个实例同时进行中的投递数。"),
        ConfigText::Label { section: "webhooks", key: "delivery_retention_days" } => text.literal("投递记录保留时间（天）"),
        ConfigText::Help { section: "webhooks", key: "delivery_retention_days" } => text.literal("投递日志保留请求和响应的天数。"),
        ConfigText::Label { section: "releases", key: "max_asset_bytes" } => text.literal("最大附件大小（字节）"),
        ConfigText::Help { section: "releases", key: "max_asset_bytes" } => text.literal("附加文件的最大大小，以字节为单位。"),
        ConfigText::Label { section: "releases", key: "max_assets_per_release" } => text.literal("每个发行版的最大附件数"),
        ConfigText::Help { section: "releases", key: "max_assets_per_release" } => text.literal("每个发行版可附加的文件数。"),
        ConfigText::Label { section: "releases", key: "upload_timeout_seconds" } => text.literal("上传超时（秒）"),
        ConfigText::Help { section: "releases", key: "upload_timeout_seconds" } => text.literal("单次上传的时长上限，以秒为单位。"),
        ConfigText::Label { section: "mirrors", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "mirrors", key: "enabled" } => text.literal("允许创建镜像并运行计划获取。"),
        ConfigText::Label { section: "mirrors", key: "default_interval_minutes" } => text.literal("默认间隔（分钟）"),
        ConfigText::Help { section: "mirrors", key: "default_interval_minutes" } => text.literal("新镜像的获取间隔。"),
        ConfigText::Label { section: "mirrors", key: "min_interval_minutes" } => text.literal("最短间隔（分钟）"),
        ConfigText::Help { section: "mirrors", key: "min_interval_minutes" } => text.literal("镜像可配置的最短间隔。"),
        ConfigText::Label { section: "mirrors", key: "fetch_timeout_seconds" } => text.literal("获取超时（秒）"),
        ConfigText::Help { section: "mirrors", key: "fetch_timeout_seconds" } => text.literal("单次获取的时长上限。"),
        ConfigText::Label { section: "mirrors", key: "allow_private_addresses" } => text.literal(PRIVATE),
        ConfigText::Help { section: "mirrors", key: "allow_private_addresses" } => {
            text.literal("允许解析为回环、私有、链路本地或唯一本地地址的上游 URL。");
        }
        ConfigText::Label { section: "push_mirrors", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "push_mirrors", key: "enabled" } => text.literal("允许配置推送镜像并向其推送。"),
        ConfigText::Label { section: "push_mirrors", key: "interval_minutes" } => text.literal("间隔（分钟）"),
        ConfigText::Help { section: "push_mirrors", key: "interval_minutes" } => text.literal("每个镜像至少按此频率推送一次。"),
        ConfigText::Label { section: "push_mirrors", key: "debounce_seconds" } => text.literal("防抖延迟（秒）"),
        ConfigText::Help { section: "push_mirrors", key: "debounce_seconds" } => text.literal("推送后等待多久再推送到镜像。"),
        ConfigText::Label { section: "push_mirrors", key: "push_timeout_seconds" } => text.literal("推送超时（秒）"),
        ConfigText::Help { section: "push_mirrors", key: "push_timeout_seconds" } => text.literal("单次 git push 的时长上限。"),
        ConfigText::Label { section: "push_mirrors", key: "allow_private_addresses" } => text.literal(PRIVATE),
        ConfigText::Help { section: "push_mirrors", key: "allow_private_addresses" } => {
            text.literal("允许解析为回环、私有、链路本地或唯一本地地址的远程仓库。");
        }
        ConfigText::Label { section: "push_mirrors", key: "allow_local_urls" } => text.literal("允许本地文件远程仓库（仅限测试）"),
        ConfigText::Help { section: "push_mirrors", key: "allow_local_urls" } => {
            text.literal("允许 file:// 远程仓库（仅限测试；参见模块文档）。");
        }
        ConfigText::Label { section: "imports", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "imports", key: "enabled" } => text.literal("在“新建仓库”页面提供导入功能。"),
        ConfigText::Label { section: "imports", key: "allow_private_addresses" } => text.literal(PRIVATE),
        ConfigText::Help { section: "imports", key: "allow_private_addresses" } => {
            text.literal("允许解析为回环、私有、链路本地或唯一本地地址的来源。");
        }
        ConfigText::Label { section: "imports", key: "max_concurrent" } => text.literal("最大并发数"),
        ConfigText::Help { section: "imports", key: "max_concurrent" } => text.literal("可同时运行的导入数。"),
        ConfigText::Label { section: "imports", key: "git_timeout_seconds" } => text.literal("Git 超时（秒）"),
        ConfigText::Help { section: "imports", key: "git_timeout_seconds" } => text.literal("Git 复制的时长上限。"),
        ConfigText::Label { section: "imports", key: "api_timeout_seconds" } => text.literal("API 超时（秒）"),
        ConfigText::Help { section: "imports", key: "api_timeout_seconds" } => text.literal("单次 API 请求的时长上限。"),
        ConfigText::Label { section: "imports", key: "rate_limit_max_wait_seconds" } => text.literal("速率限制最长等待（秒）"),
        ConfigText::Help { section: "imports", key: "rate_limit_max_wait_seconds" } => {
            text.literal("等待来源的速率限制重置的最长时间。");
        }
        ConfigText::Label { section: "imports", key: "author_mapping" } => text.literal("匹配导入的作者"),
        ConfigText::Help { section: "imports", key: "author_mapping" } => text.literal("来源中的作者如何对应到本地账户。"),
        ConfigText::Label { section: "issues", key: "scan_commit_limit" } => text.literal("提交扫描上限"),
        ConfigText::Help { section: "issues", key: "scan_commit_limit" } => {
            text.literal("每次推送的引用更新中，为查找引用和关闭关键字而扫描的提交数。");
        }
        ConfigText::Label { section: "issues", key: "notifications" } => text.literal("通知"),
        ConfigText::Help { section: "issues", key: "notifications" } => text.literal("是否生成并显示应用内通知。"),
        ConfigText::Label { section: "issues", key: "notification_retention_days" } => text.literal("通知保留时间（天）"),
        ConfigText::Help { section: "issues", key: "notification_retention_days" } => text.literal("已读通知保留的天数。"),
        ConfigText::Label { section: "pulls", key: "merge_timeout_seconds" } => text.literal("合并超时（秒）"),
        ConfigText::Help { section: "pulls", key: "merge_timeout_seconds" } => text.literal(
            "单次合并或试合并的时长上限（秒），涵盖其全部 git 操作：从创建临时仓库到基础仓库的获取和引用更新。",
        ),
        ConfigText::Label { section: "pulls", key: "max_concurrent_merges" } => text.literal("最大并发合并数"),
        ConfigText::Help { section: "pulls", key: "max_concurrent_merges" } => {
            text.literal("整个实例可同时进行的合并数（每个基础仓库始终只有一个）。");
        }
        ConfigText::Label { section: "pulls", key: "mergeability_concurrency" } => text.literal("可合并性检查并发数"),
        ConfigText::Help { section: "pulls", key: "mergeability_concurrency" } => {
            text.literal("可同时进行的可合并性试合并数。");
        }
        ConfigText::Label { section: "pulls", key: "max_commits" } => text.literal("最大提交数"),
        ConfigText::Help { section: "pulls", key: "max_commits" } => text.literal("每个拉取请求显示的提交数。"),
        ConfigText::Label { section: "pulls", key: "noreply_domain" } => text.literal("No-reply 邮件域名"),
        ConfigText::Help { section: "pulls", key: "noreply_domain" } => text.literal(
            "生成的提交中 no-reply 作者所用的域名。留空则使用 noreply 加上你的公开主机名。",
        ),
        ConfigText::Label { section: "editor", key: "max_upload_bytes" } => text.literal("最大上传大小（字节）"),
        ConfigText::Help { section: "editor", key: "max_upload_bytes" } => text.literal("上传文件的最大大小，以字节为单位。"),
        ConfigText::Label { section: "editor", key: "max_upload_files" } => text.literal("最大上传文件数"),
        ConfigText::Help { section: "editor", key: "max_upload_files" } => text.literal("每次上传的文件数。"),
        ConfigText::Label { section: "editor", key: "upload_timeout_seconds" } => text.literal("上传超时（秒）"),
        ConfigText::Help { section: "editor", key: "upload_timeout_seconds" } => text.literal("单次上传请求的时长上限，以秒为单位。"),
    }
}
