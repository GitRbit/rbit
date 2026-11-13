//! Simplified Chinese labels and help of the configuration fields in
//! `catalog/settings/config/fields_b.rs`: Git LFS, Actions and sign-in providers.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

/// The help text of provider fields that only take effect after a restart.
const AFTER_RESTART: &str = "部署设置，重启服务器后生效。";

config_table! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "lfs", key: "enabled" } => text.literal("启用此功能"),
        ConfigText::Help { section: "lfs", key: "enabled" } => text.literal("提供 LFS API 和 SSH git-lfs-authenticate 命令。"),
        ConfigText::Label { section: "lfs", key: "max_object_bytes" } => text.literal("最大对象大小（字节）"),
        ConfigText::Help { section: "lfs", key: "max_object_bytes" } => text.literal("对象的最大大小，以字节为单位。"),
        ConfigText::Label { section: "lfs", key: "token_lifetime_seconds" } => text.literal("令牌有效期（秒）"),
        ConfigText::Help { section: "lfs", key: "token_lifetime_seconds" } => text.literal("已签名传输请求头的有效期，以秒为单位。"),
        ConfigText::Label { section: "actions", key: "enabled" } => text.literal("启用此功能"),
        ConfigText::Help { section: "actions", key: "enabled" } => text.literal(
            "在 rbit-runner 上运行工作流。关闭后 Actions 将完全不存在：没有页面、没有运行器端点、没有后台工作，工作流文件也会被忽略。需要重启。",
        ),
        ConfigText::Label { section: "actions", key: "enable_for_new_repositories" } => text.literal("为新仓库启用"),
        ConfigText::Help { section: "actions", key: "enable_for_new_repositories" } => {
            text.literal("为今后创建的仓库开启 Actions。现有仓库保留各自的设置。");
        }
        ConfigText::Label { section: "actions", key: "secrets_key_file" } => text.literal("机密密钥文件"),
        ConfigText::Help { section: "actions", key: "secrets_key_file" } => text.literal(
            "Actions 机密的加密密钥。留空则使用数据目录中的 actions/secrets.key。请单独备份。",
        ),
        ConfigText::Label { section: "actions", key: "log_retention_days" } => text.literal("日志保留时间（天）"),
        ConfigText::Help { section: "actions", key: "log_retention_days" } => text.literal("作业日志保留的天数。"),
        ConfigText::Label { section: "actions", key: "run_retention_days" } => text.literal("运行保留时间（天）"),
        ConfigText::Help { section: "actions", key: "run_retention_days" } => text.literal("已完成的运行保留的天数。"),
        ConfigText::Label { section: "actions", key: "max_log_bytes" } => text.literal("最大日志大小（字节）"),
        ConfigText::Help { section: "actions", key: "max_log_bytes" } => text.literal("每个任务保留的日志字节数；超出部分会被丢弃。"),
        ConfigText::Label { section: "actions", key: "max_workflow_bytes" } => text.literal("最大工作流文件大小（字节）"),
        ConfigText::Help { section: "actions", key: "max_workflow_bytes" } => text.literal("可读取的工作流文件的最大大小，以字节为单位。"),
        ConfigText::Label { section: "actions", key: "zombie_timeout_seconds" } => text.literal("心跳超时（秒）"),
        ConfigText::Help { section: "actions", key: "zombie_timeout_seconds" } => {
            text.literal("运行中的任务在这么长时间内没有心跳即判定为失败。");
        }
        ConfigText::Label { section: "actions", key: "endless_timeout_seconds" } => text.literal("任务时间上限（秒）"),
        ConfigText::Help { section: "actions", key: "endless_timeout_seconds" } => text.literal("任务运行达到这么长时间即判定为失败。"),
        ConfigText::Label { section: "actions", key: "abandoned_timeout_seconds" } => text.literal("排队时间上限（秒）"),
        ConfigText::Help { section: "actions", key: "abandoned_timeout_seconds" } => {
            text.literal("等待中的作业在这么长时间内没有运行器接收即被取消。");
        }
        ConfigText::Label { section: "actions", key: "evaluation_concurrency" } => text.literal("评估并发数"),
        ConfigText::Help { section: "actions", key: "evaluation_concurrency" } => {
            text.literal("可同时进行的工作流评估（Git 读取和 YAML 解析）数，1 到 16。");
        }
        ConfigText::Label { section: "actions", key: "instance_runners_for_all" } => text.literal("实例运行器为所有人服务"),
        ConfigText::Help { section: "actions", key: "instance_runners_for_all" } => text.literal(
            "允许实例运行器运行所有所有者的作业。关闭后，它们只为管理员允许的所有者服务。",
        ),
        ConfigText::Label { section: "actions", key: "max_log_storage_bytes" } => text.literal("日志存储预算（字节）"),
        ConfigText::Help { section: "actions", key: "max_log_storage_bytes" } => text.literal(
            "整个实例保留的日志字节数；超出后，最早完成的运行会最先失去日志。",
        ),
        ConfigText::Label { section: "actions", key: "max_waiting_jobs_per_repository" } => text.literal("每个仓库的最大等待作业数"),
        ConfigText::Help { section: "actions", key: "max_waiting_jobs_per_repository" } => {
            text.literal("等待中的作业超过此数量时，新的运行会失败并给出可读的错误。");
        }
        ConfigText::Label { section: "actions", key: "max_runs_per_event" } => text.literal("每个事件的最大运行数"),
        ConfigText::Help { section: "actions", key: "max_runs_per_event" } => {
            text.literal("一次推送或拉取请求事件可启动的运行数。");
        }
        ConfigText::Label { section: "actions", key: "max_jobs_per_event" } => text.literal("每个事件的最大作业数"),
        ConfigText::Help { section: "actions", key: "max_jobs_per_event" } => text.literal("一个事件在其所有运行中可创建的作业数。"),
        ConfigText::Label { section: "actions", key: "max_schedules_per_repository" } => text.literal("每个仓库的最大计划数"),
        ConfigText::Help { section: "actions", key: "max_schedules_per_repository" } => text.literal("每个仓库的计划工作流条目数。"),
        ConfigText::Label { section: "actions", key: "long_poll_seconds" } => text.literal("运行器长轮询（秒）"),
        ConfigText::Help { section: "actions", key: "long_poll_seconds" } => {
            text.literal("运行器获取工作的轮询请求保持打开的时长，1 到 25。");
        }
        ConfigText::Label { section: "actions", key: "max_polls" } => text.literal("最大运行器轮询数"),
        ConfigText::Help { section: "actions", key: "max_polls" } => {
            text.literal("可同时保持打开的运行器轮询数；超出后，在有轮询结束前新的轮询会被拒绝。");
        }
        ConfigText::Label { section: "actions", key: "max_artifact_bytes" } => text.literal("最大制品大小（字节）"),
        ConfigText::Help { section: "actions", key: "max_artifact_bytes" } => text.literal("作业可上传的最大制品。"),
        ConfigText::Label { section: "actions", key: "max_artifact_storage_bytes" } => text.literal("每个仓库的制品存储（字节）"),
        ConfigText::Help { section: "actions", key: "max_artifact_storage_bytes" } => text.literal(
            "每个仓库保留的制品字节数；超出后上传会被拒绝。备份（格式 5 及以后）包含制品。",
        ),
        ConfigText::Label { section: "actions", key: "artifact_retention_days" } => text.literal("制品保留时间（天）"),
        ConfigText::Help { section: "actions", key: "artifact_retention_days" } => {
            text.literal("制品保留的天数。工作流的 retention-days 只能降低此值。");
        }
        ConfigText::Label { section: "actions", key: "max_cache_storage_bytes" } => text.literal("每个仓库的缓存存储（字节）"),
        ConfigText::Help { section: "actions", key: "max_cache_storage_bytes" } => text.literal(
            "每个仓库为其所有运行器保留的 actions/cache 字节数；最久未使用的条目最先移除。备份（格式 5 及以后）包含缓存条目。",
        ),
        ConfigText::Label { section: "actions", key: "cache_retention_days" } => text.literal("缓存保留时间（天）"),
        ConfigText::Help { section: "actions", key: "cache_retention_days" } => text.literal("缓存条目在未被恢复的情况下保留的天数。"),
        ConfigText::Label { section: "provider", key: "kind" } => text.literal("提供方类型"),
        ConfigText::Help { section: "provider", key: "kind" } => text.literal(AFTER_RESTART),
        ConfigText::Label { section: "provider", key: "id" } => text.literal("提供方 ID"),
        ConfigText::Help { section: "provider", key: "id" } => {
            text.literal("用于回调 URL 的唯一短 ID。更改后需要重新输入提供方密钥。");
        }
        ConfigText::Label { section: "provider", key: "name" } => text.literal("显示名称"),
        ConfigText::Help { section: "provider", key: "name" } => text.literal("显示在按钮和设置中。"),
        ConfigText::Label { section: "provider", key: "issuer" } => text.literal("OIDC 签发者 URL"),
        ConfigText::Help { section: "provider", key: "issuer" } => text.literal("OIDC 签发者 URL（仅限 kind = \"oidc\"）。"),
        ConfigText::Label { section: "provider", key: "base_url" } => text.literal("GitHub Enterprise URL"),
        ConfigText::Help { section: "provider", key: "base_url" } => {
            text.literal("GitHub Enterprise Server URL（仅限 kind = \"github\"）；默认为 https://github.com。");
        }
        ConfigText::Label { section: "provider", key: "client_id" } => text.literal("客户端 ID"),
        ConfigText::Help { section: "provider", key: "client_id" } => text.literal(AFTER_RESTART),
        ConfigText::Label { section: "provider", key: "client_secret" } => text.literal("客户端密钥"),
        ConfigText::Help { section: "provider", key: "client_secret" } => text.literal("永远不会被记录或显示（参见 Debug 实现）。"),
        ConfigText::Label { section: "provider", key: "scopes" } => text.literal("请求的权限范围"),
        ConfigText::Help { section: "provider", key: "scopes" } => {
            text.literal("每行输入一个权限范围。使用提供方默认值即可请求其推荐的权限范围。");
        }
        ConfigText::Label { section: "provider", key: "allow_registration" } => text.literal("账户创建"),
        ConfigText::Help { section: "provider", key: "allow_registration" } => text.literal(AFTER_RESTART),
        ConfigText::Label { section: "provider", key: "trust_email" } => text.literal("信任已验证的邮箱地址"),
        ConfigText::Help { section: "provider", key: "trust_email" } => text.literal(
            "首次登录时，将其关联到已验证邮箱与提供方已验证邮箱相同的唯一账户。仅适用于能管控其用户地址的提供方。通过它注册时，也会将其已验证的地址视为已验证，无需确认邮件。",
        ),
        ConfigText::Label { section: "provider", key: "trust_mfa" } => text.literal("信任提供方的多因素身份验证"),
        ConfigText::Help { section: "provider", key: "trust_mfa" } => text.literal(
            "当 ID 令牌的 amr 声明表明提供方已执行多因素身份验证（\"mfa\"）时，跳过账户自身的双重身份验证步骤。仅限 OIDC。",
        ),
    }
}
