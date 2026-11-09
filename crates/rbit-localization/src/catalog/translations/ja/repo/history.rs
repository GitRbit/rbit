//! Japanese (`ja`) entries for `catalog/repo/history.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::history::{RepoHistoryLabel, RepoHistoryMessage};

entries! { Ja:
    RepoHistoryLabel => MessageWriter |text| {
        RepoHistoryLabel::BrowseFilesAtCommit => text.literal("このコミット時点のファイルを表示"),
        RepoHistoryLabel::BrowseFiles => text.literal("ファイルを表示"),
        RepoHistoryLabel::Pagination => text.literal("ページ送り"),
        RepoHistoryLabel::Newer => text.literal("新しいコミット"),
        RepoHistoryLabel::Older => text.literal("古いコミット"),
        RepoHistoryLabel::Committed => text.literal("がコミット"),
        RepoHistoryLabel::CommittedBy => text.literal("コミッター："),
        RepoHistoryLabel::Commit => text.literal("コミット"),
        RepoHistoryLabel::Checks => text.literal("チェック"),
        RepoHistoryLabel::ViewFile => text.literal("ファイルを表示"),
        RepoHistoryLabel::PathInThatCommit => text.literal("そのコミットでのパス"),
        RepoHistoryLabel::FileEmpty => text.literal("このファイルは空です。"),
        RepoHistoryLabel::BinaryCannotBeBlamed => text.literal("バイナリファイルは Blame を表示できません。"),
        RepoHistoryLabel::TooLargeToBlame => text.literal("このファイルは大きすぎるため Blame を表示できません。"),
        RepoHistoryLabel::ViewTheFile => text.literal("ファイルを表示"),
        RepoHistoryLabel::CompareChanges => text.literal("変更を比較"),
        RepoHistoryLabel::CompareLead => text.literal("ベースと、それと比較するブランチ、タグ、コミットを選択してください。フォーク間の比較もできます。"),
        RepoHistoryLabel::Comparison => text.literal("比較"),
        RepoHistoryLabel::MergeBase => text.literal("マージベース"),
        RepoHistoryLabel::Swap => text.literal("入れ替え"),
        RepoHistoryLabel::CreatePullRequest => text.literal("プルリクエストを作成"),
        RepoHistoryLabel::UnrelatedHistories => text.literal("これらのリビジョンは履歴がまったく異なるため、表示できる差分はありません。"),
        RepoHistoryLabel::NothingToCompare => text.literal("比較する内容がありません"),
        RepoHistoryLabel::NothingToCompareText => text.literal("比較対象のリビジョンの内容はすべてベースに含まれています。"),
        RepoHistoryLabel::Commits => text.literal("コミット"),
        RepoHistoryLabel::Compare => text.literal("比較"),
        RepoHistoryLabel::CompareWithDefault => text.literal("デフォルトブランチと比較"),
        RepoHistoryLabel::BranchView => text.literal("ブランチの表示"),
        RepoHistoryLabel::List => text.literal("一覧"),
        RepoHistoryLabel::GitTree => text.literal("Git ツリー"),
        RepoHistoryLabel::NoMatchingRefs => text.literal("一致する参照はありません。"),
        RepoHistoryLabel::NoBranchesYet => text.literal("ブランチはまだありません"),
        RepoHistoryLabel::NoBranchesText => text.literal("コミットをプッシュすると最初のブランチが作成されます。"),
        RepoHistoryLabel::NoTagsYet => text.literal("タグはまだありません"),
        RepoHistoryLabel::NoTagsText => text.literal("タグはリリースなど、履歴上の重要な時点を示します。"),
        RepoHistoryLabel::Protected => text.literal("保護"),
        RepoHistoryLabel::ProtectedTitle => text.literal("このブランチにはブランチ保護ルールが適用されています"),
        RepoHistoryLabel::DownloadTarGz => text.literal("tar.gz をダウンロード"),
        RepoHistoryLabel::BranchCommitGraph => text.literal("ブランチのコミットグラフ"),
        RepoHistoryLabel::GraphHelp => text.literal("色付きの線をたどると履歴を追えます。トラックを選択すると、そのコミットが強調表示されます。"),
        RepoHistoryLabel::ClearHighlight => text.literal("強調表示を解除"),
        RepoHistoryLabel::HistoryConnections => text.literal("履歴のつながり"),
        RepoHistoryLabel::CommitDetails => text.literal("コミットの詳細"),
        RepoHistoryLabel::Merge => text.literal("マージ"),
        RepoHistoryLabel::GraphTruncated => text.literal("全ブランチの最新 200 件のコミットを表示しています。下端の線はさらに古い履歴へ続きます。さらに調べるには、ブランチの履歴を開いてください。"),
        RepoHistoryLabel::LinesAdded => text.literal("追加"),
        RepoHistoryLabel::LinesDeleted => text.literal("削除"),
        RepoHistoryLabel::StatusAdded => text.literal("追加"),
        RepoHistoryLabel::StatusDeleted => text.literal("削除"),
        RepoHistoryLabel::StatusModified => text.literal("変更"),
        RepoHistoryLabel::StatusRenamed => text.literal("名前変更"),
        RepoHistoryLabel::StatusCopied => text.literal("コピー"),
        RepoHistoryLabel::ChangedFiles => text.literal("変更されたファイル"),
        RepoHistoryLabel::FileTree => text.literal("ファイルツリー"),
        RepoHistoryLabel::FilterChangedFiles => text.literal("変更されたファイルを絞り込み"),
        RepoHistoryLabel::FilterFiles => text.literal("ファイルを絞り込み…"),
        RepoHistoryLabel::NoFilesMatch => text.literal("一致するファイルはありません。"),
        RepoHistoryLabel::Changes => text.literal("変更"),
        RepoHistoryLabel::ToggleFileTree => text.literal("ファイルツリーの表示切り替え"),
        RepoHistoryLabel::CollapseAll => text.literal("すべて折りたたむ"),
        RepoHistoryLabel::ExpandAll => text.literal("すべて展開"),
        RepoHistoryLabel::DiffTruncated => text.literal("この変更は非常に大きいため、一部の差分は表示されません。変更全体を確認するには、リポジトリをクローンしてください。"),
        RepoHistoryLabel::CollapseHint => text.literal("クリックで折りたたみ／展開 · Alt キーを押しながらクリックですべてのファイル"),
        RepoHistoryLabel::CopyPath => text.literal("パスをコピー"),
        RepoHistoryLabel::ViewFileAtCommit => text.literal("このコミット時点のファイルを表示"),
        RepoHistoryLabel::View => text.literal("表示"),
        RepoHistoryLabel::BinaryNotShown => text.literal("バイナリファイルは表示されません。"),
        RepoHistoryLabel::DiffTooLarge => text.literal("差分が大きすぎるため表示できません。"),
        RepoHistoryLabel::SubmoduleCommit => text.literal("サブモジュールのコミット"),
        RepoHistoryLabel::NoContentChanges => text.literal("内容の変更はありません。"),
    }
    RepoHistoryMessage<'a> => MessageWriter |text| {
        RepoHistoryMessage::CommitsOn { date } => {
            text.argument(date);
            text.literal(" のコミット");
        }
        RepoHistoryMessage::ViewCommit { id } => {
            text.literal("コミット ");
            text.argument(id);
            text.literal(" を表示");
        }
        RepoHistoryMessage::ParentNoun { .. } => text.literal("親"),
        RepoHistoryMessage::BlameOf { file } => {
            text.argument(file);
            text.literal(" の Blame");
        }
        RepoHistoryMessage::BlameTitle { path } => {
            text.literal("Blame ");
            text.argument(path);
        }
        RepoHistoryMessage::Ahead { count } => {
            text.argument(count);
            text.literal(" 件先行");
        }
        RepoHistoryMessage::Behind { count } => {
            text.argument(count);
            text.literal(" 件遅れ");
        }
        RepoHistoryMessage::OnlyNewest { count } => {
            text.literal("最新の ");
            text.number(count as u64);
            text.literal(" 件のコミットのみ表示しています。");
        }
        RepoHistoryMessage::HighlightTrack { track } => {
            text.literal("トラック ");
            text.number(track as u64);
            text.literal(" を強調表示");
        }
        RepoHistoryMessage::Track { track } => {
            text.literal("トラック ");
            text.number(track as u64);
        }
        RepoHistoryMessage::BranchName { name } => {
            text.literal("ブランチ：");
            text.argument(name);
        }
        RepoHistoryMessage::MoreBranches { count } => {
            text.literal("ほか ");
            text.number(count as u64);
            text.literal(" 件のブランチ");
        }
        RepoHistoryMessage::FilesChangedNoun { .. } => text.literal("個のファイルを変更"),
        RepoHistoryMessage::CompareTitle { base, head } => {
            text.literal("比較 ");
            text.argument(base);
            text.literal("...");
            text.argument(head);
        }
    }
}
