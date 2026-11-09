//! Simplified Chinese (`zh-Hans`) entries for `catalog/shell/home.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::home::{Feature, HomeMessage, Involvement};

entries! { ZhHans:
    HomeMessage<'a> => MessageWriter |text| {
        HomeMessage::SearchPublic => text.literal("搜索公开仓库"),
        HomeMessage::Search => text.literal("搜索"),
        HomeMessage::ExploreAll => text.literal("探索所有仓库"),
        HomeMessage::ThisInstance => text.literal("本实例"),
        HomeMessage::RepositoryCount(_) => text.literal("仓库"),
        HomeMessage::UserCount(_) => text.literal("用户"),
        HomeMessage::OrganizationCount(_) => text.literal("组织"),
        HomeMessage::RecentlyUpdated => text.literal("最近更新"),
        HomeMessage::ViewAll => text.literal("查看全部"),
        HomeMessage::NoPublicRepositories => text.literal("还没有公开仓库"),
        HomeMessage::NoPublicRepositoriesText => text.literal("此服务器上公开的仓库会显示在这里。"),
        HomeMessage::FreshServer => text.literal("此服务器还没有任何账户。第一个注册的账户将成为管理员。"),
        HomeMessage::BuiltForSelfHosting => text.literal("为自托管而生"),
        HomeMessage::FeatureTitle(feature) => text.literal(match feature {
            Feature::Binary => "一个快速的可执行文件",
            Feature::Protocols => "通过 HTTPS 和 SSH 使用 Git",
            Feature::Teams => "组织和团队",
            Feature::Releases => "发行版、镜像和 Webhook",
            Feature::Security => "默认安全",
            Feature::Screens => "适配各种屏幕",
        }),
        HomeMessage::FeatureText(feature) => text.literal(match feature {
            Feature::Binary => "SQLite 加内嵌存储。无需安装或运行其他服务。",
            Feature::Protocols => "支持协议 v2 的智能 HTTP、内置 SSH 服务器和部署密钥。",
            Feature::Teams => "按角色与团队和协作者共享仓库。",
            Feature::Releases => "发布发行版、镜像上游项目并通知其他服务。",
            Feature::Security => "双重身份验证登录、限定权限范围的访问令牌和审计日志。",
            Feature::Screens => "自适应至手机尺寸，并可作为应用安装。",
        }),
        HomeMessage::Dashboard => text.literal("仪表板"),
        HomeMessage::ViewProfile => text.literal("查看个人资料"),
        HomeMessage::New => text.literal("新建"),
        HomeMessage::NewRepository => text.literal("新建仓库"),
        HomeMessage::PersonalDashboards => text.literal("个人仪表板"),
        HomeMessage::Issues => text.literal("议题"),
        HomeMessage::PullRequests => text.literal("拉取请求"),
        HomeMessage::Repositories => text.literal("仓库"),
        HomeMessage::PrivateCount(count) => {
            text.number(u64::try_from(count).unwrap_or(u64::MAX));
            text.literal(" 个私有");
        }
        HomeMessage::FindRepository => text.literal("查找仓库…"),
        HomeMessage::FilterRepositories => text.literal("筛选你的仓库"),
        HomeMessage::NoRepositories => text.literal("还没有仓库"),
        HomeMessage::NoRepositoriesText => text.literal("创建一个仓库，开始在此服务器上托管代码。"),
        HomeMessage::CreateRepository => text.literal("创建仓库"),
        HomeMessage::NoFilterMatch => text.literal("没有符合筛选条件的仓库。"),
        HomeMessage::Organizations => text.literal("组织"),
        HomeMessage::NewOrganization => text.literal("新建组织"),
        HomeMessage::NotAMember => text.literal("你还不是任何组织的成员。"),
        HomeMessage::CreateOne => text.literal("创建一个组织"),
        HomeMessage::ShareWithTeam => text.literal("，与团队共享仓库。"),
        HomeMessage::Explore => text.literal("探索"),
        HomeMessage::NothingPublic => text.literal("还没有公开内容。"),
        HomeMessage::YourInvolvement => text.literal("你的参与"),
        HomeMessage::InvolvementTab(tab) => text.literal(match tab {
            Involvement::Created => "我创建的",
            Involvement::Assigned => "分配给我的",
            Involvement::Mentioned => "提及我的",
            Involvement::ReviewRequested => "请求我审查的",
        }),
        HomeMessage::SearchIn(title) => {
            text.literal("搜索");
            text.argument(title);
        }
        HomeMessage::SearchHelp => text.literal("搜索帮助"),
        HomeMessage::SearchHelpText => text.literal(
            "可使用 is:open、is:closed、author:@me、assignee:@me、label:bug、milestone:\"v1\"、no:assignee 或 \
             sort:updated-desc。文本会搜索标题和描述。结果仅包含你当前可读取的仓库。“提及我的”包括正文、评论和已提交审查中的\
             已知提及，即使通知已读也会包含。",
        ),
        HomeMessage::Results => text.literal("结果"),
        HomeMessage::OpenCount(count) => {
            text.number(count);
            text.literal(" 个开放");
        }
        HomeMessage::ClosedCount(count) => {
            text.number(count);
            text.literal(" 个已关闭");
        }
        HomeMessage::NoMatchingThreads => text.literal("没有匹配的讨论"),
        HomeMessage::NoMatchingThreadsText => text.literal("试试其他筛选条件或参与类型标签页。"),
        HomeMessage::CommentCount(count) => {
            text.number(u64::from(count));
            text.literal(" 条评论");
        }
    }
}
