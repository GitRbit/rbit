//! Korean (`ko`) entries for `catalog/actions_ui/secrets.rs`.

use crate::catalog::actions_ui::secrets::ActionsSecretsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ActionsSecretsMessage<'a> => MessageWriter |text| {
        ActionsSecretsMessage::SecretsTitle => text.literal("시크릿"),
        ActionsSecretsMessage::SecretsIntro { expression, .. } => {
            text.literal("워크플로에서는 ");
            text.argument(expression);
            text.literal(" 형식으로 읽습니다. 값은 암호화되며 다시 표시할 수 없고 교체만 할 수 있습니다. 포크에서 온 실행이나 쓰기 권한이 없는 작성자의 실행에는 전달되지 않으며, 푸시할 수 있는 사람은 누구나 워크플로를 통해 읽을 수 있습니다.");
        }
        ActionsSecretsMessage::SecretsUnavailable { problem, .. } => {
            text.literal("시크릿을 저장하거나 사용할 수 없습니다: ");
            text.argument(problem);
        }
        ActionsSecretsMessage::NoSecrets => text.literal("시크릿 없음"),
        ActionsSecretsMessage::NoSecretsHint => text.literal("워크플로에 필요한 토큰과 키를 시크릿으로 추가하세요."),
        ActionsSecretsMessage::CannotDecrypt => text.literal("복호화할 수 없음 — 다시 설정하세요"),
        ActionsSecretsMessage::Updated { when, .. } => {
            text.literal("업데이트: ");
            text.argument(when);
        }
        ActionsSecretsMessage::AllRepositories => text.literal("모든 저장소"),
        ActionsSecretsMessage::SelectedRepositories { count, .. } => {
            text.literal("선택한 저장소 ");
            text.number(count);
            text.literal("개");
        }
        ActionsSecretsMessage::DeleteSecret { name, .. } => {
            text.literal("시크릿 ");
            text.argument(name);
            text.literal(" 삭제");
        }
        ActionsSecretsMessage::Delete => text.literal("삭제"),
        ActionsSecretsMessage::ReplaceValue => text.literal("값 교체"),
        ActionsSecretsMessage::NewValueOf { name, .. } => {
            text.argument(name);
            text.literal("의 새 값");
        }
        ActionsSecretsMessage::ReplaceSecret => text.literal("시크릿 교체"),
        ActionsSecretsMessage::ChooseRepositories => text.literal("저장소 선택"),
        ActionsSecretsMessage::Recipients { name, .. } => {
            text.literal("값을 바꾸지 않고 ");
            text.argument(name);
            text.literal("을(를) 받을 워크플로의 저장소를 선택합니다.");
        }
        ActionsSecretsMessage::SaveRepositories => text.literal("저장소 저장"),
        ActionsSecretsMessage::NewSecret => text.literal("새 시크릿"),
        ActionsSecretsMessage::Name => text.literal("이름"),
        ActionsSecretsMessage::SecretNameHint => text.literal("영문자, 숫자, 밑줄만 사용할 수 있으며 숫자나 GITHUB_, GITEA_, FORGEJO_, RBIT_로 시작할 수 없습니다. 이름은 대문자로 저장되며, 같은 이름이 있으면 교체됩니다."),
        ActionsSecretsMessage::Value => text.literal("값"),
        ActionsSecretsMessage::SecretValueHint { kib, .. } => {
            text.literal("최대 ");
            text.number(kib);
            text.literal("KiB. 다시 표시할 수 없습니다.");
        }
        ActionsSecretsMessage::AddSecret => text.literal("시크릿 추가"),
        ActionsSecretsMessage::VariablesTitle => text.literal("변수"),
        ActionsSecretsMessage::VariablesIntro { expression, .. } => {
            text.literal("워크플로에서는 ");
            text.argument(expression);
            text.literal(" 형식으로 읽습니다. 변수는 비밀이 아닙니다. 워크플로를 변경할 수 있는 사람은 누구나 출력할 수 있습니다. 포크에서 온 실행이나 쓰기 권한이 없는 작성자의 실행에는 전달되지 않습니다.");
        }
        ActionsSecretsMessage::NoVariables => text.literal("변수 없음"),
        ActionsSecretsMessage::NoVariablesHint => text.literal("리전이나 버전처럼 워크플로가 공유하는 구성을 변수로 추가하세요."),
        ActionsSecretsMessage::DeleteVariable { name, .. } => {
            text.literal("변수 ");
            text.argument(name);
            text.literal(" 삭제");
        }
        ActionsSecretsMessage::Edit => text.literal("편집"),
        ActionsSecretsMessage::ValueOf { name, .. } => {
            text.argument(name);
            text.literal("의 값");
        }
        ActionsSecretsMessage::SaveVariable => text.literal("변수 저장"),
        ActionsSecretsMessage::NewVariable => text.literal("새 변수"),
        ActionsSecretsMessage::VariableNameHint => text.literal("시크릿 이름과 같은 규칙이 적용되며, 같은 이름이 있으면 교체됩니다."),
        ActionsSecretsMessage::VariableValueHint { kib, .. } => {
            text.literal("최대 ");
            text.number(kib);
            text.literal("KiB.");
        }
        ActionsSecretsMessage::AddVariable => text.literal("변수 추가"),
    }
}
