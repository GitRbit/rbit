//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/sentences.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::collab::sentences::CollabSentence;

fn slot_then(text: &mut RichWriter, slot: usize, words: &str) {
    text.slot(slot);
    text.literal(words);
}

/// "a、b、c 或 d" over slots 0–3.
fn examples(text: &mut RichWriter) {
    text.slot(0);
    text.literal("、");
    text.slot(1);
    text.literal("、");
    text.slot(2);
    text.literal(" 或 ");
    text.slot(3);
}

/// "#12 由 alice 于 {0} {verb}".
fn row_by(text: &mut RichWriter, number: &str, author: &str, verb: &str) {
    text.argument(number);
    text.literal(" 由 ");
    text.argument(author);
    text.literal(" 于 ");
    text.slot(0);
    text.literal(verb);
}

/// "{0} {verb} {1} 从 {3} 合并到 {2}".
fn merge_header(text: &mut RichWriter, verb: &str) {
    text.slot(0);
    text.literal(verb);
    text.slot(1);
    text.literal(" 从 ");
    text.slot(3);
    text.literal(" 合并到 ");
    text.slot(2);
}

entries! { ZhHans:
    CollabSentence<'a> => RichWriter |text| {
        CollabSentence::OpenCount { .. } => slot_then(text, 0, " 个开放"),
        CollabSentence::ClosedCount { .. } => slot_then(text, 0, " 个已关闭"),
        CollabSentence::IssueSearchHelp => {
            text.literal("使用菜单筛选结果，或输入诸如 ");
            examples(text);
            text.literal(" 的条件。文本会搜索标题和描述。每个菜单都会保留你的其他筛选条件。");
        }
        CollabSentence::PullSearchHelp => {
            text.literal("在下方选择筛选条件，或输入诸如 ");
            examples(text);
            text.literal(" 的条件。");
        }
        CollabSentence::OpenedIssue { comments } => {
            text.slot(0);
            text.literal(" 于 ");
            text.slot(1);
            text.literal(" 创建了此议题 · ");
            text.number(comments);
            text.literal(" 条评论");
        }
        CollabSentence::OpenedPull { comments } => {
            text.slot(0);
            text.literal(" 于 ");
            text.slot(1);
            text.literal(" 创建了此拉取请求 · ");
            text.number(comments);
            text.literal(" 条评论");
        }
        CollabSentence::RowOpened { number, author } => row_by(text, number, author, " 创建"),
        CollabSentence::RowClosed { number } => {
            text.argument(number);
            text.literal(" 于 ");
            text.slot(0);
            text.literal(" 关闭");
        }
        CollabSentence::RowClosedBy { number, author } => row_by(text, number, author, " 关闭"),
        CollabSentence::RowMergedBy { number, author } => row_by(text, number, author, " 合并"),
        CollabSentence::NoOneAssignYourself => {
            text.literal("无——");
            text.slot(0);
        }
        CollabSentence::PercentComplete => {
            text.literal("已完成 ");
            text.slot(0);
        }
        CollabSentence::WantsToMerge => merge_header(text, " 想要将 "),
        CollabSentence::MergedInto => merge_header(text, " 已将 "),
        CollabSentence::ProposedToMerge => merge_header(text, " 曾提议将 "),
        CollabSentence::SignInToOpenPull => slot_then(text, 0, "后即可发起拉取请求。"),
        CollabSentence::SignInToCommentOnLines => slot_then(text, 0, "后即可对代码行发表评论。"),
        CollabSentence::SignInToJoin => slot_then(text, 0, "后即可参与对话。"),
        CollabSentence::PullMergedAs => {
            text.literal("拉取请求已合并为 ");
            text.slot(0);
            text.literal("。");
        }
        CollabSentence::FilesChanged { .. } => slot_then(text, 0, " 个文件已更改"),
        CollabSentence::CommentOpened => {
            text.literal("创建于 ");
            text.slot(0);
        }
        CollabSentence::CommentCommented => {
            text.literal("评论于 ");
            text.slot(0);
        }
        CollabSentence::ReviewApproved => slot_then(text, 0, " 批准了这些更改"),
        CollabSentence::ReviewChangesRequested => slot_then(text, 0, " 请求了更改"),
        CollabSentence::ReviewDismissed => slot_then(text, 0, " 进行了审查（已驳回）"),
        CollabSentence::Reviewed => slot_then(text, 0, " 进行了审查"),
    }
}
