//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/admin.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage};

entries! { ZhHans:
    AdminMessage => MessageWriter |text| {
        AdminMessage::SiteAdministration => text.literal("站点管理"),
        AdminMessage::Administration => text.literal("管理"),
        AdminMessage::Dashboard => text.literal("仪表板"),
        AdminMessage::Users => text.literal("用户"),
        AdminMessage::Organizations => text.literal("组织"),
        AdminMessage::Repositories => text.literal("仓库"),
        AdminMessage::Settings => text.literal("设置"),
        AdminMessage::AuditLog => text.literal("审计日志"),
        AdminMessage::Configuration => text.literal("配置"),
        AdminMessage::Runners => text.literal("运行器"),
        AdminMessage::Search => text.literal("搜索"),
        AdminMessage::Pagination => text.literal("分页"),
        AdminMessage::Previous => text.literal("上一页"),
        AdminMessage::Next => text.literal("下一页"),
        AdminMessage::NoResults => text.literal("没有结果"),
        AdminMessage::Cancel => text.literal("取消"),
        AdminMessage::Actions => text.literal("操作"),
        AdminMessage::Private => text.literal("私有"),
        AdminMessage::Archived => text.literal("已归档"),
        AdminMessage::Delete => text.literal("删除"),
        AdminMessage::TypeToConfirmLead => text.literal("输入"),
        AdminMessage::TypeToConfirmTail => text.literal("以确认"),
        AdminMessage::NewPassword => text.literal("新密码"),
        AdminMessage::ConfirmPassword => text.literal("确认新密码"),
        AdminMessage::Unavailable => text.literal("不可用"),
    }
    DashboardMessage => MessageWriter |text| {
        DashboardMessage::Intro => text.literal("此实例的概览。"),
        DashboardMessage::RestartServer => text.literal("重启服务器"),
        DashboardMessage::RestartUnavailableHint => text.literal("此服务器宿主环境不支持重启"),
        DashboardMessage::SoftwareUpdates => text.literal("软件更新"),
        DashboardMessage::ConfigureUpdates => text.literal("配置更新检查和版本隐私"),
        DashboardMessage::LiveMonitoring => text.literal("实时监控"),
        DashboardMessage::MonitoringIntro => text.literal("最近五分钟的流量、响应时间和 Git 活动。"),
        DashboardMessage::Snapshot => text.literal("快照"),
        DashboardMessage::Pause => text.literal("暂停"),
        DashboardMessage::NeedsJavaScript => text.literal("实时更新需要 JavaScript。重新加载此页面可获取最新快照。"),
        DashboardMessage::System => text.literal("系统"),
        DashboardMessage::InstanceSettings => text.literal("实例设置"),
        DashboardMessage::Edit => text.literal("编辑"),
        DashboardMessage::Users => text.literal("用户"),
        DashboardMessage::Organizations => text.literal("组织"),
        DashboardMessage::SharedNamespaces => text.literal("共享命名空间"),
        DashboardMessage::Repositories => text.literal("仓库"),
        DashboardMessage::AccessTokens => text.literal("访问令牌"),
        DashboardMessage::AcrossAllUsers => text.literal("所有用户合计"),
        DashboardMessage::Uptime => text.literal("运行时间"),
        DashboardMessage::DataDirectory => text.literal("数据目录"),
        DashboardMessage::DiskUsage => text.literal("磁盘用量"),
        DashboardMessage::SiteName => text.literal("站点名称"),
        DashboardMessage::Explore => text.literal("探索页面"),
        DashboardMessage::ExploreEnabled => text.literal("开启"),
        DashboardMessage::ExploreDisabled => text.literal("关闭"),
        DashboardMessage::Browsing => text.literal("浏览"),
        DashboardMessage::BrowsingOpen => text.literal("任何人，无论是否登录"),
        DashboardMessage::BrowsingSignedIn => text.literal("仅限已登录用户"),
        DashboardMessage::Registration => text.literal("注册"),
        DashboardMessage::RegistrationOpen => text.literal("向所有人开放"),
        DashboardMessage::RegistrationClosed => text.literal("已关闭（由管理员创建账户）"),
        DashboardMessage::OrgPolicy => text.literal("组织"),
        DashboardMessage::AnyUserCreates => text.literal("任何用户都可以创建"),
        DashboardMessage::AdminsCreate => text.literal("只有管理员可以创建"),
        DashboardMessage::NewRepositories => text.literal("新仓库"),
        DashboardMessage::PublicByDefault => text.literal("默认公开"),
        DashboardMessage::PrivateByDefault => text.literal("默认私有"),
        DashboardMessage::EmailDigests => text.literal("邮件摘要"),
        DashboardMessage::Restarting => text.literal("正在重启服务器"),
        DashboardMessage::RestartRequested => text.literal("已请求重启"),
        DashboardMessage::RestartingText => text.literal(
            "服务器正在完成进行中的 HTTP 请求、停止 SSH 和后台作业，并在重启前关闭存储。长时间运行的传输可能需要一些时间才能完成。",
        ),
        DashboardMessage::Waiting => text.literal("正在等待服务器恢复…"),
        DashboardMessage::ReturnToDashboard => text.literal("返回仪表板"),
        DashboardMessage::WaitNoScript => text.literal("请稍候，然后返回仪表板重新连接。"),
        DashboardMessage::RestartQuestion => text.literal("要重启此 rbit 应用吗？"),
        DashboardMessage::RestartText => text.literal(
            "应用关闭并使用已保存的配置重新启动期间，站点会短暂不可用。HTTP 请求可以完成；SSH 连接将被关闭。用户会保留已保存的会话。",
        ),
        DashboardMessage::RestartAudited => text.literal("此操作会记录在管理审计日志中。"),
        DashboardMessage::RestartUnavailable => text.literal("无法重启"),
        DashboardMessage::RestartUnsupported => {
            text.literal("此服务器宿主环境不支持重启应用。请使用你的服务管理器重启 rbit。");
        }
        DashboardMessage::BackToDashboard => text.literal("返回仪表板"),
    }
    MonitoringMessage => MessageWriter |text| {
        MonitoringMessage::Requests => text.literal("时间窗口内的请求"),
        MonitoringMessage::ErrorRate => text.literal("服务器错误率"),
        MonitoringMessage::ProcessUptime => text.literal("进程运行时间"),
        MonitoringMessage::Samples => text.literal("5 秒采样"),
        MonitoringMessage::FiveMinutesAgo => text.literal("5 分钟前"),
        MonitoringMessage::LatestSample => text.literal("最新采样"),
        MonitoringMessage::Collecting => text.literal("正在采集第一个样本。图表将在五秒后开始显示。"),
        MonitoringMessage::HttpResponses => text.literal("HTTP 响应"),
        MonitoringMessage::CurrentWindow => text.literal("当前时间窗口"),
        MonitoringMessage::Status => text.literal("状态"),
        MonitoringMessage::RequestsColumn => text.literal("请求数"),
        MonitoringMessage::Share => text.literal("占比"),
        MonitoringMessage::GitActivity => text.literal("Git 活动"),
        MonitoringMessage::Operation => text.literal("操作"),
        MonitoringMessage::Exchanges => text.literal("交互次数"),
        MonitoringMessage::CloneFetch => text.literal("克隆 / 获取"),
        MonitoringMessage::Push => text.literal("推送"),
        MonitoringMessage::GitNote => text.literal(
            "智能 HTTP 交互，包括失败的交互。不包括 SSH 和引用通告。响应时间以返回响应头为准；流式传输可能仍在继续。",
        ),
        MonitoringMessage::Footnote => text.literal("仅限此进程；重启后历史数据会重置。不包括监控请求本身。"),
        MonitoringMessage::Informational => text.literal("信息"),
        MonitoringMessage::Successful => text.literal("成功"),
        MonitoringMessage::Redirects => text.literal("重定向"),
        MonitoringMessage::ClientErrors => text.literal("客户端错误"),
        MonitoringMessage::ServerErrors => text.literal("服务器错误"),
        MonitoringMessage::RequestTraffic => text.literal("请求流量"),
        MonitoringMessage::ResponseTime => text.literal("响应时间"),
        MonitoringMessage::RequestsPerSecond => text.literal("请求/秒"),
        MonitoringMessage::MillisecondsAverage => text.literal("毫秒（平均）"),
        MonitoringMessage::WaitingFirst => text.literal("正在等待第一个样本"),
    }
    AdminText<'a> => MessageWriter |text| {
        AdminText::ReleaseNotes(version) => {
            text.literal("查看 rbit ");
            text.argument(version);
            text.literal(" 发行说明");
        }
        AdminText::UsersDetail(admins, suspended) => {
            text.argument(admins);
            text.literal(" 位管理员 · ");
            text.argument(suspended);
            text.literal(" 位已停用");
        }
        AdminText::UsersDetailOne(suspended) => {
            text.literal("1 位管理员 · ");
            text.argument(suspended);
            text.literal(" 位已停用");
        }
        AdminText::RepositoriesDetail(public, private) => {
            text.argument(public);
            text.literal(" 个公开 · ");
            text.argument(private);
            text.literal(" 个私有");
        }
        AdminText::Summary(first, last, total) => {
            text.argument(first);
            text.literal("–");
            text.argument(last);
            text.literal("，共 ");
            text.argument(total);
            text.literal(" 个");
        }
        AdminText::DigestsAt(hour) => {
            text.literal("在 ");
            text.argument(hour);
            text.literal(" 发送，除非用户另行选择");
        }
        AdminText::DeleteLabel(name) => {
            text.literal("删除 ");
            text.argument(name);
        }
        AdminText::ActionsFor(name) => {
            text.argument(name);
            text.literal(" 的操作");
        }
        AdminText::PasswordLength(length) => {
            text.literal("至少 ");
            text.number(length);
            text.literal(" 个字符。");
        }
        AdminText::ChartLabel(title, value, unit) => {
            text.literal("最近五分钟的");
            text.argument(title);
            text.literal("。最新值：");
            text.argument(value);
            text.literal(" ");
            text.argument(unit);
            text.literal("。没有请求时，响应时间会出现空缺。");
        }
        AdminText::UpdatedAt(time) => {
            text.literal("更新于 ");
            text.argument(time);
            text.literal(" UTC");
        }
        AdminText::Coverage(samples, limit) => {
            text.number(samples);
            text.literal(" / ");
            text.number(limit);
            text.literal(" 个样本");
        }
        AdminText::InAdministration(page) => {
            text.argument(page);
            text.literal(" · 管理");
        }
        AdminText::SetPasswordFor(name) => {
            text.literal("为 ");
            text.argument(name);
            text.literal(" 设置密码 · 管理");
        }
        AdminText::DeletePageTitle(name) => {
            text.literal("删除 ");
            text.argument(name);
            text.literal(" · 管理");
        }
    }
}
