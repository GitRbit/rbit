//! Japanese (`ja`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};

entries! { Ja:
    RepoEditLabel => MessageWriter |text| {
        RepoEditLabel::NewFile => text.literal("新しいファイル"),
        RepoEditLabel::Edit => text.literal("編集："),
        RepoEditLabel::AfterName => text.literal("。"),
        RepoEditLabel::On => text.literal("ブランチ："),
        RepoEditLabel::NewFileHint => text.literal("パスに / を含めるとディレクトリを作成できます。"),
        RepoEditLabel::EditHint => text.literal("パスを変更すると、ファイルの名前を変更または移動できます。"),
        RepoEditLabel::BackToFile => text.literal("ファイルに戻る"),
        RepoEditLabel::Path => text.literal("パス"),
        RepoEditLabel::Content => text.literal("内容"),
        RepoEditLabel::CommitChanges => text.literal("変更をコミット"),
        RepoEditLabel::Cancel => text.literal("キャンセル"),
        RepoEditLabel::Delete => text.literal("削除："),
        RepoEditLabel::DeleteHint => text.literal("ファイルは履歴に残ります。空になったディレクトリは消えます。"),
        RepoEditLabel::DeleteFile => text.literal("ファイルを削除"),
        RepoEditLabel::UploadFiles => text.literal("ファイルをアップロード"),
        RepoEditLabel::Into => text.literal("アップロード先："),
        RepoEditLabel::OnBranch => text.literal("、ブランチ："),
        RepoEditLabel::UploadHint => text.literal("同じ名前のファイルは置き換えられます。"),
        RepoEditLabel::Files => text.literal("ファイル"),
        RepoEditLabel::NothingCommitted => text.literal("何もコミットされませんでした。"),
        RepoEditLabel::ReviewNewCommits => text.literal("新しいコミットを確認"),
        RepoEditLabel::KeptAndCommittedOnTop => text.literal("してから、もう一度コミットしてください。変更内容は下に保持されており、新しいバージョンの上にコミットされます。"),
        RepoEditLabel::BranchGone => text.literal("何もコミットされませんでした。代わりに新しいブランチにコミットしてください。変更内容は下に保持されています。"),
        RepoEditLabel::CommitChangesLegend => text.literal("変更をコミット"),
        RepoEditLabel::CommitMessage => text.literal("コミットメッセージ"),
        RepoEditLabel::ExtendedDescription => text.literal("詳しい説明"),
        RepoEditLabel::Optional => text.literal("任意"),
        RepoEditLabel::CommitDirectlyTo => text.literal("次のブランチに直接コミット："),
        RepoEditLabel::NotAllowedEnd => text.literal("。"),
        RepoEditLabel::NotAllowed => text.literal("許可されていません："),
        RepoEditLabel::NewBranchAndPull => text.literal("新しいブランチを作成してプルリクエストを開始"),
        RepoEditLabel::NewBranchName => text.literal("新しいブランチ名"),
    }
    RepoEditMessage<'a> => MessageWriter |text| {
        RepoEditMessage::AtMost { size } => {
            text.literal("最大 ");
            text.argument(size);
        }
        RepoEditMessage::UploadLimits { files, size } => {
            text.literal("1 ファイルあたり最大 ");
            text.argument(size);
            text.literal("、最大 ");
            text.number(files as u64);
            text.literal(" 個のファイルをまとめてコミットできます。");
        }
        RepoEditMessage::BranchChanged { branch } => {
            text.literal("編集の開始後に ");
            text.argument(branch);
            text.literal(" が変更されました");
        }
        RepoEditMessage::BranchMissing { branch } => {
            text.argument(branch);
            text.literal(" はもう存在しません");
        }
        RepoEditMessage::NowPointsAt => text.literal("現在の参照先："),
        RepoEditMessage::NotEditable { limit } => {
            text.literal("このファイルはバイナリであるか ");
            text.argument(limit);
            text.literal(" を超えているため、ブラウザーでは編集できません。削除するか、置き換えるファイルをアップロードしてください。");
        }
        RepoEditMessage::EditingTitle { path } => {
            text.argument(path);
            text.literal(" を編集中");
        }
        RepoEditMessage::DeleteTitle { path } => {
            text.argument(path);
            text.literal(" を削除");
        }
    }
}
