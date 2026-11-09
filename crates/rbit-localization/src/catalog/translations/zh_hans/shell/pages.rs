//! Simplified Chinese (`zh-Hans`) entries for `catalog/shell/pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::pages::{Failure, PageMessage};

entries! { ZhHans:
    PageMessage<'a> => MessageWriter |text| {
        PageMessage::ErrorTitle(failure) => text.literal(match failure {
            Failure::NotFound => "找不到页面",
            Failure::Forbidden => "拒绝访问",
            Failure::BadRequest => "请求无效",
            Failure::SlowDown => "请放慢速度",
            Failure::Unavailable => "暂时不可用",
            Failure::Internal => "出错了",
        }),
        PageMessage::NotFoundText => text.literal("你要找的页面不存在，或者你没有访问权限。"),
        PageMessage::UnavailableText => text.literal("服务暂时不可用。请稍后重试。"),
        PageMessage::InternalText => text.literal("发生了意外错误，已记录到日志。"),
        PageMessage::GoHome => text.literal("返回首页"),
        PageMessage::Explore => text.literal("探索"),
        PageMessage::Offline => text.literal("你已离线"),
        PageMessage::OfflineText => text.literal("无法连接到服务器。请检查网络连接后重试。"),
        PageMessage::Retry => text.literal("重试"),
        PageMessage::Private => text.literal("私有"),
        PageMessage::Archived => text.literal("已归档"),
        PageMessage::Stars(count) => {
            text.number(count.unsigned_abs());
            text.literal(" 个星标");
        }
        PageMessage::Pagination => text.literal("分页"),
        PageMessage::Previous => text.literal("上一页"),
        PageMessage::Next => text.literal("下一页"),
        PageMessage::PageOf(page, pages) => {
            text.literal("第 ");
            text.number(u64::from(page));
            text.literal(" 页，共 ");
            text.number(u64::from(pages));
            text.literal(" 页");
        }
        PageMessage::EditProfile => text.literal("编辑个人资料"),
        PageMessage::Joined(date) => {
            text.argument(date);
            text.literal(" 加入");
        }
        PageMessage::Profile => text.literal("个人资料"),
        PageMessage::Repositories => text.literal("仓库"),
        PageMessage::StarsTab => text.literal("星标"),
        PageMessage::NewRepository => text.literal("新建仓库"),
        PageMessage::NoStars(name) => {
            text.argument(name);
            text.literal(" 还没有为任何仓库加星标。");
        }
        PageMessage::NoPublicRepositories(name) => {
            text.argument(name);
            text.literal(" 还没有公开仓库。");
        }
    }
}
