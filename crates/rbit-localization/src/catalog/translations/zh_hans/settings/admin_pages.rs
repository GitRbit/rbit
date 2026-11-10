//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/admin_pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage};

entries! { ZhHans:
    AdminPageMessage => MessageWriter |text| {
        AdminPageMessage::UsersIntro => text.literal("在此实例上拥有账户的所有人。"),
        AdminPageMessage::UsersSearch => text.literal("按用户名、名称或邮箱地址搜索"),
        AdminPageMessage::NewUser => text.literal("新建用户"),
        AdminPageMessage::NoUsers => text.literal("没有找到用户"),
        AdminPageMessage::NoUsersText => text.literal("试试其他搜索。"),
        AdminPageMessage::User => text.literal("用户"),
        AdminPageMessage::Joined => text.literal("加入时间"),
        AdminPageMessage::Admin => text.literal("管理员"),
        AdminPageMessage::Suspended => text.literal("已停用"),
        AdminPageMessage::EmailVerified => text.literal("邮箱地址已验证"),
        AdminPageMessage::Verified => text.literal("已验证"),
        AdminPageMessage::You => text.literal("你"),
        AdminPageMessage::RemoveAdmin => text.literal("取消管理员"),
        AdminPageMessage::MakeAdmin => text.literal("设为管理员"),
        AdminPageMessage::LiftSuspension => text.literal("解除停用"),
        AdminPageMessage::Suspend => text.literal("停用"),
        AdminPageMessage::SendReset => text.literal("发送密码重置邮件"),
        AdminPageMessage::SetPasswordMenu => text.literal("设置密码…"),
        AdminPageMessage::DeleteMenu => text.literal("删除…"),
        AdminPageMessage::ResetSent => text.literal("密码重置邮件已发送。"),
        AdminPageMessage::PasswordSet => text.literal("密码已更新。"),
        AdminPageMessage::NewUserIntro => text.literal("即使注册已关闭，管理员也可以创建账户。"),
        AdminPageMessage::Username => text.literal("用户名"),
        AdminPageMessage::UsernameHint => text.literal("字母、数字、“-”、“_”和“.”"),
        AdminPageMessage::Email => text.literal("邮箱地址"),
        AdminPageMessage::InitialPassword => text.literal("初始密码"),
        AdminPageMessage::SiteAdministrator => text.literal("站点管理员"),
        AdminPageMessage::SiteAdministratorText => text.literal("拥有对所有仓库和此面板的完全访问权限。"),
        AdminPageMessage::CreateUser => text.literal("创建用户"),
        AdminPageMessage::RegistrationClosed => text.literal("公开注册目前已关闭；这是添加用户的唯一方式。"),
        AdminPageMessage::SetPassword => text.literal("设置密码"),
        AdminPageMessage::ResetHint => text.literal("如果想让用户自行设置密码，请在用户列表中向其发送密码重置邮件。"),
        AdminPageMessage::DeleteUser => text.literal("删除用户"),
        AdminPageMessage::DeleteUserLead => text.literal("这将永久删除"),
        AdminPageMessage::OrganizationsIntro => text.literal("共享命名空间及其成员。"),
        AdminPageMessage::SearchByName => text.literal("按名称搜索"),
        AdminPageMessage::NoOrganizations => text.literal("没有找到组织"),
        AdminPageMessage::NoOrganizationsText => text.literal("组织创建后会列在这里。"),
        AdminPageMessage::Organization => text.literal("组织"),
        AdminPageMessage::Members => text.literal("成员"),
        AdminPageMessage::Created => text.literal("创建时间"),
        AdminPageMessage::RepositoriesIntro => text.literal("所有公开和私有仓库，最近活跃的排在最前。"),
        AdminPageMessage::RepositoriesSearch => text.literal("按 owner/name 或描述搜索"),
        AdminPageMessage::NoRepositories => text.literal("没有找到仓库"),
        AdminPageMessage::NoRepositoriesText => text.literal("仓库创建后会列在这里。"),
        AdminPageMessage::Repository => text.literal("仓库"),
        AdminPageMessage::Updated => text.literal("更新时间"),
        AdminPageMessage::DeleteRepository => text.literal("删除仓库"),
        AdminPageMessage::DeleteRepositoryText => {
            text.literal("这将永久删除该仓库，包括其所有 Git 数据。此操作无法撤销。");
        }
        AdminPageMessage::AuditIntro => {
            text.literal("登录、双重身份验证变更、会话、访问令牌和管理员操作，最新的排在最前。");
        }
        AdminPageMessage::AuditSearch => text.literal("用户、对象或 IP 地址"),
        AdminPageMessage::Category => text.literal("类别"),
        AdminPageMessage::Filter => text.literal("筛选"),
        AdminPageMessage::AllEvents => text.literal("所有事件"),
        AdminPageMessage::Clear => text.literal("清除"),
        AdminPageMessage::NoEvents => text.literal("没有事件"),
        AdminPageMessage::NoEventsText => text.literal("还没有符合这些筛选条件的内容。"),
        AdminPageMessage::As => text.literal("身份"),
        AdminPageMessage::By => text.literal("操作者"),
        AdminPageMessage::On => text.literal("对象"),
    }
    InstanceSettingsMessage => MessageWriter |text| {
        InstanceSettingsMessage::Intro => text.literal("更改会立即应用于整个实例。"),
        InstanceSettingsMessage::Saved => text.literal("设置已保存。"),
        InstanceSettingsMessage::SiteName => text.literal("站点名称"),
        InstanceSettingsMessage::SiteNameHint => text.literal("显示在页头、页面标题中，以及作为应用安装时。"),
        InstanceSettingsMessage::Branding => text.literal("品牌与标识"),
        InstanceSettingsMessage::BrandingHint => text.literal(
            "文本为纯文本。图片和链接可使用 HTTPS URL，或由反向代理提供的本地 /路径。图片字段留空则使用内置默认图片。",
        ),
        InstanceSettingsMessage::PrimaryColor => text.literal("主按钮颜色（留空则使用主题颜色）"),
        InstanceSettingsMessage::PrimaryTextColor => text.literal("主按钮文字颜色"),
        InstanceSettingsMessage::ColorExample => text.literal("#2563eb"),
        InstanceSettingsMessage::TextColorExample => text.literal("#ffffff"),
        InstanceSettingsMessage::ColorHint => {
            text.literal("请选择对比度足够的颜色，确保按钮文字清晰可读。两个颜色要么都设置，要么都留空。");
        }
        InstanceSettingsMessage::Description => text.literal("站点描述"),
        InstanceSettingsMessage::WelcomeTitle => text.literal("欢迎标题（留空则使用站点名称）"),
        InstanceSettingsMessage::WelcomeMessage => text.literal("欢迎信息"),
        InstanceSettingsMessage::LogoUrl => text.literal("徽标 URL"),
        InstanceSettingsMessage::FaviconUrl => text.literal("网站图标 URL"),
        InstanceSettingsMessage::AppIconUrl => text.literal("应用图标 URL（正方形，至少 512 像素）"),
        InstanceSettingsMessage::FooterText => text.literal("页脚文本"),
        InstanceSettingsMessage::SupportUrl => text.literal("支持 URL"),
        InstanceSettingsMessage::TermsUrl => text.literal("服务条款 URL"),
        InstanceSettingsMessage::PrivacyUrl => text.literal("隐私政策 URL"),
        InstanceSettingsMessage::ShowPoweredBy => text.literal("显示“Powered by rbit”"),
        InstanceSettingsMessage::ShowVersion => text.literal("显示软件版本"),
        InstanceSettingsMessage::ShowFeatures => text.literal("在欢迎页面上展示自托管功能"),
        InstanceSettingsMessage::Explore => text.literal("探索页面"),
        InstanceSettingsMessage::ExploreEnabled => text.literal("开启"),
        InstanceSettingsMessage::ExploreEnabledText => text.literal("所有可以浏览本站的人都能搜索并列出全部公开仓库。"),
        InstanceSettingsMessage::ExploreDisabled => text.literal("关闭"),
        InstanceSettingsMessage::ExploreDisabledText => text.literal("探索页面将无法访问，其链接也会隐藏，管理员同样如此。公开仓库仍保持公开。"),
        InstanceSettingsMessage::Registration => text.literal("注册"),
        InstanceSettingsMessage::Open => text.literal("开放"),
        InstanceSettingsMessage::OpenText => text.literal("任何人都可以在注册页面创建账户。"),
        InstanceSettingsMessage::Closed => text.literal("关闭"),
        InstanceSettingsMessage::ClosedText => text.literal("只有管理员可以创建账户。"),
        InstanceSettingsMessage::OrgCreation => text.literal("谁可以创建组织"),
        InstanceSettingsMessage::AnyUser => text.literal("任何用户"),
        InstanceSettingsMessage::AnyUserText => text.literal("每个已登录的用户都可以创建组织。"),
        InstanceSettingsMessage::AdminsOnly => text.literal("仅限管理员"),
        InstanceSettingsMessage::AdminsOnlyText => text.literal("用户仍可被添加到现有组织中。"),
        InstanceSettingsMessage::DefaultVisibility => text.literal("仓库默认可见性"),
        InstanceSettingsMessage::Public => text.literal("公开"),
        InstanceSettingsMessage::PublicText => text.literal("除非另行更改，新仓库对所有人可见。"),
        InstanceSettingsMessage::Private => text.literal("私有"),
        InstanceSettingsMessage::PrivateText => text.literal("新仓库默认私有；所有者仍可将其设为公开。"),
        InstanceSettingsMessage::VisibilityHint => text.literal("在新建仓库表单中预先选中。"),
        InstanceSettingsMessage::DigestTime => text.literal("邮件摘要发送时间"),
        InstanceSettingsMessage::DigestHint => text.literal(
            "向尚未选择时间的用户发送每日和每周通知摘要的时间。每周摘要在周一发送。",
        ),
        InstanceSettingsMessage::Save => text.literal("保存设置"),
    }
    AdminPageText<'a> => MessageWriter |text| {
        AdminPageText::ShareSecurely(length) => {
            text.literal("至少 ");
            text.number(length);
            text.literal(" 个字符。请以安全的方式告知用户。用户首次登录时必须更换此密码。");
        }
        AdminPageText::ReplacesPassword(name) => {
            text.literal("新密码会立即替换当前密码，所有密码重置链接也将失效。请以安全的方式告知 ");
            text.argument(name);
            text.literal("。");
        }
        AdminPageText::DeleteUserTail(repositories) => {
            text.literal("及其仓库（");
            text.argument(repositories);
            text.literal("，包括所有 Git 数据）、访问令牌和组织成员关系。此操作无法撤销。");
        }
    }
}
