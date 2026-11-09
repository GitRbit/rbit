//! Japanese (`ja`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};

entries! { Ja:
    OrgMessage => MessageWriter |text| {
        OrgMessage::ChoosePermission => text.literal("権限を選択してください。"),
        OrgMessage::Organization => text.literal("組織"),
        OrgMessage::Repositories => text.literal("リポジトリ"),
        OrgMessage::Members => text.literal("メンバー"),
        OrgMessage::Teams => text.literal("チーム"),
        OrgMessage::Settings => text.literal("設定"),
        OrgMessage::NewOrganization => text.literal("新しい組織"),
        OrgMessage::NewIntro => text.literal("組織はチームに代わってリポジトリを所有します。あなたが最初のオーナーになります。"),
        OrgMessage::Name => text.literal("名前"),
        OrgMessage::NameHint => text.literal("URL に使われます。英字、数字、「-」「_」「.」が使用できます。ユーザーと組織は同じ名前空間を共有します。"),
        OrgMessage::DisplayName => text.literal("表示名"),
        OrgMessage::Optional => text.literal("任意"),
        OrgMessage::Description => text.literal("説明"),
        OrgMessage::Cancel => text.literal("キャンセル"),
        OrgMessage::CreateOrganization => text.literal("組織を作成"),
        OrgMessage::New => text.literal("新規"),
        OrgMessage::NoRepositories => text.literal("リポジトリはまだありません"),
        OrgMessage::NoRepositoriesText => text.literal("この組織が所有するリポジトリがここに表示されます。"),
        OrgMessage::Profile => text.literal("プロフィール"),
        OrgMessage::Website => text.literal("Web サイト"),
        OrgMessage::WebsitePlaceholder => text.literal("https://"),
        OrgMessage::SaveProfile => text.literal("プロフィールを保存"),
        OrgMessage::MemberPermission => text.literal("メンバーの権限"),
        OrgMessage::PermissionLegend => text.literal("すべてのメンバーがすべてのリポジトリで持つ権限"),
        OrgMessage::PermissionHint => text.literal("チームやコラボレーターはこの権限に追加できますが、減らすことはできません。オーナーは常にすべてのリポジトリを管理できます。"),
        OrgMessage::SavePermission => text.literal("権限を保存"),
        OrgMessage::NoPermission => text.literal("権限なし"),
        OrgMessage::NoPermissionText => text.literal("メンバーには、パブリックリポジトリと所属チームがアクセスできるリポジトリだけが表示されます。"),
        OrgMessage::Read => text.literal("読み取り"),
        OrgMessage::ReadText => text.literal("メンバーはすべてのリポジトリを閲覧、クローンできます。"),
        OrgMessage::Write => text.literal("書き込み"),
        OrgMessage::WriteText => text.literal("メンバーはすべてのリポジトリを閲覧、クローンし、プッシュできます。"),
        OrgMessage::DangerZone => text.literal("危険な操作"),
        OrgMessage::DeleteOrganization => text.literal("この組織を削除"),
        OrgMessage::DeleteText => text.literal("組織とそのすべてのメンバーシップが完全に削除されます。元に戻すことはできません。"),
        OrgMessage::TypeToConfirmLead => text.literal("確認のため"),
        OrgMessage::TypeToConfirmTail => text.literal("と入力してください"),
        OrgMessage::DeleteOrganizationButton => text.literal("組織を削除"),
        OrgMessage::AddMember => text.literal("メンバーを追加"),
        OrgMessage::Username => text.literal("ユーザー名"),
        OrgMessage::Role => text.literal("ロール"),
        OrgMessage::Member => text.literal("メンバー"),
        OrgMessage::Owner => text.literal("オーナー"),
        OrgMessage::Add => text.literal("追加"),
        OrgMessage::MembersHint => text.literal("メンバーは、すべてのリポジトリで組織の基本権限に加えて、所属チームが付与する権限を持ちます。オーナーはメンバー、チーム、設定も管理できます。"),
        OrgMessage::You => text.literal("（あなた）"),
        OrgMessage::Leave => text.literal("脱退"),
        OrgMessage::Remove => text.literal("削除"),
        OrgMessage::MakeOwner => text.literal("オーナーにする"),
        OrgMessage::MakeMember => text.literal("メンバーにする"),
    }
    OrgText<'a> => MessageWriter |text| {
        OrgText::StillOwnsOne(org) => {
            text.argument(org);
            text.literal(" はまだリポジトリを 1 件所有しています。先に削除するか移譲してください。");
        }
        OrgText::StillOwns(org, count) => {
            text.argument(org);
            text.literal(" はまだリポジトリを ");
            text.number(count);
            text.literal(" 件所有しています。先に削除するか移譲してください。");
        }
        OrgText::RemoveLabel(name) => {
            text.argument(name);
            text.literal(" を削除");
        }
        OrgText::RoleLabel(action, name) => {
            text.argument(action);
            text.literal("：");
            text.argument(name);
        }
        OrgText::MembersTitle(org) => {
            text.literal("メンバー · ");
            text.argument(org);
        }
        OrgText::SettingsTitle(org) => {
            text.literal("設定 · ");
            text.argument(org);
        }
    }
}
