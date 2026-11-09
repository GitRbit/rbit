//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/labels.rs`.

use crate::catalog::collab::labels::LabelMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    LabelMessage<'a> => MessageWriter |text| {
        LabelMessage::NewLabel => text.literal("新建标签"),
        LabelMessage::Name => text.literal("名称"),
        LabelMessage::Colour => text.literal("颜色"),
        LabelMessage::CreateLabel => text.literal("创建标签"),
        LabelMessage::Count { count } => {
            text.number(count);
            text.literal(" 个标签");
        }
        LabelMessage::EmptyTitle => text.literal("还没有标签"),
        LabelMessage::EmptyText => text.literal("标签用于对议题和拉取请求分类：bug、enhancement、good first issue…"),
        LabelMessage::CreateDefaults => text.literal("创建默认标签"),
        LabelMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(" 个开放议题");
        }
        LabelMessage::DeleteConfirm { name } => {
            text.literal("删除标签“");
            text.argument(name);
            text.literal("”？它将从所有议题中移除。");
        }
    }
}
