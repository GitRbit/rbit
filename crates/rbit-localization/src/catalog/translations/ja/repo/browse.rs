//! Japanese (`ja`) entries for `catalog/repo/browse.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::browse::{RepoBrowseLabel, RepoBrowseMessage};

entries! { Ja:
    RepoBrowseLabel => MessageWriter |text| {
        RepoBrowseLabel::About => text.literal("概要"),
        RepoBrowseLabel::NoDescription => text.literal("説明はありません。"),
        RepoBrowseLabel::Visibility => text.literal("公開範囲"),
        RepoBrowseLabel::History => text.literal("履歴"),
        RepoBrowseLabel::CommitHistory => text.literal("コミット履歴"),
        RepoBrowseLabel::AddFile => text.literal("ファイルを追加"),
        RepoBrowseLabel::UploadFiles => text.literal("ファイルをアップロード"),
        RepoBrowseLabel::Files => text.literal("ファイル"),
        RepoBrowseLabel::ParentDirectory => text.literal("親ディレクトリ"),
        RepoBrowseLabel::Path => text.literal("パス"),
        RepoBrowseLabel::Latest => text.literal("最新"),
        RepoBrowseLabel::YourAccess => text.literal("あなたのアクセス権"),
        RepoBrowseLabel::LeaveRepository => text.literal("リポジトリから脱退"),
        RepoBrowseLabel::Languages => text.literal("言語"),
        RepoBrowseLabel::RepositoryLinks => text.literal("リポジトリのリンク"),
        RepoBrowseLabel::StoredWithLfs => text.literal("Git LFS で保存"),
        RepoBrowseLabel::LfsObjectMissing => text.literal("このサーバーにオブジェクトがありません"),
        RepoBrowseLabel::EditThisFile => text.literal("このファイルを編集"),
        RepoBrowseLabel::DeleteThisFile => text.literal("このファイルを削除"),
        RepoBrowseLabel::Blame => text.literal("Blame"),
        RepoBrowseLabel::Raw => text.literal("Raw"),
        RepoBrowseLabel::Download => text.literal("ダウンロード"),
        RepoBrowseLabel::BinaryNotShown => text.literal("バイナリファイルは表示されません。"),
        RepoBrowseLabel::TooLargeToDisplay => text.literal("このファイルは大きすぎるため表示できません。"),
        RepoBrowseLabel::ViewRaw => text.literal("Raw を表示"),
        RepoBrowseLabel::RepositoryDocuments => text.literal("リポジトリのドキュメント"),
        RepoBrowseLabel::DocumentOutline => text.literal("ドキュメントの目次"),
        RepoBrowseLabel::FilterHeadings => text.literal("見出しを絞り込み"),
        RepoBrowseLabel::DocumentHeadings => text.literal("ドキュメントの見出し"),
        RepoBrowseLabel::NoMatchingHeadings => text.literal("一致する見出しはありません。"),
        RepoBrowseLabel::EmptyTitle => text.literal("このリポジトリは空です"),
        RepoBrowseLabel::EmptyLead => text.literal("既存のプロジェクトをプッシュするか、コマンドラインから新しく始めてください。"),
        RepoBrowseLabel::RepositoryUrl => text.literal("リポジトリの URL"),
        RepoBrowseLabel::CopyRepositoryUrl => text.literal("リポジトリの URL をコピー"),
        RepoBrowseLabel::SshRepositoryUrl => text.literal("リポジトリの SSH URL"),
        RepoBrowseLabel::CopySshRepositoryUrl => text.literal("リポジトリの SSH URL をコピー"),
        RepoBrowseLabel::EmptySshNote => text.literal("以下のコマンドは HTTPS を使用します。SSH でプッシュするには、代わりに SSH URL を使い、次の場所でキーを追加してください："),
        RepoBrowseLabel::CreateNewRepository => text.literal("新しいリポジトリを作成"),
        RepoBrowseLabel::PushExistingRepository => text.literal("既存のリポジトリをプッシュ"),
        RepoBrowseLabel::CloseFileBrowser => text.literal("ファイルブラウザーを閉じる"),
        RepoBrowseLabel::LanguageFiles => text.literal("言語別のファイル"),
        RepoBrowseLabel::NoFilesForLanguage => text.literal("この言語のファイルは見つかりませんでした。"),
        RepoBrowseLabel::NoMatchingFiles => text.literal("一致するファイルはありません。"),
        RepoBrowseLabel::FileContent => text.literal("ファイルの内容"),
        RepoBrowseLabel::SelectFileToPreview => text.literal("ファイルを選択すると内容をプレビューできます。"),
        RepoBrowseLabel::SearchThisListing => text.literal("この一覧を検索"),
        RepoBrowseLabel::SearchNames => text.literal("名前を検索"),
        RepoBrowseLabel::Search => text.literal("検索"),
        RepoBrowseLabel::ListingPages => text.literal("一覧のページ"),
        RepoBrowseLabel::Previous => text.literal("前へ"),
        RepoBrowseLabel::Next => text.literal("次へ"),
        RepoBrowseLabel::BranchesAndTags => text.literal("ブランチとタグ"),
        RepoBrowseLabel::BrowseBranchesAndTags => text.literal("ブランチとタグを参照"),
        RepoBrowseLabel::FindBranchOrTag => text.literal("ブランチまたはタグを検索"),
        RepoBrowseLabel::Find => text.literal("検索"),
        RepoBrowseLabel::Branch => text.literal("ブランチ"),
        RepoBrowseLabel::Tag => text.literal("タグ"),
        RepoBrowseLabel::Default => text.literal("デフォルト"),
        RepoBrowseLabel::NoMatchingRefs => text.literal("一致するブランチやタグはありません。"),
        RepoBrowseLabel::NoForksYet => text.literal("フォークはまだありません"),
        RepoBrowseLabel::ForksEmpty => text.literal("フォークとは、他のユーザーが独立して作業するためのこのリポジトリのコピーです。"),
        RepoBrowseLabel::ForkLead => text.literal("フォークは、リポジトリのブランチとタグをコピーした自分専用のリポジトリです。公開範囲は元のリポジトリと同じになります。"),
        RepoBrowseLabel::ForkLeadPrivate => text.literal("フォークは、リポジトリのブランチとタグをコピーした自分専用のリポジトリです。公開範囲は元のリポジトリと同じになるため、このフォークはプライベートのままです。"),
        RepoBrowseLabel::Owner => text.literal("オーナー"),
        RepoBrowseLabel::Name => text.literal("名前"),
        RepoBrowseLabel::CreateFork => text.literal("フォークを作成"),
        RepoBrowseLabel::Cancel => text.literal("キャンセル"),
        RepoBrowseLabel::NoStargazersYet => text.literal("スターを付けたユーザーはまだいません"),
        RepoBrowseLabel::StargazersEmpty => text.literal("このリポジトリにスターを付けたユーザーがここに表示されます。"),
        RepoBrowseLabel::RepositoryActivity => text.literal("リポジトリのアクティビティ"),
    }
    RepoBrowseMessage<'a> => MessageWriter |text| {
        RepoBrowseMessage::BranchNoun { .. } => text.literal("ブランチ"),
        RepoBrowseMessage::TagNoun { .. } => text.literal("タグ"),
        RepoBrowseMessage::CollaboratorAccess { permission } => {
            text.literal("コラボレーターとして ");
            text.argument(permission);
            text.literal(" 権限があります。");
        }
        RepoBrowseMessage::LineCount { count } => {
            text.number(count as u64);
            text.literal(" 行");
        }
        RepoBrowseMessage::LanguageFiles { language } => {
            text.argument(language);
            text.literal(" のファイル");
        }
        RepoBrowseMessage::ListingResults { total, page, pages } => {
            text.number(total as u64);
            text.literal(" 件 · ");
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
        }
        RepoBrowseMessage::RefPage { page, pages, total } => {
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
            text.literal(" · ");
            text.number(total as u64);
            text.literal(" 件の参照");
        }
        RepoBrowseMessage::ForkTitle { owner, name } => {
            text.argument(owner);
            text.literal("/");
            text.argument(name);
            text.literal(" をフォーク");
        }
    }
}
