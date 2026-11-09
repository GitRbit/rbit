//! Simplified Chinese (`zh-Hans`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};

entries! { ZhHans:
    ReleaseLabel => MessageWriter |text| {
        ReleaseLabel::Releases => text.literal("发行版"),
        ReleaseLabel::NewRelease => text.literal("新建发行版"),
        ReleaseLabel::EditRelease => text.literal("编辑发行版"),
        ReleaseLabel::NoReleasesYet => text.literal("还没有发行版"),
        ReleaseLabel::EmptyForWriters => {
            text.literal("发行版将一个标记与说明和文件打包在一起。先推送一个标记，再为它创建发行版。");
        }
        ReleaseLabel::EmptyForReaders => text.literal("发行版用说明和下载文件标识此项目的各个版本。"),
        ReleaseLabel::Draft => text.literal("草稿"),
        ReleaseLabel::PreRelease => text.literal("预发行版"),
        ReleaseLabel::Latest => text.literal("最新"),
        ReleaseLabel::Bot => text.literal("机器人"),
        ReleaseLabel::Created => text.literal("创建于"),
        ReleaseLabel::Edit => text.literal("编辑"),
        ReleaseLabel::Downloads => text.literal("下载"),
        ReleaseLabel::SourceZip => text.literal("源代码（zip）"),
        ReleaseLabel::SourceTarGz => text.literal("源代码（tar.gz）"),
        ReleaseLabel::Files => text.literal("文件"),
        ReleaseLabel::Remove => text.literal("移除"),
        ReleaseLabel::FileToAttach => text.literal("要附加的文件"),
        ReleaseLabel::Upload => text.literal("上传"),
        ReleaseLabel::DeleteTitle => text.literal("删除此发行版"),
        ReleaseLabel::DeleteHintStart => text.literal("发行版及其说明和文件将被移除。标记"),
        ReleaseLabel::DeleteHintEnd => text.literal("会保留。"),
        ReleaseLabel::DeleteRelease => text.literal("删除发行版"),
        ReleaseLabel::FormLead => text.literal("发行版用于描述一个标记：它的说明，以及可供下载的文件。"),
        ReleaseLabel::Tag => text.literal("标记"),
        ReleaseLabel::AnExisting => text.literal("已有"),
        ReleaseLabel::TagLink => text.literal("标记"),
        ReleaseLabel::OrNewTag => text.literal("或从下方分支新建的标记。"),
        ReleaseLabel::CreateTagFrom => text.literal("从此处新建标记"),
        ReleaseLabel::TargetHint => text.literal("仅在标记不存在时使用"),
        ReleaseLabel::OneOfTags => text.literal("仓库现有的"),
        ReleaseLabel::TagsEnd => text.literal("。"),
        ReleaseLabel::TagsLink => text.literal("标记"),
        ReleaseLabel::Title => text.literal("标题"),
        ReleaseLabel::TitleHint => text.literal("可选，默认为标记名"),
        ReleaseLabel::Notes => text.literal("说明"),
        ReleaseLabel::IsPreRelease => text.literal("这是预发行版"),
        ReleaseLabel::PreReleaseHint => {
            text.literal("标记为尚未可用于生产环境；正式发行版会优先作为最新版本。");
        }
        ReleaseLabel::SaveRelease => text.literal("保存发行版"),
        ReleaseLabel::PublishRelease => text.literal("发布发行版"),
        ReleaseLabel::SaveDraft => text.literal("保存草稿"),
        ReleaseLabel::Cancel => text.literal("取消"),
    }
    ReleaseMessage<'a> => MessageWriter |text| {
        ReleaseMessage::Checksum { sha256 } => {
            text.literal("SHA-256 ");
            text.argument(sha256);
        }
        ReleaseMessage::Downloads { count } => {
            text.number(count);
            text.literal(" 次下载");
        }
        ReleaseMessage::RemoveFile { name } => {
            text.literal("移除 ");
            text.argument(name);
        }
        ReleaseMessage::UploadLimit { size } => {
            text.literal("每个文件最大 ");
            text.argument(size);
            text.literal("。下载内容始终以文件形式提供，不会直接显示。");
        }
        ReleaseMessage::Uploaded { name } => {
            text.literal("已上传 ");
            text.argument(name);
            text.literal("。");
        }
        ReleaseMessage::Removed { name } => {
            text.literal("已移除 ");
            text.argument(name);
            text.literal("。");
        }
        ReleaseMessage::EditTitle { tag } => {
            text.literal("编辑 ");
            text.argument(tag);
        }
    }
}
