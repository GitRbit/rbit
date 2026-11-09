//! Korean (`ko`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::{InboxMessage, ThreadState};

entries! { Ko:
    InboxMessage<'a> => MessageWriter |text| {
        InboxMessage::Notifications => text.literal("알림"),
        InboxMessage::Show => text.literal("표시"),
        InboxMessage::Unread => text.literal("읽지 않음"),
        InboxMessage::All => text.literal("전체"),
        InboxMessage::MarkAllRead => text.literal("모두 읽음으로 표시"),
        InboxMessage::NoNotifications => text.literal("알림 없음"),
        InboxMessage::NoNotificationsText => text.literal("참여한 이슈와 풀 리퀘스트의 소식이 여기에 표시됩니다. 직접 열었거나, 댓글을 달았거나, 담당자로 지정되었거나, 멘션된 항목이 해당됩니다."),
        InboxMessage::AllCaughtUp => text.literal("모두 확인했습니다"),
        InboxMessage::AllCaughtUpText => text.literal("읽지 않은 알림이 없습니다. 이전 알림은 전체에서 볼 수 있습니다."),
        InboxMessage::MarkAsRead => text.literal("읽음으로 표시"),
        InboxMessage::Unsubscribe => text.literal("구독 취소"),
        InboxMessage::State(state) => text.literal(match state {
            ThreadState::OpenIssue => "열린 이슈",
            ThreadState::ClosedIssue => "닫힌 이슈",
            ThreadState::OpenPull => "열린 풀 리퀘스트",
            ThreadState::MergedPull => "병합된 풀 리퀘스트",
            ThreadState::ClosedPull => "닫힌 풀 리퀘스트",
        }),
        InboxMessage::StopEmails => text.literal("알림 이메일 중지"),
        InboxMessage::TurnOffFor(username) => {
            text.argument(username);
            text.literal(" 계정의 알림 이메일을 끄시겠습니까? 알림은 받은 알림함에 계속 표시됩니다.");
        }
        InboxMessage::TurnOff => text.literal("알림 이메일 끄기"),
        InboxMessage::EmailsOff => text.literal("알림 이메일이 꺼졌습니다"),
        InboxMessage::NoLongerReceive(username) => {
            text.literal("이제 ");
            text.argument(username);
            text.literal(" 계정으로 알림 이메일이 발송되지 않습니다. 언제든지 다시 켤 수 있습니다.");
        }
        InboxMessage::NotificationSettings => text.literal("알림 설정"),
        InboxMessage::LinkNotValid => text.literal("유효하지 않은 링크입니다"),
        InboxMessage::LinkNotValidText => text.literal("링크가 불완전할 수 있습니다. 로그인하여 이메일로 받을 알림을 선택하세요."),
    }
}
