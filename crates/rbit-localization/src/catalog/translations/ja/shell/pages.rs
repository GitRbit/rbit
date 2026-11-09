//! Japanese (`ja`) entries for `catalog/shell/pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::pages::{Failure, PageMessage};

entries! { Ja:
    PageMessage<'a> => MessageWriter |text| {
        PageMessage::ErrorTitle(failure) => text.literal(match failure {
            Failure::NotFound => "ページが見つかりません",
            Failure::Forbidden => "アクセスが拒否されました",
            Failure::BadRequest => "不正なリクエスト",
            Failure::SlowDown => "リクエストが多すぎます",
            Failure::Unavailable => "一時的に利用できません",
            Failure::Internal => "問題が発生しました",
        }),
        PageMessage::NotFoundText => {
            text.literal("お探しのページは存在しないか、アクセスする権限がありません。");
        }
        PageMessage::UnavailableText => {
            text.literal("サービスは一時的に利用できません。しばらくしてからもう一度お試しください。");
        }
        PageMessage::InternalText => text.literal("予期しないエラーが発生しました。エラーは記録されています。"),
        PageMessage::GoHome => text.literal("ホームに戻る"),
        PageMessage::Explore => text.literal("探索"),
        PageMessage::Offline => text.literal("オフラインです"),
        PageMessage::OfflineText => {
            text.literal("サーバーに接続できませんでした。接続を確認して、もう一度お試しください。");
        }
        PageMessage::Retry => text.literal("再試行"),
        PageMessage::Private => text.literal("プライベート"),
        PageMessage::Archived => text.literal("アーカイブ済み"),
        PageMessage::Stars(count) => {
            text.number(count.unsigned_abs());
            text.literal(" スター");
        }
        PageMessage::Pagination => text.literal("ページ送り"),
        PageMessage::Previous => text.literal("前へ"),
        PageMessage::Next => text.literal("次へ"),
        PageMessage::PageOf(page, pages) => {
            text.number(u64::from(page));
            text.literal(" / ");
            text.number(u64::from(pages));
            text.literal(" ページ");
        }
        PageMessage::EditProfile => text.literal("プロフィールを編集"),
        PageMessage::Joined(date) => {
            text.argument(date);
            text.literal(" に登録");
        }
        PageMessage::Profile => text.literal("プロフィール"),
        PageMessage::Repositories => text.literal("リポジトリ"),
        PageMessage::StarsTab => text.literal("スター"),
        PageMessage::NewRepository => text.literal("新しいリポジトリ"),
        PageMessage::NoStars(name) => {
            text.argument(name);
            text.literal(" はまだリポジトリにスターを付けていません。");
        }
        PageMessage::NoPublicRepositories(name) => {
            text.argument(name);
            text.literal(" にはまだパブリックリポジトリがありません。");
        }
    }
}
