//! Japanese (`ja`) entries for `catalog/settings/profile.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::profile::GallerySentence;
use crate::catalog::settings::profile::ProfileSentence;
use crate::catalog::settings::profile::{
    AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage,
};

entries! { Ja:
    ProfileMessage => MessageWriter |text| {
        ProfileMessage::Title => text.literal("プロフィール"),
        ProfileMessage::IntroLead => text.literal("ここでの内容は"),
        ProfileMessage::IntroLink => text.literal("プロフィールページ"),
        ProfileMessage::IntroTail => text.literal("やアクティビティの横に表示されます。すべて公開され、入力は任意です。"),
        ProfileMessage::Updated => text.literal("プロフィールを更新しました。"),
        ProfileMessage::DisplayName => text.literal("表示名"),
        ProfileMessage::DisplayNameHint => text.literal("スペースがある場所では、ユーザー名の代わりに表示されます。"),
        ProfileMessage::Bio => text.literal("自己紹介"),
        ProfileMessage::BioHint => text.literal("300 文字以内。"),
        ProfileMessage::Website => text.literal("Web サイト"),
        ProfileMessage::WebsitePlaceholder => text.literal("https://"),
        ProfileMessage::Location => text.literal("所在地"),
        ProfileMessage::Save => text.literal("プロフィールを保存"),
    }
    AppearanceMessage => MessageWriter |text| {
        AppearanceMessage::Title => text.literal("外観"),
        AppearanceMessage::Intro => text.literal("テーマはアカウントに保存され、サインインするすべてのデバイスで使われます。テーマにカーソルを合わせるとプレビューし、クリックすると適用します。"),
    }
    NotificationsMessage => MessageWriter |text| {
        NotificationsMessage::Title => text.literal("通知"),
        NotificationsMessage::IntroLead => text.literal("すべての通知は引き続き"),
        NotificationsMessage::IntroLink => text.literal("受信トレイ"),
        NotificationsMessage::IntroTail => text.literal("に表示されます。メールアドレスにも送信する通知を選択してください。"),
        NotificationsMessage::Saved => text.literal("設定を保存しました。"),
        NotificationsMessage::Unavailable => text.literal("このサーバーではメールを利用できないため、通知は受信トレイにのみ表示されます。"),
        NotificationsMessage::VerifyLead => text.literal("通知メールを受け取るには、"),
        NotificationsMessage::VerifyMiddle => text.literal(" を"),
        NotificationsMessage::VerifyLink => text.literal("アカウント設定"),
        NotificationsMessage::VerifyTail => text.literal("で確認してください。"),
        NotificationsMessage::Email => text.literal("メール"),
        NotificationsMessage::SendTo => text.literal("送信先"),
        NotificationsMessage::When => text.literal("タイミング"),
        NotificationsMessage::DigestTime => text.literal("ダイジェストの送信時刻"),
        NotificationsMessage::DigestHint => text.literal("ダイジェストには、ここで既読にしたスレッドは含まれません。すべてのメールに、ワンクリックで配信を停止できるリンクが付いています。"),
        NotificationsMessage::Save => text.literal("設定を保存"),
        NotificationsMessage::Off => text.literal("オフ"),
        NotificationsMessage::OffText => text.literal("通知は受信トレイにのみ表示されます。"),
        NotificationsMessage::Participating => text.literal("参加中"),
        NotificationsMessage::ParticipatingText => text.literal("自分が作成、コメント、レビューしたスレッドと、メンションされたとき、担当者に指定されたとき、レビューを依頼されたときに通知します。"),
        NotificationsMessage::Everything => text.literal("すべて"),
        NotificationsMessage::EverythingText => text.literal("参加していない購読中のスレッドも通知します。"),
        NotificationsMessage::Immediately => text.literal("すぐに送信"),
        NotificationsMessage::ImmediatelyText => text.literal("通知が発生するたびに 1 通ずつ送信します。"),
        NotificationsMessage::Daily => text.literal("毎日のダイジェスト"),
        NotificationsMessage::DailyText => text.literal("前回以降のすべての通知を 1 日 1 通にまとめて送信します。"),
        NotificationsMessage::Weekly => text.literal("毎週のダイジェスト"),
        NotificationsMessage::WeeklyText => text.literal("前週のすべての通知を毎週月曜日に 1 通にまとめて送信します。"),
    }
    NotificationsText<'a> => MessageWriter |text| {
        NotificationsText::DefaultHour(hour) => {
            text.literal("デフォルト（");
            text.argument(hour);
            text.literal("）");
        }
    }
    GalleryMessage => MessageWriter |text| {
        GalleryMessage::Show => text.literal("表示"),
        GalleryMessage::Search => text.literal("テーマを検索"),
        GalleryMessage::BuiltIn => text.literal("組み込み"),
        GalleryMessage::Classic => text.literal("rbit Classic"),
        GalleryMessage::Empty => text.literal("検索に一致するテーマはありません。"),
        GalleryMessage::CreditLink => text.literal("tinted-theming"),
        GalleryMessage::Licence => text.literal("ライセンスとクレジット"),
    }
    GalleryText => MessageWriter |text| {
        GalleryText::SearchCount(total) => {
            text.number(total);
            text.literal(" 個のテーマを検索");
        }
    }
    GallerySentence => RichWriter |text| {
        GallerySentence::Credit => {
            text.literal("Tinted テーマは ");
            text.slot(0);
            text.literal(" の base16 および base24 スキーム（MIT）から生成されています。テーマにカーソルを合わせると作者が表示されます。");
            text.slot(1);
        }
    }
    ProfileSentence => RichWriter |text| {
        ProfileSentence::ProfileIntro => {
            text.literal("ここでの内容は");
            text.slot(0);
            text.literal("やアクティビティの横に表示されます。すべて公開され、入力は任意です。");
        }
        ProfileSentence::NotificationsIntro => {
            text.literal("すべての通知は引き続き");
            text.slot(0);
            text.literal("に表示されます。メールアドレスにも送信する通知を選択してください。");
        }
    }
}
