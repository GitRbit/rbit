//! Simplified Chinese (`zh-Hans`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;

entries! { ZhHans:
    RunLayoutMessage => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("摘要"),
        RunLayoutMessage::Navigation => text.literal("运行导航"),
        RunLayoutMessage::Usage => text.literal("用量"),
        RunLayoutMessage::WorkflowFile => text.literal("工作流文件"),
        RunLayoutMessage::JobCount { count } => {
            text.number(count);
            text.literal(" 个作业");
        }
    }
}
