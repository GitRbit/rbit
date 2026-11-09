//! Simplified Chinese (`zh-Hans`) entries for `catalog/actions_ui/secrets.rs`.

use crate::catalog::actions_ui::secrets::ActionsSecretsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    ActionsSecretsMessage<'a> => MessageWriter |text| {
        ActionsSecretsMessage::SecretsTitle => text.literal("机密"),
        ActionsSecretsMessage::SecretsIntro { expression } => {
            text.literal("工作流通过 ");
            text.argument(expression);
            text.literal(
                " 读取机密。机密值经过加密，无法再次显示，只能替换。来自复刻或没有写入权限的作者的运行永远不会获得机密；\
                 任何能推送的人都可以通过工作流读取它们。",
            );
        }
        ActionsSecretsMessage::SecretsUnavailable { problem } => {
            text.literal("无法存储或使用机密：");
            text.argument(problem);
        }
        ActionsSecretsMessage::NoSecrets => text.literal("没有机密"),
        ActionsSecretsMessage::NoSecretsHint => text.literal("为工作流需要的令牌和密钥添加机密。"),
        ActionsSecretsMessage::CannotDecrypt => text.literal("无法解密——请重新设置"),
        ActionsSecretsMessage::Updated { when } => {
            text.literal("更新于 ");
            text.argument(when);
        }
        ActionsSecretsMessage::AllRepositories => text.literal("所有仓库"),
        ActionsSecretsMessage::SelectedRepositories { count } => {
            text.literal("已选 ");
            text.number(count);
            text.literal(" 个仓库");
        }
        ActionsSecretsMessage::DeleteSecret { name } => {
            text.literal("删除机密 ");
            text.argument(name);
        }
        ActionsSecretsMessage::Delete => text.literal("删除"),
        ActionsSecretsMessage::ReplaceValue => text.literal("替换值"),
        ActionsSecretsMessage::NewValueOf { name } => {
            text.argument(name);
            text.literal(" 的新值");
        }
        ActionsSecretsMessage::ReplaceSecret => text.literal("替换机密"),
        ActionsSecretsMessage::ChooseRepositories => text.literal("选择仓库"),
        ActionsSecretsMessage::Recipients { name } => {
            text.literal("其工作流可获得 ");
            text.argument(name);
            text.literal(" 的仓库，不会更改它的值。");
        }
        ActionsSecretsMessage::SaveRepositories => text.literal("保存仓库"),
        ActionsSecretsMessage::NewSecret => text.literal("新建机密"),
        ActionsSecretsMessage::Name => text.literal("名称"),
        ActionsSecretsMessage::SecretNameHint => text.literal(
            "字母、数字和下划线，不能以数字或 GITHUB_、GITEA_、FORGEJO_、RBIT_ 开头。名称以大写形式存储；同名的现有机密会被替换。",
        ),
        ActionsSecretsMessage::Value => text.literal("值"),
        ActionsSecretsMessage::SecretValueHint { kib } => {
            text.literal("最多 ");
            text.number(kib);
            text.literal(" KiB。保存后无法再次显示。");
        }
        ActionsSecretsMessage::AddSecret => text.literal("添加机密"),
        ActionsSecretsMessage::VariablesTitle => text.literal("变量"),
        ActionsSecretsMessage::VariablesIntro { expression } => {
            text.literal("工作流通过 ");
            text.argument(expression);
            text.literal(
                " 读取变量。变量不是机密：任何能修改工作流的人都可以将其打印出来。来自复刻或没有写入权限的作者的运行永远不会获得变量。",
            );
        }
        ActionsSecretsMessage::NoVariables => text.literal("没有变量"),
        ActionsSecretsMessage::NoVariablesHint => text.literal("为工作流共享的配置（例如区域或版本）添加变量。"),
        ActionsSecretsMessage::DeleteVariable { name } => {
            text.literal("删除变量 ");
            text.argument(name);
        }
        ActionsSecretsMessage::Edit => text.literal("编辑"),
        ActionsSecretsMessage::ValueOf { name } => {
            text.argument(name);
            text.literal(" 的值");
        }
        ActionsSecretsMessage::SaveVariable => text.literal("保存变量"),
        ActionsSecretsMessage::NewVariable => text.literal("新建变量"),
        ActionsSecretsMessage::VariableNameHint => text.literal("规则与机密名称相同；同名的现有变量会被替换。"),
        ActionsSecretsMessage::VariableValueHint { kib } => {
            text.literal("最多 ");
            text.number(kib);
            text.literal(" KiB。");
        }
        ActionsSecretsMessage::AddVariable => text.literal("添加变量"),
    }
}
