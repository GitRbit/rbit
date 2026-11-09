//! Simplified Chinese (`zh-Hans`) entries for `catalog/chrome.rs`.

use crate::catalog::chrome::{ChromeMessage, ChromeSentence};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    ChromeMessage<'a> => MessageWriter |text| {
        ChromeMessage::Skip => text.literal("跳到内容"),
        ChromeMessage::Main => text.literal("主导航"),
        ChromeMessage::Explore => text.literal("探索"),
        ChromeMessage::Language => text.literal("语言"),
        ChromeMessage::SwitchTheme => text.literal("在浅色和深色之间切换"),
        ChromeMessage::ChooseTheme => text.literal("选择主题"),
        ChromeMessage::Notifications => text.literal("通知"),
        ChromeMessage::Unread => text.literal("你有未读通知"),
        ChromeMessage::NewRepository => text.literal("新建仓库"),
        ChromeMessage::AccountMenu => text.literal("账户菜单"),
        ChromeMessage::Issues => text.literal("议题"),
        ChromeMessage::Pulls => text.literal("拉取请求"),
        ChromeMessage::Dashboard => text.literal("仪表板"),
        ChromeMessage::Profile => text.literal("个人资料"),
        ChromeMessage::NewOrganization => text.literal("新建组织"),
        ChromeMessage::Settings => text.literal("设置"),
        ChromeMessage::Administration => text.literal("站点管理"),
        ChromeMessage::SignOut => text.literal("退出登录"),
        ChromeMessage::SignIn => text.literal("登录"),
        ChromeMessage::SignUp => text.literal("注册"),
        ChromeMessage::Menu => text.literal("菜单"),
        ChromeMessage::CreateAccount => text.literal("创建账户"),
        ChromeMessage::Themes => text.literal("主题"),
        ChromeMessage::LightMode => text.literal("浅色模式"),
        ChromeMessage::DarkMode => text.literal("深色模式"),
        ChromeMessage::UpdateAvailable => text.literal("有可用更新"),
        ChromeMessage::TimingHelp => text.literal("上一个页面或更新的服务器耗时"),
        ChromeMessage::RenderedIn => text.literal("渲染耗时"),
        ChromeMessage::Footer => text.literal("页脚"),
        ChromeMessage::Support => text.literal("帮助"),
        ChromeMessage::Terms => text.literal("条款"),
        ChromeMessage::Privacy => text.literal("隐私"),
        ChromeMessage::Status => text.literal("状态"),
        ChromeMessage::Home { site } => {
            text.argument(site);
            text.literal(" 首页");
        }
    }
    ChromeSentence => RichWriter |text| {
        ChromeSentence::PoweredBy => {
            text.literal("由 ");
            text.slot(0);
            text.literal(" 驱动");
        }
    }
}
