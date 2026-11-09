//! Brazilian Portuguese (`pt-BR`) labels and help of the configuration fields: email
//! delivery, SSH, sign-in providers and branding defaults.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

/// The help shared by the deployment settings that apply after a restart.
const RESTART: &str = "Configuração de implantação aplicada após reiniciar o servidor.";
/// The help shared by the branding defaults.
const SHOWN: &str = "Exibido aos visitantes desta instância.";

config_entries! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "mail", key: "transport" } => text.literal("Transporte"),
        ConfigText::Help { section: "mail", key: "transport" } => text.literal(RESTART),
        ConfigText::Label { section: "mail", key: "from_address" } => text.literal("Endereço de e-mail do remetente"),
        ConfigText::Help { section: "mail", key: "from_address" } => text.literal("O endereço From: de toda mensagem."),
        ConfigText::Label { section: "mail", key: "from_name" } => text.literal("Nome de exibição do remetente"),
        ConfigText::Help { section: "mail", key: "from_name" } => {
            text.literal("O nome de exibição em From:. Em branco significa o nome do site.");
        }
        ConfigText::Label { section: "mail", key: "smtp_host" } => text.literal("Servidor SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_host" } => {
            text.literal("Nome do host do relay SMTP (transport = \"smtp\").");
        }
        ConfigText::Label { section: "mail", key: "smtp_port" } => text.literal("Porta SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_port" } => text.literal(
            "Deixe em branco para a porta 587 com STARTTLS ou sem criptografia, ou para a porta 465 com TLS implícito.",
        ),
        ConfigText::Label { section: "mail", key: "smtp_security" } => text.literal("Criptografia SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_security" } => text.literal(RESTART),
        ConfigText::Label { section: "mail", key: "smtp_username" } => text.literal("Nome de usuário SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_username" } => {
            text.literal("Credenciais do relay. Em branco significa sem autenticação.");
        }
        ConfigText::Label { section: "mail", key: "smtp_password" } => text.literal("Senha SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_password" } => text.literal(RESTART),
        ConfigText::Label { section: "mail", key: "timeout_seconds" } => text.literal("Tempo limite (segundos)"),
        ConfigText::Help { section: "mail", key: "timeout_seconds" } => {
            text.literal("Desiste de uma mensagem que não foi entregue após esta quantidade de segundos.");
        }
        ConfigText::Label { section: "ssh", key: "enabled" } => text.literal("Ativar este recurso"),
        ConfigText::Help { section: "ssh", key: "enabled" } => text.literal("Executa o servidor SSH embutido."),
        ConfigText::Label { section: "ssh", key: "bind" } => text.literal("Endereço de escuta"),
        ConfigText::Help { section: "ssh", key: "bind" } => text.literal("Endereço ao qual o servidor SSH se vincula."),
        ConfigText::Label { section: "ssh", key: "public_host" } => text.literal("Host público"),
        ConfigText::Help { section: "ssh", key: "public_host" } => text.literal(
            "Nome do host exibido nas URLs de clone SSH. Por padrão, o host de server.public_url.",
        ),
        ConfigText::Label { section: "ssh", key: "public_port" } => text.literal("Porta pública"),
        ConfigText::Help { section: "ssh", key: "public_port" } => text.literal(
            "Porta exibida nas URLs de clone SSH, quando um proxy ou mapeamento de portas difere do endereço de escuta.",
        ),
        ConfigText::Label { section: "ssh", key: "host_key_path" } => text.literal("Caminho da chave do host"),
        ConfigText::Help { section: "ssh", key: "host_key_path" } => text.literal(
            "Caminho do arquivo da chave de host SSH. Em branco usa ssh/host_ed25519 dentro do diretório de dados; é \
             gerada na primeira inicialização se não existir.",
        ),
        ConfigText::Label { section: "ssh", key: "max_connections" } => text.literal("Máximo de conexões"),
        ConfigText::Help { section: "ssh", key: "max_connections" } => text.literal(
            "Conexões simultâneas de todos os clientes; conexões adicionais são fechadas imediatamente.",
        ),
        ConfigText::Label { section: "ssh", key: "max_connections_per_ip" } => text.literal("Máximo de conexões por IP"),
        ConfigText::Help { section: "ssh", key: "max_connections_per_ip" } => {
            text.literal("Conexões simultâneas de um endereço IP.");
        }
        ConfigText::Label { section: "ssh", key: "auth_timeout_seconds" } => {
            text.literal("Tempo limite de autenticação (segundos)");
        }
        ConfigText::Help { section: "ssh", key: "auth_timeout_seconds" } => {
            text.literal("Segundos que um cliente tem para se autenticar após conectar.");
        }
        ConfigText::Label { section: "ssh", key: "idle_timeout_seconds" } => {
            text.literal("Tempo limite de inatividade (segundos)");
        }
        ConfigText::Help { section: "ssh", key: "idle_timeout_seconds" } => {
            text.literal("Segundos sem nenhum tráfego antes de uma conexão ser fechada.");
        }
        ConfigText::Label { section: "ssh", key: "max_session_seconds" } => text.literal("Sessão máxima (segundos)"),
        ConfigText::Help { section: "ssh", key: "max_session_seconds" } => text.literal(
            "Limite superior, em segundos, de uma única operação Git (um clone ou push muito grande).",
        ),
        ConfigText::Label { section: "auth", key: "timeout_seconds" } => text.literal("Tempo limite (segundos)"),
        ConfigText::Help { section: "auth", key: "timeout_seconds" } => {
            text.literal("Limite superior de uma requisição a um provedor.");
        }
        ConfigText::Label { section: "auth", key: "allow_private_addresses" } => {
            text.literal("Permitir destinos em redes privadas");
        }
        ConfigText::Help { section: "auth", key: "allow_private_addresses" } => text.literal(
            "Permite provedores cujos endereços são de loopback, privados, link-local ou unique-local, e emissores ou \
             URLs base por http simples.",
        ),
        ConfigText::Label { section: "branding", key: "primary_color" } => text.literal("Cor principal"),
        ConfigText::Help { section: "branding", key: "primary_color" } => text.literal(
            "Fundo do botão principal no formato #RRGGBB. Defina as duas cores ou deixe ambas em branco.",
        ),
        ConfigText::Label { section: "branding", key: "primary_text_color" } => text.literal("Cor do texto principal"),
        ConfigText::Help { section: "branding", key: "primary_text_color" } => {
            text.literal("Texto do botão principal no formato #RRGGBB. Escolha um contraste legível.");
        }
        ConfigText::Label { section: "branding", key: "description" } => text.literal("Descrição"),
        ConfigText::Help { section: "branding", key: "description" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "welcome_title" } => text.literal("Título de boas-vindas"),
        ConfigText::Help { section: "branding", key: "welcome_title" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "welcome_message" } => text.literal("Mensagem de boas-vindas"),
        ConfigText::Help { section: "branding", key: "welcome_message" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "logo_url" } => text.literal("URL do logotipo"),
        ConfigText::Help { section: "branding", key: "logo_url" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "favicon_url" } => text.literal("URL do favicon"),
        ConfigText::Help { section: "branding", key: "favicon_url" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "app_icon_url" } => text.literal("URL do ícone do aplicativo"),
        ConfigText::Help { section: "branding", key: "app_icon_url" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "footer_text" } => text.literal("Texto do rodapé"),
        ConfigText::Help { section: "branding", key: "footer_text" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "support_url" } => text.literal("URL de suporte"),
        ConfigText::Help { section: "branding", key: "support_url" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "terms_url" } => text.literal("URL dos termos"),
        ConfigText::Help { section: "branding", key: "terms_url" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "privacy_url" } => text.literal("URL de privacidade"),
        ConfigText::Help { section: "branding", key: "privacy_url" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "show_powered_by" } => text.literal("Mostrar “Powered by”"),
        ConfigText::Help { section: "branding", key: "show_powered_by" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "show_version" } => text.literal("Mostrar a versão"),
        ConfigText::Help { section: "branding", key: "show_version" } => text.literal(SHOWN),
        ConfigText::Label { section: "branding", key: "show_features" } => text.literal("Mostrar os recursos"),
        ConfigText::Help { section: "branding", key: "show_features" } => text.literal(SHOWN),
    }
}
