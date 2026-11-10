//! Simplified Chinese (`zh-Hans`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;

entries! { ZhHans:
    RunLayoutMessage<'a> => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("摘要"),
        RunLayoutMessage::Navigation => text.literal("运行导航"),
        RunLayoutMessage::Usage => text.literal("用量"),
        RunLayoutMessage::WorkflowFile => text.literal("工作流文件"),
        RunLayoutMessage::JobCount { count } => {
            text.number(count);
            text.literal(" 个作业");
        }
        RunLayoutMessage::Graph => text.literal("作业图"),
        RunLayoutMessage::MoreJobs { count } => {
            text.literal("另外 ");
            text.number(count);
            text.literal(" 个");
        }
        RunLayoutMessage::Annotations => text.literal("注释"),
        RunLayoutMessage::Errors { count } => {
            text.number(count);
            text.literal(" 个错误");
        }
        RunLayoutMessage::Warnings { count } => {
            text.number(count);
            text.literal(" 个警告");
        }
        RunLayoutMessage::Notices { count } => {
            text.number(count);
            text.literal(" 个通知");
        }
        RunLayoutMessage::ErrorLevel => text.literal("错误"),
        RunLayoutMessage::WarningLevel => text.literal("警告"),
        RunLayoutMessage::NoticeLevel => text.literal("通知"),
        RunLayoutMessage::StepFailed { step } => {
            text.literal("步骤 ");
            text.argument(step);
            text.literal(" 失败。");
        }
        RunLayoutMessage::MoreAnnotations { count } => {
            text.literal("作业日志中还有 ");
            text.number(count);
            text.literal(" 条。");
        }
    }
}
