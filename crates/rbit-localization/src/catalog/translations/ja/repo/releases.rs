//! Japanese (`ja`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};

entries! { Ja:
    ReleaseLabel => MessageWriter |text| {
        ReleaseLabel::Releases => text.literal("リリース"),
        ReleaseLabel::NewRelease => text.literal("新しいリリース"),
        ReleaseLabel::EditRelease => text.literal("リリースを編集"),
        ReleaseLabel::NoReleasesYet => text.literal("リリースはまだありません"),
        ReleaseLabel::EmptyForWriters => text.literal("リリースは、タグにノートとファイルをまとめたものです。タグをプッシュしてから、そのリリースを作成してください。"),
        ReleaseLabel::EmptyForReaders => text.literal("リリースは、このプロジェクトのバージョンをノートとダウンロードとともに示します。"),
        ReleaseLabel::Draft => text.literal("ドラフト"),
        ReleaseLabel::PreRelease => text.literal("プレリリース"),
        ReleaseLabel::Latest => text.literal("最新"),
        ReleaseLabel::Bot => text.literal("ボット"),
        ReleaseLabel::Created => text.literal("作成："),
        ReleaseLabel::Edit => text.literal("編集"),
        ReleaseLabel::Downloads => text.literal("ダウンロード"),
        ReleaseLabel::SourceZip => text.literal("ソースコード（zip）"),
        ReleaseLabel::SourceTarGz => text.literal("ソースコード（tar.gz）"),
        ReleaseLabel::Files => text.literal("ファイル"),
        ReleaseLabel::Remove => text.literal("削除"),
        ReleaseLabel::FileToAttach => text.literal("添付するファイル"),
        ReleaseLabel::Upload => text.literal("アップロード"),
        ReleaseLabel::DeleteTitle => text.literal("このリリースを削除"),
        ReleaseLabel::DeleteHintStart => text.literal("リリースとそのノート、ファイルが削除されます。タグ"),
        ReleaseLabel::DeleteHintEnd => text.literal("は残ります。"),
        ReleaseLabel::DeleteRelease => text.literal("リリースを削除"),
        ReleaseLabel::FormLead => text.literal("リリースはタグについて、そのノートとダウンロード用のファイルを記述します。"),
        ReleaseLabel::Tag => text.literal("タグ"),
        ReleaseLabel::AnExisting => text.literal("既存の"),
        ReleaseLabel::TagLink => text.literal("タグ"),
        ReleaseLabel::OrNewTag => text.literal("、または下のブランチから作成する新しいタグ。"),
        ReleaseLabel::CreateTagFrom => text.literal("新しいタグの作成元"),
        ReleaseLabel::TargetHint => text.literal("タグが存在しない場合にのみ使用されます"),
        ReleaseLabel::OneOfTags => text.literal("リポジトリの既存の"),
        ReleaseLabel::TagsEnd => text.literal("。"),
        ReleaseLabel::TagsLink => text.literal("タグ"),
        ReleaseLabel::Title => text.literal("タイトル"),
        ReleaseLabel::TitleHint => text.literal("任意。省略するとタグ名になります"),
        ReleaseLabel::Notes => text.literal("ノート"),
        ReleaseLabel::IsPreRelease => text.literal("プレリリースとしてマーク"),
        ReleaseLabel::PreReleaseHint => text.literal("本番環境向けではないことを示します。最新リリースには正式リリースが優先されます。"),
        ReleaseLabel::SaveRelease => text.literal("リリースを保存"),
        ReleaseLabel::PublishRelease => text.literal("リリースを公開"),
        ReleaseLabel::SaveDraft => text.literal("ドラフトを保存"),
        ReleaseLabel::Cancel => text.literal("キャンセル"),
    }
    ReleaseMessage<'a> => MessageWriter |text| {
        ReleaseMessage::Checksum { sha256 } => {
            text.literal("SHA-256 ");
            text.argument(sha256);
        }
        ReleaseMessage::Downloads { count } => {
            text.number(count);
            text.literal(" 回ダウンロード");
        }
        ReleaseMessage::RemoveFile { name } => {
            text.argument(name);
            text.literal(" を削除");
        }
        ReleaseMessage::UploadLimit { size } => {
            text.literal("1 ファイルあたり最大 ");
            text.argument(size);
            text.literal(" です。ダウンロードは常にファイルとして提供され、ブラウザーで表示されることはありません。");
        }
        ReleaseMessage::Uploaded { name } => {
            text.argument(name);
            text.literal(" をアップロードしました。");
        }
        ReleaseMessage::Removed { name } => {
            text.argument(name);
            text.literal(" を削除しました。");
        }
        ReleaseMessage::EditTitle { tag } => {
            text.argument(tag);
            text.literal(" を編集");
        }
    }
}
