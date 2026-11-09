//! Japanese (`ja`) entries for `catalog/collab/sentences.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::collab::sentences::CollabSentence;

/// "a、b、c、d" over slots 0–3.
fn examples(text: &mut RichWriter) {
    text.slot(0);
    text.literal("、");
    text.slot(1);
    text.literal("、");
    text.slot(2);
    text.literal("、");
    text.slot(3);
}

/// Under a title: the author (slot 0) opened it when (slot 1), then the comment count.
fn opened(text: &mut RichWriter, words: &str, comments: u64) {
    text.slot(0);
    text.literal(" が ");
    text.slot(1);
    text.literal(words);
    text.number(comments);
    text.literal(" 件のコメント");
}

/// A list row: the number, then who did what and when (slot 0).
fn row(text: &mut RichWriter, number: &str, author: Option<&str>, verb: &str) {
    text.argument(number);
    text.literal(" · ");
    if let Some(author) = author {
        text.argument(author);
        text.literal(" が ");
    }
    text.slot(0);
    text.literal(verb);
}

/// A pull request's header: slot 0 merges the commits (1) from the head (3) into the base (2).
fn merge_header(text: &mut RichWriter, verb: &str) {
    text.slot(0);
    text.literal(" が ");
    text.slot(1);
    text.literal(" を ");
    text.slot(3);
    text.literal(" から ");
    text.slot(2);
    text.literal(verb);
}

entries! { Ja:
    CollabSentence<'a> => RichWriter |text| {
        CollabSentence::OpenCount { .. } => {
            text.literal("オープン ");
            text.slot(0);
        }
        CollabSentence::ClosedCount { .. } => {
            text.literal("クローズ ");
            text.slot(0);
        }
        CollabSentence::IssueSearchHelp => {
            text.literal("メニューで結果を絞り込むか、");
            examples(text);
            text.literal(" などの条件を入力してください。テキストはタイトルと説明から検索されます。各メニューはほかの絞り込みを保持します。");
        }
        CollabSentence::PullSearchHelp => {
            text.literal("下の絞り込みを選択するか、");
            examples(text);
            text.literal(" などの条件を入力してください。");
        }
        CollabSentence::OpenedIssue { comments } => opened(text, " にこの Issue を作成しました · ", comments),
        CollabSentence::OpenedPull { comments } => opened(text, " にこのプルリクエストを作成しました · ", comments),
        CollabSentence::RowOpened { number, author } => row(text, number, Some(author), " に作成"),
        CollabSentence::RowClosed { number } => row(text, number, None, " にクローズ"),
        CollabSentence::RowClosedBy { number, author } => row(text, number, Some(author), " にクローズ"),
        CollabSentence::RowMergedBy { number, author } => row(text, number, Some(author), " にマージ"),
        CollabSentence::NoOneAssignYourself => {
            text.literal("なし — ");
            text.slot(0);
        }
        CollabSentence::PercentComplete => {
            text.slot(0);
            text.literal(" 完了");
        }
        CollabSentence::WantsToMerge => merge_header(text, " にマージしようとしています"),
        CollabSentence::MergedInto => merge_header(text, " にマージしました"),
        CollabSentence::ProposedToMerge => merge_header(text, " にマージすることを提案しました"),
        CollabSentence::SignInToOpenPull => {
            text.literal("プルリクエストを作成するには");
            text.slot(0);
            text.literal("してください。");
        }
        CollabSentence::SignInToCommentOnLines => {
            text.literal("行にコメントするには");
            text.slot(0);
            text.literal("してください。");
        }
        CollabSentence::SignInToJoin => {
            text.literal("会話に参加するには");
            text.slot(0);
            text.literal("してください。");
        }
        CollabSentence::PullMergedAs => {
            text.literal("プルリクエストは ");
            text.slot(0);
            text.literal(" としてマージされました。");
        }
        CollabSentence::FilesChanged { .. } => {
            text.slot(0);
            text.literal(" 個のファイルを変更");
        }
        CollabSentence::CommentOpened => {
            text.slot(0);
            text.literal(" に作成");
        }
        CollabSentence::CommentCommented => {
            text.slot(0);
            text.literal(" にコメント");
        }
        CollabSentence::ReviewApproved => {
            text.slot(0);
            text.literal(" がこれらの変更を承認しました");
        }
        CollabSentence::ReviewChangesRequested => {
            text.slot(0);
            text.literal(" が変更をリクエストしました");
        }
        CollabSentence::ReviewDismissed => {
            text.slot(0);
            text.literal(" がレビューしました（却下済み）");
        }
        CollabSentence::Reviewed => {
            text.slot(0);
            text.literal(" がレビューしました");
        }
    }
}
