//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};

entries! { ZhHans:
    TokensMessage => MessageWriter |text| {
        TokensMessage::Title => text.literal("访问令牌"),
        TokensMessage::IntroLead => text.literal("通过 HTTPS 使用 Git 时，可用令牌代替密码：输入你的用户名"),
        TokensMessage::IntroUsername => text.literal("，并在 Git 要求输入密码时粘贴令牌。位于"),
        TokensMessage::IntroApi => text.literal("的 REST API 以如下形式接受令牌："),
        TokensMessage::IntroPrefix => text.literal("。令牌以"),
        TokensMessage::IntroLegacy => text.literal("开头；早期版本的令牌以"),
        TokensMessage::IntroTail => text.literal("开头，并且仍然有效。"),
        TokensMessage::CopyNow => text.literal("请立即复制你的新令牌。之后将无法再次查看。"),
        TokensMessage::CopyToken => text.literal("复制令牌"),
        TokensMessage::Copy => text.literal("复制"),
        TokensMessage::NewToken => text.literal("新建令牌"),
        TokensMessage::Name => text.literal("名称"),
        TokensMessage::NamePlaceholder => text.literal("例如：工作笔记本"),
        TokensMessage::NameHint => text.literal("便于日后识别的名称。"),
        TokensMessage::Scope => text.literal("权限范围"),
        TokensMessage::ScopeHint => text.literal("令牌的权限永远不会超过你的账户。"),
        TokensMessage::Expiration => text.literal("有效期"),
        TokensMessage::Generate => text.literal("生成令牌"),
        TokensMessage::YourTokens => text.literal("你的令牌"),
        TokensMessage::EmptyTitle => text.literal("没有访问令牌"),
        TokensMessage::EmptyText => text.literal("生成一个令牌，即可不使用密码通过 HTTPS 克隆、获取和推送。"),
        TokensMessage::Revoke => text.literal("吊销"),
        TokensMessage::NoExpiration => text.literal("永不过期"),
        TokensMessage::OneDay => text.literal("1 天"),
        TokensMessage::NeverUsed => text.literal("从未使用"),
        TokensMessage::ReadDescription => text.literal("克隆和获取；通过 API 读取。"),
        TokensMessage::WriteDescription => text.literal("还可以推送，以及通过 API 创建仓库和加星标。"),
        TokensMessage::AdminDescription => {
            text.literal("还可以通过 API 修改仓库设置和 Webhook，以及删除仓库。");
        }
    }
    TokenText<'a> => MessageWriter |text| {
        TokenText::Created(name) => {
            text.literal("令牌“");
            text.argument(name);
            text.literal("”已创建");
        }
        TokenText::RevokeLabel(name) => {
            text.literal("吊销 ");
            text.argument(name);
        }
        TokenText::Days(days) => {
            text.number(days);
            text.literal(" 天");
        }
        TokenText::CreatedOn(date) => {
            text.literal("创建于 ");
            text.argument(date);
        }
        TokenText::LastUsed(when) => {
            text.literal("上次使用 ");
            text.argument(when);
        }
        TokenText::Expires(date) => {
            text.literal("过期时间 ");
            text.argument(date);
        }
        TokenText::Expired(date) => {
            text.literal("已于 ");
            text.argument(date);
            text.literal(" 过期");
        }
    }
}
