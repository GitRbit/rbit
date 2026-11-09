//! Japanese (`ja`) entries for `catalog/collab/pulls.rs`.

use crate::catalog::collab::pulls::PullMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    PullMessage<'a> => MessageWriter |text| {
        PullMessage::AuthorFilter { author } => {
            text.literal("作成者：");
            text.argument(author);
        }
        PullMessage::UpToDate { base, head } => {
            text.argument(base);
            text.literal(" には ");
            text.argument(head);
            text.literal(" のすべてのコミットが含まれています。");
        }
        PullMessage::CommitsPhrase { count: Some(count) } => {
            text.number(count);
            text.literal(" 件のコミット");
        }
        PullMessage::CommitsPhrase { count: None } => text.literal("コミット"),
        PullMessage::DeleteBranch { branch } => {
            text.argument(branch);
            text.literal(" を削除");
        }
        PullMessage::NewestCommits { count } => {
            text.literal("最新の ");
            text.number(count);
            text.literal(" 件のコミットのみ表示しています。");
        }
        PullMessage::PullRequests => text.literal("プルリクエスト"),
        PullMessage::SearchPulls => text.literal("プルリクエストを検索"),
        PullMessage::ClearSearch => text.literal("検索と絞り込みをクリア"),
        PullMessage::FiltersLabel => text.literal("プルリクエストの絞り込み"),
        PullMessage::StateNav => text.literal("プルリクエストの状態"),
        PullMessage::New => text.literal("新規"),
        PullMessage::NewSuffix => text.literal("プルリクエスト"),
        PullMessage::EmptyTitle => text.literal("プルリクエストはありません"),
        PullMessage::EmptyText => text.literal("プルリクエストは、ブランチやフォークからの変更を提案します。ブランチを比較して作成してください。"),
        PullMessage::OpenTitle => text.literal("プルリクエストを作成"),
        PullMessage::OpenIntro => text.literal("このリポジトリまたはフォークのブランチの変更を、ベースブランチに提案します。"),
        PullMessage::OpenAsDraft => text.literal("ドラフトとして作成"),
        PullMessage::Create => text.literal("プルリクエストを作成"),
        PullMessage::DifferentHistories => text.literal("これらのブランチの履歴はまったく異なります。"),
        PullMessage::ChooseBranches => text.literal("比較する既存のブランチを 2 つ選択してください。"),
        PullMessage::BaseRepository => text.literal("ベースリポジトリ"),
        PullMessage::BaseRefs => text.literal("ベースのブランチとタグ"),
        PullMessage::Base => text.literal("ベース"),
        PullMessage::HeadRepository => text.literal("ヘッドリポジトリ"),
        PullMessage::Compare => text.literal("比較"),
        PullMessage::CompareRefs => text.literal("比較するブランチとタグ"),
        PullMessage::PullRequest => text.literal("プルリクエスト"),
        PullMessage::Conversation => text.literal("会話"),
        PullMessage::Commits => text.literal("コミット"),
        PullMessage::FilesChanged => text.literal("変更されたファイル"),
        PullMessage::ChangeBase => text.literal("ベースブランチを変更"),
        PullMessage::BaseBranch => text.literal("ベースブランチ"),
        PullMessage::BaseBranches => text.literal("ベースブランチ"),
        PullMessage::ChangeBaseHint => text.literal("ベースを変更すると、コミット、差分、マージチェックが更新されます。"),
        PullMessage::UpdateBase => text.literal("ベースブランチを更新"),
        PullMessage::IsClosed => text.literal("このプルリクエストはクローズされています。"),
        PullMessage::IsMerged => text.literal("プルリクエストはマージされました。"),
        PullMessage::ConvertToDraft => text.literal("ドラフトに変換"),
        PullMessage::NoCommitsTitle => text.literal("コミットはありません"),
        PullMessage::NoCommitsText => text.literal("ヘッドとベースが同じコミットを指しています。"),
    }
}
