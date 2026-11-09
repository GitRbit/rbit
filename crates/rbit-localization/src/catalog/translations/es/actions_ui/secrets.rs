//! Spanish (`es`) entries for `catalog/actions_ui/secrets.rs`.

use crate::catalog::actions_ui::secrets::ActionsSecretsMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    ActionsSecretsMessage<'a> => MessageWriter |text| {
        ActionsSecretsMessage::SecretsTitle => text.literal("Secretos"),
        ActionsSecretsMessage::SecretsIntro { expression } => {
            text.literal("Los flujos de trabajo los leen como ");
            text.argument(expression);
            text.literal(
                ". Los valores están cifrados y no se pueden volver a mostrar, solo reemplazar. Las ejecuciones de \
                 bifurcaciones y de autores sin acceso de escritura nunca los reciben; cualquiera que pueda enviar \
                 puede leerlos mediante un flujo de trabajo.",
            );
        }
        ActionsSecretsMessage::SecretsUnavailable { problem } => {
            text.literal("No se pueden guardar ni usar secretos: ");
            text.argument(problem);
        }
        ActionsSecretsMessage::NoSecrets => text.literal("No hay secretos"),
        ActionsSecretsMessage::NoSecretsHint => {
            text.literal("Añade un secreto para los tokens y las claves que necesitan tus flujos de trabajo.");
        }
        ActionsSecretsMessage::CannotDecrypt => text.literal("No se puede descifrar: vuelve a definirlo"),
        ActionsSecretsMessage::Updated { when } => {
            text.literal("Actualizado ");
            text.argument(when);
        }
        ActionsSecretsMessage::AllRepositories => text.literal("Todos los repositorios"),
        ActionsSecretsMessage::SelectedRepositories { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositorio seleccionado",
                _ => " repositorios seleccionados",
            });
        }
        ActionsSecretsMessage::DeleteSecret { name } => {
            text.literal("Eliminar el secreto ");
            text.argument(name);
        }
        ActionsSecretsMessage::Delete => text.literal("Eliminar"),
        ActionsSecretsMessage::ReplaceValue => text.literal("Reemplazar el valor"),
        ActionsSecretsMessage::NewValueOf { name } => {
            text.literal("Nuevo valor de ");
            text.argument(name);
        }
        ActionsSecretsMessage::ReplaceSecret => text.literal("Reemplazar secreto"),
        ActionsSecretsMessage::ChooseRepositories => text.literal("Elegir repositorios"),
        ActionsSecretsMessage::Recipients { name } => {
            text.literal("Los repositorios cuyos flujos de trabajo reciben ");
            text.argument(name);
            text.literal(", sin cambiar su valor.");
        }
        ActionsSecretsMessage::SaveRepositories => text.literal("Guardar repositorios"),
        ActionsSecretsMessage::NewSecret => text.literal("Nuevo secreto"),
        ActionsSecretsMessage::Name => text.literal("Nombre"),
        ActionsSecretsMessage::SecretNameHint => text.literal(
            "Letras, dígitos y guiones bajos, sin empezar por un dígito ni por GITHUB_, GITEA_, FORGEJO_ o RBIT_. \
             Los nombres se guardan en mayúsculas; un nombre existente se reemplaza.",
        ),
        ActionsSecretsMessage::Value => text.literal("Valor"),
        ActionsSecretsMessage::SecretValueHint { kib } => {
            text.literal("Como máximo ");
            text.number(kib);
            text.literal(" KiB. No se puede volver a mostrar.");
        }
        ActionsSecretsMessage::AddSecret => text.literal("Añadir secreto"),
        ActionsSecretsMessage::VariablesTitle => text.literal("Variables"),
        ActionsSecretsMessage::VariablesIntro { expression } => {
            text.literal("Los flujos de trabajo las leen como ");
            text.argument(expression);
            text.literal(
                ". No son secretas: cualquiera que pueda cambiar un flujo de trabajo puede mostrarlas. Las \
                 ejecuciones de bifurcaciones y de autores sin acceso de escritura nunca las reciben.",
            );
        }
        ActionsSecretsMessage::NoVariables => text.literal("No hay variables"),
        ActionsSecretsMessage::NoVariablesHint => text.literal(
            "Añade una variable para la configuración que comparten tus flujos de trabajo, como una región o una \
             versión.",
        ),
        ActionsSecretsMessage::DeleteVariable { name } => {
            text.literal("Eliminar la variable ");
            text.argument(name);
        }
        ActionsSecretsMessage::Edit => text.literal("Editar"),
        ActionsSecretsMessage::ValueOf { name } => {
            text.literal("Valor de ");
            text.argument(name);
        }
        ActionsSecretsMessage::SaveVariable => text.literal("Guardar variable"),
        ActionsSecretsMessage::NewVariable => text.literal("Nueva variable"),
        ActionsSecretsMessage::VariableNameHint => {
            text.literal("Las mismas reglas que los nombres de secretos; un nombre existente se reemplaza.");
        }
        ActionsSecretsMessage::VariableValueHint { kib } => {
            text.literal("Como máximo ");
            text.number(kib);
            text.literal(" KiB.");
        }
        ActionsSecretsMessage::AddVariable => text.literal("Añadir variable"),
    }
}
