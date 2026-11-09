//! Simplified Chinese (`zh-Hans`) entries for `catalog/actions.rs`.

use crate::catalog::actions::ActionsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    ActionsMessage<'a> => MessageWriter |text| {
        ActionsMessage::Actions => text.literal("Actions"),
        ActionsMessage::Workflow => text.literal("工作流"),
        ActionsMessage::Commit => text.literal("提交"),
        ActionsMessage::Duration => text.literal("耗时"),
        ActionsMessage::Attempt => text.literal("尝试"),
        ActionsMessage::TriggeredBy => text.literal("触发者"),
        ActionsMessage::Jobs => text.literal("作业"),
        ActionsMessage::Artifacts => text.literal("制品"),
        ActionsMessage::RunDetails => text.literal("运行详情"),
        ActionsMessage::CancelRun => text.literal("取消运行"),
        ActionsMessage::RerunFailed => text.literal("重新运行失败的作业"),
        ActionsMessage::RerunAll => text.literal("全部重新运行"),
        ActionsMessage::DownloadTar => text.literal("下载为 tar.gz"),
        ActionsMessage::Expires => text.literal("到期"),
        ActionsMessage::RerunJob { name } => {
            text.literal("重新运行 ");
            text.argument(name);
        }
        ActionsMessage::Summary { job } => {
            text.argument(job);
            text.literal(" 概要");
        }
    }
}
