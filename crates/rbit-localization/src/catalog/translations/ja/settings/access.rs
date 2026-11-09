//! Japanese (`ja`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { Ja:
    AccessMessage => MessageWriter |text| {
        AccessMessage::Settings => text.literal("設定"),
        AccessMessage::Collaborators => text.literal("コラボレーター"),
        AccessMessage::IntroLead => text.literal("ここで追加したユーザーは"),
        AccessMessage::IntroDirectly => text.literal("に直接アクセスできます"),
        AccessMessage::IntroOrganization => text.literal("。組織に所属しているかどうかは問いません"),
        AccessMessage::IntroTail => text.literal("。"),
        AccessMessage::AddCollaborator => text.literal("コラボレーターを追加"),
        AccessMessage::Username => text.literal("ユーザー名"),
        AccessMessage::Permission => text.literal("権限"),
        AccessMessage::Add => text.literal("追加"),
        AccessMessage::People => text.literal("ユーザー"),
        AccessMessage::NoCollaborators => text.literal("コラボレーターはいません"),
        AccessMessage::NoCollaboratorsText => text.literal("ユーザー名で追加すると、このリポジトリへのアクセス権を付与できます。"),
        AccessMessage::You => text.literal("（あなた）"),
        AccessMessage::Save => text.literal("保存"),
        AccessMessage::Leave => text.literal("脱退"),
        AccessMessage::Remove => text.literal("削除"),
        AccessMessage::Teams => text.literal("チーム"),
        AccessMessage::ManageTeams => text.literal("チームを管理"),
        AccessMessage::NoTeams => text.literal("チームはありません"),
        AccessMessage::NoTeamsText => text.literal("チームのページからチームにアクセス権を付与すると、そのメンバーはここでその権限を得ます。"),
        AccessMessage::Transfer => text.literal("オーナーの移管"),
        AccessMessage::TransferLead => text.literal("リポジトリ"),
        AccessMessage::TransferTail => text.literal("を別のアカウントに移管します。履歴、コラボレーター、スターも一緒に移動します。古いアドレスを使うリンクとクローン URL は機能しなくなり、現在の組織のチームはアクセス権を失います。"),
        AccessMessage::NewOwner => text.literal("新しいオーナー"),
        AccessMessage::AdminEntry => text.literal("サイト管理者は任意のユーザーまたは組織を入力できます。"),
        AccessMessage::Organization => text.literal("組織"),
        AccessMessage::OwnerHint => text.literal("自分のアカウント、または所属している組織です。"),
        AccessMessage::TypeLead => text.literal("確認のため"),
        AccessMessage::TypeTail => text.literal("と入力してください"),
        AccessMessage::TransferRepository => text.literal("リポジトリを移管"),
        AccessMessage::Nowhere => text.literal("移管先がありません"),
        AccessMessage::NowhereText => text.literal("リポジトリを作成できる組織に所属しておらず、このリポジトリはすでにあなたのものです。"),
    }
    AccessText<'a> => MessageWriter |text| {
        AccessText::PermissionFor(name) => {
            text.argument(name);
            text.literal(" の権限");
        }
        AccessText::SavePermissionFor(name) => {
            text.argument(name);
            text.literal(" の権限を保存");
        }
        AccessText::RemoveLabel(name) => {
            text.argument(name);
            text.literal(" を削除");
        }
        AccessText::Added(date) => {
            text.argument(date);
            text.literal(" に追加");
        }
        AccessText::CollaboratorsTitle(repository) => {
            text.literal("コラボレーター · ");
            text.argument(repository);
        }
        AccessText::TransferTitle(repository) => {
            text.literal("移管 · ");
            text.argument(repository);
        }
    }
}
