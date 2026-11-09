//! Japanese (`ja`) entries for `catalog/wiki.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::wiki::WikiMessage;

/// A revision number as the English arm writes it.
fn number(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

entries! { Ja:
    WikiMessage<'a> => MessageWriter |text| {
        WikiMessage::Tab => text.literal("Wiki"),
        WikiMessage::Pages => text.literal("ページ"),
        WikiMessage::AllPages => text.literal("すべてのページ"),
        WikiMessage::PagesLabel => text.literal("Wiki のページ"),
        WikiMessage::BreadcrumbLabel => text.literal("パンくずリスト"),
        WikiMessage::SearchPages => text.literal("ページを検索"),
        WikiMessage::Search => text.literal("検索"),
        WikiMessage::NewPage => text.literal("新しいページ"),
        WikiMessage::EditPage => text.literal("ページを編集"),
        WikiMessage::Edit => text.literal("編集"),
        WikiMessage::History => text.literal("履歴"),
        WikiMessage::Delete => text.literal("削除"),
        WikiMessage::LastEdited => text.literal("最終編集"),
        WikiMessage::Revision { generation } => {
            text.literal("版 ");
            text.number(number(generation));
        }
        WikiMessage::RevisionColumn => text.literal("版"),
        WikiMessage::Author => text.literal("作成者"),
        WikiMessage::Date => text.literal("日付"),
        WikiMessage::UnknownAuthor => text.literal("不明な作成者"),
        WikiMessage::Created => text.literal("作成"),
        WikiMessage::Edited => text.literal("編集"),
        WikiMessage::Renamed => text.literal("名前変更"),
        WikiMessage::Deleted => text.literal("削除"),
        WikiMessage::Restored => text.literal("復元"),
        WikiMessage::View => text.literal("表示"),
        WikiMessage::CompareWithPrevious => text.literal("前の版と比較"),
        WikiMessage::Restore => text.literal("この版を復元"),
        WikiMessage::OlderRevisions => text.literal("以前の版"),
        WikiMessage::OldRevisionNotice => text.literal("このページの以前の版を表示しています。"),
        WikiMessage::ViewCurrent => text.literal("現在のページを表示"),
        WikiMessage::HistoryOf { title } => {
            text.argument(title);
            text.literal(" の履歴");
        }
        WikiMessage::Comparing { base, head } => {
            text.literal("版 ");
            text.number(number(base));
            text.literal(" から版 ");
            text.number(number(head));
            text.literal(" への変更");
        }
        WikiMessage::Changes { additions, deletions } => {
            text.number(additions);
            text.literal(" 行の追加、");
            text.number(deletions);
            text.literal(" 行の削除");
        }
        WikiMessage::NoDifferences => text.literal("これらの版の内容は同じです。"),
        WikiMessage::Title => text.literal("タイトル"),
        WikiMessage::Path => text.literal("パス"),
        WikiMessage::PathHint => text.literal(
            "英字、数字、- _ . を / で区切って使用できます。空欄にするとタイトルから生成されます。",
        ),
        WikiMessage::Content => text.literal("内容"),
        WikiMessage::Markdown => text.literal("Markdown"),
        WikiMessage::Save => text.literal("ページを保存"),
        WikiMessage::Preview => text.literal("プレビュー"),
        WikiMessage::NothingToPreview => text.literal("プレビューする内容がありません。"),
        WikiMessage::Cancel => text.literal("キャンセル"),
        WikiMessage::TitleEmpty => text.literal("タイトルを入力してください。"),
        WikiMessage::TitleTooLong => text.literal("タイトルが長すぎます。"),
        WikiMessage::TitleInvalid => text.literal("タイトルに使用できない文字が含まれています。"),
        WikiMessage::PathEmpty => text.literal("パス、またはパスの生成元となるタイトルを入力してください。"),
        WikiMessage::PathTooLong => text.literal("パスまたはその一部が長すぎます。"),
        WikiMessage::PathTooDeep => text.literal("パスの階層が多すぎます。"),
        WikiMessage::PathInvalid => text.literal(
            "パスの各部分は英字または数字で始め、英字、数字、- _ . のみを使用してください。",
        ),
        WikiMessage::PathReserved => text.literal("パスに予約済みの名前が使われています。"),
        WikiMessage::ContentTooLarge => text.literal("内容が大きすぎます。"),
        WikiMessage::ContentInvalid => text.literal("内容に使用できない文字が含まれています。"),
        WikiMessage::Conflict => text.literal(
            "編集を始めた後にこのページが変更されました。入力したテキストは下に残っています。もう一度保存する前に現在のページを確認してください。",
        ),
        WikiMessage::CompareChanges => text.literal("変更を比較"),
        WikiMessage::PathTaken => text.literal("このパスは別のページで使用されています。"),
        WikiMessage::Unchanged => text.literal("変更はありません。"),
        WikiMessage::DeleteTitle { title } => {
            text.argument(title);
            text.literal(" を削除しますか？");
        }
        WikiMessage::DeletePage => text.literal("ページを削除"),
        WikiMessage::Disabled => text.literal("このリポジトリでは Wiki が無効になっています。"),
        WikiMessage::About => text.literal(
            "Wiki には、コードとは別に独自の履歴を持つドキュメントページを置けます。",
        ),
        WikiMessage::TurnOn => text.literal("Wiki を有効にする"),
        WikiMessage::TurnOff => text.literal("Wiki を無効にする"),
        WikiMessage::Empty => text.literal("この Wiki にはまだページがありません。"),
        WikiMessage::CreateFirst => text.literal("最初のページを作成"),
        WikiMessage::Missing => text.literal("このパスのページはありません。"),
        WikiMessage::CreatePath { path } => {
            text.argument(path);
            text.literal(" を作成");
        }
        WikiMessage::SearchCount { count, query } => {
            text.literal("「");
            text.argument(query);
            text.literal("」に一致するページ：");
            text.number(count);
            text.literal(" 件");
        }
        WikiMessage::NoResults { query } => {
            text.literal("「");
            text.argument(query);
            text.literal("」に一致するページはありません。");
        }
        WikiMessage::MorePages => text.literal("その他のページ"),
        WikiMessage::DeleteKeepsHistory => text.literal(
            "このページは Wiki から削除されます。履歴は保持され、書き込み権限を持つユーザーは削除済みのページから復元できます。",
        ),
        WikiMessage::DeletedPages => text.literal("削除済みのページ"),
        WikiMessage::NoDeletedPages => text.literal("削除されたページはありません。"),
        WikiMessage::DeletedNotice => text.literal("このページは削除されました。履歴は保持されています。"),
        WikiMessage::RestorePage => text.literal("ページを復元"),
        WikiMessage::MoreDeletedPages => text.literal("以前の削除"),
        WikiMessage::RestoreBlocked => text.literal(
            "このパスは現在別のページで使用されています。先にそのページの名前を変更してから、このページを復元してください。",
        ),
        WikiMessage::RenamedFrom { path } => {
            text.argument(path);
            text.literal(" から名前変更");
        }
    }
}
