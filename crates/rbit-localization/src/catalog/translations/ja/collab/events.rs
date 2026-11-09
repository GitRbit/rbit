//! Japanese (`ja`) entries for `catalog/collab/events.rs`.

use crate::catalog::collab::events::{CloseReason, TimelineEventMessage, TimelineNoteMessage};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;

/// Slot 0 (the actor) and the subject marker that follows it.
fn actor(text: &mut RichWriter) {
    text.slot(0);
    text.literal(" が");
}

/// How an issue or pull request was closed, as the tail of the sentence.
const fn closed_as(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::Completed => "これを完了としてクローズしました",
        CloseReason::NotPlanned => "これを計画なしとしてクローズしました",
        CloseReason::Merged => "これをマージ済みとしてクローズしました",
    }
}

entries! { Ja:
    TimelineEventMessage => RichWriter |text| {
        TimelineEventMessage::Closed { reason } => {
            actor(text);
            text.literal(closed_as(reason));
        }
        TimelineEventMessage::ClosedIn { reason } => {
            actor(text);
            text.literal(" ");
            text.slot(1);
            text.literal(" で");
            text.literal(closed_as(reason));
        }
        TimelineEventMessage::Reopened => {
            actor(text);
            text.literal("これを再オープンしました");
        }
        TimelineEventMessage::Renamed => {
            actor(text);
            text.literal("タイトルを ");
            text.slot(1);
            text.literal(" から ");
            text.slot(2);
            text.literal(" に変更しました");
        }
        TimelineEventMessage::Locked => {
            actor(text);
            text.literal("会話をロックし、コラボレーターのみに制限しました");
        }
        TimelineEventMessage::Unlocked => {
            actor(text);
            text.literal("この会話のロックを解除しました");
        }
        TimelineEventMessage::Labeled => {
            actor(text);
            text.literal("ラベル ");
            text.slot(1);
            text.literal(" を追加しました");
        }
        TimelineEventMessage::Unlabeled => {
            actor(text);
            text.literal("ラベル ");
            text.slot(1);
            text.literal(" を削除しました");
        }
        TimelineEventMessage::Milestoned => {
            actor(text);
            text.literal("これをマイルストーン ");
            text.slot(1);
            text.literal(" に追加しました");
        }
        TimelineEventMessage::Demilestoned => {
            actor(text);
            text.literal("これをマイルストーン ");
            text.slot(1);
            text.literal(" から削除しました");
        }
        TimelineEventMessage::SelfAssigned => {
            actor(text);
            text.literal("自分を担当者に割り当てました");
        }
        TimelineEventMessage::Assigned => {
            actor(text);
            text.literal(" ");
            text.slot(1);
            text.literal(" を担当者に割り当てました");
        }
        TimelineEventMessage::SelfUnassigned => {
            actor(text);
            text.literal("自分の担当を外しました");
        }
        TimelineEventMessage::Unassigned => {
            actor(text);
            text.literal(" ");
            text.slot(1);
            text.literal(" の担当を外しました");
        }
        TimelineEventMessage::CommitReferenced => {
            actor(text);
            text.literal("コミット ");
            text.slot(1);
            text.literal(" でこれを参照しました");
        }
        TimelineEventMessage::MentionedInIssue => {
            actor(text);
            text.literal(" Issue ");
            text.slot(1);
            text.literal(" でこれに言及しました");
        }
        TimelineEventMessage::MentionedInPull => {
            actor(text);
            text.literal("プルリクエスト ");
            text.slot(1);
            text.literal(" でこれに言及しました");
        }
        TimelineEventMessage::Mentioned => {
            actor(text);
            text.literal("これに言及しました");
        }
        TimelineEventMessage::ForcePushed => {
            actor(text);
            text.literal(" ");
            text.slot(2);
            text.literal(" から ");
            text.slot(3);
            text.literal(" へフォースプッシュしました");
        }
        TimelineEventMessage::ForcePushedBranch => {
            actor(text);
            text.literal("ブランチ ");
            text.slot(1);
            text.literal(" を ");
            text.slot(2);
            text.literal(" から ");
            text.slot(3);
            text.literal(" へフォースプッシュしました");
        }
        TimelineEventMessage::Pushed { count } => {
            actor(text);
            text.literal(" ");
            text.number(count);
            text.literal(" 件のコミットを追加しました（現在 ");
            text.slot(2);
            text.literal("）");
        }
        TimelineEventMessage::PushedTo { count } => {
            actor(text);
            text.literal(" ");
            text.slot(1);
            text.literal(" に ");
            text.number(count);
            text.literal(" 件のコミットを追加しました（現在 ");
            text.slot(2);
            text.literal("）");
        }
        TimelineEventMessage::BaseChanged => {
            actor(text);
            text.literal("ベースブランチを ");
            text.slot(1);
            text.literal(" から ");
            text.slot(2);
            text.literal(" に変更しました");
        }
        TimelineEventMessage::Merged => {
            actor(text);
            text.literal("コミット ");
            text.slot(1);
            text.literal(" をマージしました");
            text.slot(3);
        }
        TimelineEventMessage::MergedInto => {
            actor(text);
            text.literal("コミット ");
            text.slot(1);
            text.literal(" を ");
            text.slot(2);
            text.literal(" にマージしました");
            text.slot(3);
        }
        TimelineEventMessage::ReviewRequestedSelf => {
            actor(text);
            text.literal("自分にレビューをリクエストしました");
        }
        TimelineEventMessage::ReviewRequested => {
            actor(text);
            text.literal(" ");
            text.slot(1);
            text.literal(" にレビューをリクエストしました");
        }
        TimelineEventMessage::ReviewRequestRemoved => {
            actor(text);
            text.literal(" ");
            text.slot(1);
            text.literal(" へのレビューリクエストを取り消しました");
        }
        TimelineEventMessage::ReviewDismissedOf => {
            actor(text);
            text.literal(" ");
            text.slot(1);
            text.literal(" の");
            text.slot(2);
            text.literal("を却下しました");
        }
        TimelineEventMessage::ReviewDismissed => {
            actor(text);
            text.slot(2);
            text.literal("を却下しました");
        }
        TimelineEventMessage::ThreadResolved => {
            actor(text);
            text.slot(1);
            text.literal("を解決済みにしました");
        }
        TimelineEventMessage::ThreadReopened => {
            actor(text);
            text.slot(1);
            text.literal("を再オープンしました");
        }
        TimelineEventMessage::HeadRefDeleted => {
            actor(text);
            text.literal("ブランチ ");
            text.slot(1);
            text.literal(" を削除しました");
        }
        TimelineEventMessage::ConvertedToDraft => {
            actor(text);
            text.literal("このプルリクエストをドラフトにしました");
        }
        TimelineEventMessage::ReadyForReview => {
            actor(text);
            text.literal("このプルリクエストをレビュー可能にしました");
        }
    }
    TimelineNoteMessage => MessageWriter |text| {
        TimelineNoteMessage::Review => text.literal("レビュー"),
        TimelineNoteMessage::ReviewConversation => text.literal("レビューの会話"),
        TimelineNoteMessage::MergeCommit => text.literal("（マージコミット）"),
        TimelineNoteMessage::Squash => text.literal("（スカッシュ）"),
        TimelineNoteMessage::Rebase => text.literal("（リベース）"),
        TimelineNoteMessage::OutsideInstance => text.literal("（このインスタンス外）"),
    }
}
