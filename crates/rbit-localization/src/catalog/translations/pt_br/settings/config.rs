//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/config.rs`.
//!
//! The field labels and help live in the sibling modules `config/fields_*.rs`, one part
//! of the schema each, so that every file stays short.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText, ConfigText};

/// Declares `write`, the field labels and help of one part of the schema, from arms
/// written like the table below so the coverage report counts them.
macro_rules! config_entries {
    (ConfigText<$lifetime:lifetime> => MessageWriter |$text:ident| { $($arms:tt)* }) => {
        /// Writes a field's label or help, or returns `false` when this part lacks it.
        #[allow(clippy::too_many_lines, reason = "one arm per field label and help")]
        pub(super) fn write(message: ConfigText<'_>, $text: &mut MessageWriter) -> bool {
            match message {
                $($arms)*
                _ => return false,
            }
            true
        }
    };
}

mod fields_a;
mod fields_b;
mod fields_c;
mod fields_d;

entries! { PtBr:
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::SectionTitle("updates") => text.literal("Atualizações de software e privacidade"),
        ConfigText::SectionTitle("server") => text.literal("Servidor"),
        ConfigText::SectionTitle("storage") => text.literal("Armazenamento"),
        ConfigText::SectionTitle("security") => text.literal("Segurança"),
        ConfigText::SectionTitle("repository") => text.literal("Repositórios"),
        ConfigText::SectionTitle("mail") => text.literal("Envio de e-mails"),
        ConfigText::SectionTitle("ssh") => text.literal("SSH"),
        ConfigText::SectionTitle("auth") => text.literal("Provedores de entrada"),
        ConfigText::SectionTitle("branding") => text.literal("Padrões de marca"),
        ConfigText::SectionTitle("notifications") => text.literal("Notificações por e-mail"),
        ConfigText::SectionTitle("operations") => text.literal("Operações"),
        ConfigText::SectionTitle("api") => text.literal("API REST"),
        ConfigText::SectionTitle("webhooks") => text.literal("Webhooks"),
        ConfigText::SectionTitle("releases") => text.literal("Releases"),
        ConfigText::SectionTitle("mirrors") => text.literal("Espelhos de pull"),
        ConfigText::SectionTitle("push_mirrors") => text.literal("Espelhos de push"),
        ConfigText::SectionTitle("imports") => text.literal("Importações"),
        ConfigText::SectionTitle("issues") => text.literal("Issues e caixa de entrada"),
        ConfigText::SectionTitle("pulls") => text.literal("Pull requests"),
        ConfigText::SectionTitle("editor") => text.literal("Editor no navegador"),
        ConfigText::SectionTitle("lfs") => text.literal("Git LFS"),
        ConfigText::SectionTitle("actions") => text.literal("Actions"),
        ConfigText::SectionHelp("updates") => text.literal(
            "Verificações independentes de releases em segundo plano e controles de divulgação da versão em execução. As \
             alterações valem após reiniciar.",
        ),
        ConfigText::SectionHelp("server") => text.literal("Endereço público, escuta e padrões da instância."),
        ConfigText::SectionHelp("storage") => text.literal(
            "Veja o diretório de dados. Para mover o armazenamento, faça isso offline pela configuração de implantação.",
        ),
        ConfigText::SectionHelp("security") => {
            text.literal("Sessões, senhas, limitação de requisições e proxies reversos confiáveis.");
        }
        ConfigText::SectionHelp("repository") => text.literal("Padrões e limites dos repositórios Git."),
        ConfigText::SectionHelp("mail") => {
            text.literal("Configure a identidade do remetente, a conexão SMTP e os tempos limite de envio.");
        }
        ConfigText::SectionHelp("ssh") => {
            text.literal("Servidor SSH do Git embutido, endereço público de clone e limites de conexão.");
        }
        ConfigText::SectionHelp("auth") => text.literal("Conecte provedores de identidade OIDC e GitHub."),
        ConfigText::SectionHelp("branding") => text.literal(
            "Padrões de implantação. Use Marca e políticas para alterações imediatas em toda a instância.",
        ),
        ConfigText::SectionHelp("notifications") => {
            text.literal("Ative os e-mails de notificação e escolha a política de inscrição padrão.");
        }
        ConfigText::SectionHelp("operations") => {
            text.literal("Métricas, manutenção, jobs em segundo plano e caches.");
        }
        ConfigText::SectionHelp("api") => text.literal("Disponibilidade da API e limites de paginação."),
        ConfigText::SectionHelp("webhooks") => {
            text.literal("Configurações de envio, novas tentativas e retenção.");
        }
        ConfigText::SectionHelp("releases") => {
            text.literal("Tamanhos e quantidades de arquivos de releases e tempos limite de envio.");
        }
        ConfigText::SectionHelp("mirrors") => {
            text.literal("Agendamento das buscas, tempos limite e acesso à rede.");
        }
        ConfigText::SectionHelp("push_mirrors") => {
            text.literal("Agendamento dos pushes, tempos limite e acesso à rede.");
        }
        ConfigText::SectionHelp("imports") => {
            text.literal("Importação de repositórios, concorrência e correspondência de autores.");
        }
        ConfigText::SectionHelp("issues") => {
            text.literal("Varredura de commits e retenção das notificações no aplicativo.");
        }
        ConfigText::SectionHelp("pulls") => {
            text.literal("Limites de mesclagem, verificações em segundo plano e endereços de autores.");
        }
        ConfigText::SectionHelp("editor") => {
            text.literal("Limites de tamanho, quantidade e tempo dos envios pelo navegador.");
        }
        ConfigText::SectionHelp("lfs") => {
            text.literal("Armazenamento de arquivos grandes, tamanhos de objetos e validade dos tokens.");
        }
        ConfigText::SectionHelp("actions") => {
            text.literal("CI/CD em executores externos, retenção, limites e tempos limite.");
        }
        ConfigText::Choice { section: "mail", key: "transport", value: "disabled" } => text.literal("Desativado"),
        ConfigText::Choice { section: "mail", key: "transport", value: "log" } => text.literal("Log de desenvolvimento"),
        ConfigText::Choice { section: "mail", key: "transport", value: "smtp" } => text.literal("Servidor SMTP"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "starttls" } => {
            text.literal("STARTTLS (recomendado)");
        }
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "tls" } => text.literal("TLS implícito"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "none" } => text.literal("Sem criptografia"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "off" } => {
            text.literal("Desativado");
        }
        ConfigText::Choice { section: "notifications", key: "default_email", value: "participating" } => {
            text.literal("Conversas de que o usuário participa");
        }
        ConfigText::Choice { section: "notifications", key: "default_email", value: "all" } => {
            text.literal("Todas as conversas inscritas");
        }
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "none" } => {
            text.literal("Manter os autores originais como texto");
        }
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email" } => {
            text.literal("Corresponder pelo e-mail verificado");
        }
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email_or_username" } => {
            text.literal("Corresponder pelo e-mail verificado ou pelo nome de usuário");
        }
        ConfigText::Choice { section: "provider", key: "kind", value: "oidc" } => text.literal("OpenID Connect"),
        ConfigText::Choice { section: "provider", key: "kind", value: "github" } => {
            text.literal("GitHub / GitHub Enterprise");
        }
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "true" } => {
            text.literal("Seguir a política de cadastro da instância");
        }
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "false" } => {
            text.literal("Somente contas existentes");
        }
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "always" } => {
            text.literal("Permitir contas mesmo com o cadastro fechado");
        }
        field => {
            return fields_a::write(field, text)
                || fields_b::write(field, text)
                || fields_c::write(field, text)
                || fields_d::write(field, text);
        }
    }
    ConfigMessage => MessageWriter |text| {
        ConfigMessage::Title => text.literal("Configuração"),
        ConfigMessage::Intro => {
            text.literal("Gerencie sua instância auto-hospedada com configurações organizadas por recurso.");
        }
        ConfigMessage::RestartServer => text.literal("Reiniciar servidor"),
        ConfigMessage::Saved => {
            text.literal("Configurações salvas. Reinicie o servidor para aplicar as alterações de implantação.");
        }
        ConfigMessage::RestartRequired => text.literal("Reinicialização necessária"),
        ConfigMessage::EnvironmentWins => text.literal("As variáveis de ambiente têm precedência."),
        ConfigMessage::LivePolicies => text.literal("Marca e políticas — aplicadas imediatamente"),
        ConfigMessage::SettingsSection => text.literal("Seção de configurações"),
        ConfigMessage::OpenSection => text.literal("Abrir seção"),
        ConfigMessage::Sections => text.literal("Seções de configuração"),
        ConfigMessage::LiveOverrides => {
            text.literal(
                "As políticas de marca e de cadastro salvas e em vigor substituem estes padrões de implantação.",
            );
        }
        ConfigMessage::IdentityProviders => text.literal("Provedores de identidade"),
        ConfigMessage::ProvidersHint => {
            text.literal(
                "Primeiro adicione e remova os cartões de provedores; depois digite as novas credenciais e salve. As \
                 alterações de provedores ficam neste formulário até você salvar a seção.",
            );
        }
        ConfigMessage::CallbackLead => {
            text.literal("Registre uma URL de callback em cada provedor: a sua URL pública seguida de");
        }
        ConfigMessage::CallbackTail => {
            text.literal(
                ", substituindo PROVIDER_ID pelo ID abaixo. Para OpenID Connect, defina a URL do emissor; para GitHub \
                 Enterprise, defina a URL do Enterprise.",
            );
        }
        ConfigMessage::NoProviders => {
            text.literal("Nenhum provedor de entrada externo configurado. A entrada local está disponível.");
        }
        ConfigMessage::RemoveProvider => text.literal("Remover provedor"),
        ConfigMessage::AddProvider => text.literal("Adicionar provedor de identidade"),
        ConfigMessage::NewProvider => text.literal("Novo provedor de entrada"),
        ConfigMessage::OnlySection => text.literal("Somente esta seção será atualizada."),
        ConfigMessage::Recovery => text.literal("Recuperação e padrões de implantação"),
        ConfigMessage::RecoveryLead => text.literal("Se as configurações salvas impedirem a inicialização, defina"),
        ConfigMessage::RecoveryTail => {
            text.literal("no ambiente do servidor, reinicie e depois corrija ou remova as substituições aqui.");
        }
        ConfigMessage::RemovingOverrides => {
            text.literal(
                "Remover as substituições restaura a configuração do arquivo e do ambiente na próxima \
                 reinicialização. A marca e as políticas em vigor são mantidas.",
            );
        }
        ConfigMessage::RemoveOverrides => text.literal("Remover todas as substituições de implantação"),
        ConfigMessage::Optional => text.literal("(opcional)"),
        ConfigMessage::OnePerLine => text.literal("Uma entrada por linha."),
        ConfigMessage::ProviderDefaults => text.literal("Usar os padrões do provedor"),
        ConfigMessage::Configured => text.literal("Configurado — deixe em branco para manter"),
        ConfigMessage::NotConfigured => text.literal("Não configurado"),
        ConfigMessage::SecretHint => {
            text.literal(
                "Deixe em branco para manter a credencial atual. Novas credenciais nunca são exibidas novamente após \
                 um erro; digite-as de novo ao corrigir o formulário. Proteja o banco de dados da instância e os \
                 backups dele.",
            );
        }
        ConfigMessage::ClearSecret => text.literal("Apagar esta credencial"),
        ConfigMessage::BlankDefault => text.literal("Deixe em branco para usar o padrão."),
    }
    ConfigPageText<'a> => MessageWriter |text| {
        ConfigPageText::SaveSection(section) => {
            text.literal("Salvar as configurações de ");
            text.argument(section);
        }
    }
}
