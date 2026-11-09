//! Japanese (`ja`) entries for `catalog/repo/nav.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::nav::{RepoNavLabel, RepoNavMessage};

entries! { Ja:
    RepoNavLabel => MessageWriter |text| {
        RepoNavLabel::ForkedFrom => text.literal("フォーク元："),
        RepoNavLabel::MirrorOf => text.literal("ミラー元："),
        RepoNavLabel::ArchivedNote => text.literal("このリポジトリはアーカイブされています。読み取り専用のため、クローン、フォーク、スターはできますが、変更はできません。"),
        RepoNavLabel::Private => text.literal("プライベート"),
        RepoNavLabel::Public => text.literal("パブリック"),
        RepoNavLabel::Repository => text.literal("リポジトリ"),
        RepoNavLabel::Code => text.literal("コード"),
        RepoNavLabel::Issues => text.literal("Issue"),
        RepoNavLabel::PullRequests => text.literal("プルリクエスト"),
        RepoNavLabel::Commits => text.literal("コミット"),
        RepoNavLabel::Branches => text.literal("ブランチ"),
        RepoNavLabel::Tags => text.literal("タグ"),
        RepoNavLabel::Releases => text.literal("リリース"),
        RepoNavLabel::Activity => text.literal("アクティビティ"),
        RepoNavLabel::Actions => text.literal("Actions"),
        RepoNavLabel::Settings => text.literal("設定"),
        RepoNavLabel::ForkThisRepository => text.literal("このリポジトリをフォーク"),
        RepoNavLabel::Fork => text.literal("フォーク"),
        RepoNavLabel::Forks => text.literal("フォーク"),
        RepoNavLabel::Clone => text.literal("クローン"),
        RepoNavLabel::CloneUrl => text.literal("クローン URL"),
        RepoNavLabel::CopyCloneUrl => text.literal("クローン URL をコピー"),
        RepoNavLabel::SshCloneUrl => text.literal("SSH クローン URL"),
        RepoNavLabel::CopySshCloneUrl => text.literal("SSH クローン URL をコピー"),
        RepoNavLabel::Copy => text.literal("コピー"),
        RepoNavLabel::SshKeyNote => text.literal("SSH を使うには、キーを登録してください："),
        RepoNavLabel::YourSettings => text.literal("設定"),
        RepoNavLabel::DownloadZip => text.literal("ZIP をダウンロード"),
        RepoNavLabel::Star => text.literal("スター"),
        RepoNavLabel::Starred => text.literal("スター済み"),
        RepoNavLabel::StarThisRepository => text.literal("このリポジトリにスターを付ける"),
        RepoNavLabel::UnstarThisRepository => text.literal("このリポジトリのスターを外す"),
        RepoNavLabel::SignInToStar => text.literal("このリポジトリにスターを付けるにはサインインしてください"),
        RepoNavLabel::Stargazers => text.literal("スターを付けたユーザー"),
    }
    RepoNavMessage => MessageWriter |text| {
        RepoNavMessage::ForkCount { count } => {
            text.literal("フォーク ");
            text.number(count);
            text.literal(" 件");
        }
        RepoNavMessage::StargazerCount { count } => {
            text.literal("スター ");
            text.number(count.unsigned_abs());
            text.literal(" 件");
        }
    }
}
