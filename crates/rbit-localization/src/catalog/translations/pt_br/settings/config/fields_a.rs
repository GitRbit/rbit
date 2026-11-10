//! Brazilian Portuguese (`pt-BR`) labels and help of the configuration fields: updates,
//! server, storage, security and repositories.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

config_entries! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "updates", key: "check_for_updates" } => {
            text.literal("Verificar atualizações de software");
        }
        ConfigText::Help { section: "updates", key: "check_for_updates" } => text.literal(
            "Consulta o feed público de releases no GitHub no máximo uma vez por dia após um sucesso. Nenhuma versão \
             em execução nem identidade da instância é enviada. Exige reinicialização.",
        ),
        ConfigText::Label { section: "updates", key: "report_version" } => text.literal("Informar a versão em execução"),
        ConfigText::Help { section: "updates", key: "report_version" } => text.literal(
            "Expõe a versão do software no rodapé público, na API de versão, no banner SSH e nos User-Agents das \
             integrações de saída. Desative para ocultá-la; os administradores mantêm os diagnósticos locais. Exige \
             reinicialização.",
        ),
        ConfigText::Label { section: "server", key: "bind" } => text.literal("Endereço de escuta"),
        ConfigText::Help { section: "server", key: "bind" } => text.literal("Endereço ao qual o servidor HTTP se vincula."),
        ConfigText::Label { section: "server", key: "public_url" } => text.literal("URL pública"),
        ConfigText::Help { section: "server", key: "public_url" } => {
            text.literal("URL base pública, usada nas URLs de clone e na verificação de origem CSRF.");
        }
        ConfigText::Label { section: "server", key: "site_name" } => text.literal("Nome padrão do site"),
        ConfigText::Help { section: "server", key: "site_name" } => text.literal(
            "Nome padrão antes das substituições de marca em vigor. Altere o nome ativo em Marca e políticas.",
        ),
        ConfigText::Label { section: "server", key: "explore_enabled" } => text.literal("Oferecer a página Explorar por padrão"),
        ConfigText::Help { section: "server", key: "explore_enabled" } => text.literal("Padrão da página Explorar, que lista todos os repositórios públicos. A configuração em vigor em Marca e políticas tem precedência."),
        ConfigText::Label { section: "server", key: "require_sign_in" } => text.literal("Exigir entrada para navegar por padrão"),
        ConfigText::Help { section: "server", key: "require_sign_in" } => text.literal("Padrão de quem pode navegar. Quando ativado, visitantes são enviados para a página de entrada e Git, LFS e a API recusam leituras anônimas. A configuração em vigor em Marca e políticas tem precedência."),
        ConfigText::Label { section: "server", key: "open_registration" } => {
            text.literal("Cadastro aberto por padrão");
        }
        ConfigText::Help { section: "server", key: "open_registration" } => text.literal(
            "Política de cadastro padrão. A política em vigor em Marca e políticas tem precedência.",
        ),
        ConfigText::Label { section: "storage", key: "data_dir" } => text.literal("Diretório de dados"),
        ConfigText::Help { section: "storage", key: "data_dir" } => text.literal(
            "Somente leitura aqui. Pare o servidor, mova os dados e atualize o arquivo de implantação ou o ambiente.",
        ),
        ConfigText::Label { section: "storage", key: "secret_key_file" } => text.literal("Arquivo da chave secreta"),
        ConfigText::Help { section: "storage", key: "secret_key_file" } => text.literal(
            "Somente leitura aqui. A chave que criptografa as credenciais armazenadas; vazio significa secret.key no \
             diretório de dados. Faça backup dela junto com o banco de dados.",
        ),
        ConfigText::Label { section: "security", key: "session_lifetime_days" } => {
            text.literal("Duração da sessão (dias)");
        }
        ConfigText::Help { section: "security", key: "session_lifetime_days" } => {
            text.literal("Por quanto tempo uma sessão continua válida sem atividade, em dias.");
        }
        ConfigText::Label { section: "security", key: "session_max_age_days" } => {
            text.literal("Idade máxima da sessão (dias)");
        }
        ConfigText::Help { section: "security", key: "session_max_age_days" } => text.literal(
            "Por quanto tempo, no máximo, uma sessão continua válida, por mais ativa que seja, em dias. 0 significa \
             sem limite.",
        ),
        ConfigText::Label { section: "security", key: "min_password_length" } => {
            text.literal("Tamanho mínimo da senha");
        }
        ConfigText::Help { section: "security", key: "min_password_length" } => {
            text.literal("Tamanho mínimo de senha aceito no cadastro.");
        }
        ConfigText::Label { section: "security", key: "max_password_jobs" } => {
            text.literal("Máximo de jobs de senha");
        }
        ConfigText::Help { section: "security", key: "max_password_jobs" } => text.literal(
            "Jobs simultâneos de hash ou verificação no orçamento compartilhado de processamento de senhas (1–64; \
             padrão 2). Exige reinicialização.",
        ),
        ConfigText::Label { section: "security", key: "trusted_proxies" } => text.literal("Redes de proxies confiáveis"),
        ConfigText::Help { section: "security", key: "trusted_proxies" } => text.literal(
            "Proxies reversos (redes CIDR, por exemplo 127.0.0.1/32, 10.0.0.0/8, ::1/128) cujos cabeçalhos \
             X-Forwarded-For, Forwarded e X-Forwarded-Proto são considerados confiáveis. Vazio (o padrão) significa \
             que o par TCP é sempre o cliente.",
        ),
        ConfigText::Label { section: "security", key: "client_ip_header" } => text.literal("Cabeçalho do endereço do cliente"),
        ConfigText::Help { section: "security", key: "client_ip_header" } => text.literal("Um cabeçalho em que um proxy confiável informa o endereço do cliente, como CF-Connecting-IP atrás do Cloudflare ou do Cloudflare Tunnel. Aceito apenas das redes de proxy confiáveis acima, antes de X-Forwarded-For. Vazio usa apenas X-Forwarded-For."),
        ConfigText::Label { section: "security", key: "rate_limit" } => text.literal("Ativar limitação de requisições"),
        ConfigText::Help { section: "security", key: "rate_limit" } => text.literal(
            "Limita por cliente as entradas, cadastros, códigos de dois fatores, credenciais Git e envios de \
             formulários. Só vale a pena desativar para testes de carga.",
        ),
        ConfigText::Label { section: "security", key: "hsts" } => text.literal("Enviar HSTS"),
        ConfigText::Help { section: "security", key: "hsts" } => text.literal(
            "Envia Strict-Transport-Security (um ano) quando a URL pública é https://, para que os navegadores recusem \
             HTTP simples para este host. Desative apenas ao migrar para fora do HTTPS.",
        ),
        ConfigText::Label { section: "security", key: "hsts_include_subdomains" } => {
            text.literal("HSTS inclui subdomínios");
        }
        ConfigText::Help { section: "security", key: "hsts_include_subdomains" } => text.literal(
            "Adiciona includeSubDomains a esse cabeçalho, vinculando também todos os subdomínios do host ao HTTPS.",
        ),
        ConfigText::Label { section: "security", key: "external_images" } => text.literal("Permitir imagens externas"),
        ConfigText::Help { section: "security", key: "external_images" } => text.literal(
            "Permite que as páginas mostrem imagens de outros sites, o que também informa a esses sites quem viu a \
             página. Desativado, limita as imagens a esta instância e a imagens data: embutidas.",
        ),
        ConfigText::Label { section: "repository", key: "default_branch" } => text.literal("Branch padrão"),
        ConfigText::Help { section: "repository", key: "default_branch" } => {
            text.literal("Branch para o qual o HEAD aponta nos repositórios recém-criados.");
        }
        ConfigText::Label { section: "repository", key: "git_binary" } => text.literal("Executável do Git"),
        ConfigText::Help { section: "repository", key: "git_binary" } => {
            text.literal("Caminho do executável git usado pelo transporte smart HTTP.");
        }
        ConfigText::Label { section: "repository", key: "max_render_bytes" } => {
            text.literal("Renderização máxima (bytes)");
        }
        ConfigText::Help { section: "repository", key: "max_render_bytes" } => {
            text.literal("Arquivos maiores que isso não são renderizados diretamente no navegador.");
        }
        ConfigText::Label { section: "repository", key: "max_git_processes" } => {
            text.literal("Máximo de processos Git");
        }
        ConfigText::Help { section: "repository", key: "max_git_processes" } => text.literal(
            "Processos de transporte Git (clones, fetches e pushes por HTTP e SSH) em execução ao mesmo tempo; \
             requisições adicionais são recusadas (503 por HTTP).",
        ),
        ConfigText::Label { section: "repository", key: "max_git_requests_per_ip" } => {
            text.literal("Máximo de requisições Git por IP");
        }
        ConfigText::Help { section: "repository", key: "max_git_requests_per_ip" } => text.literal(
            "Requisições Git smart HTTP simultâneas de um endereço IP (429 além disso). Atrás de um proxy reverso, \
             isso exige redes de proxies confiáveis.",
        ),
        ConfigText::Label { section: "repository", key: "git_idle_timeout_seconds" } => {
            text.literal("Tempo limite de inatividade do Git (segundos)");
        }
        ConfigText::Help { section: "repository", key: "git_idle_timeout_seconds" } => {
            text.literal("Segundos que uma troca Git smart HTTP pode ficar sem tráfego em qualquer direção.");
        }
        ConfigText::Label { section: "repository", key: "git_max_seconds" } => {
            text.literal("Duração máxima do Git (segundos)");
        }
        ConfigText::Help { section: "repository", key: "git_max_seconds" } => text.literal(
            "Limite superior, em segundos, de uma única troca Git smart HTTP (um clone ou push muito grande).",
        ),
        ConfigText::Label { section: "repository", key: "max_push_bytes" } => text.literal("Push máximo (bytes)"),
        ConfigText::Help { section: "repository", key: "max_push_bytes" } => {
            text.literal("Maior pack que um push pode enviar, por HTTP e SSH. 0 significa ilimitado.");
        }
        ConfigText::Label { section: "repository", key: "max_archives_per_ip" } => {
            text.literal("Máximo de arquivos compactados por IP");
        }
        ConfigText::Help { section: "repository", key: "max_archives_per_ip" } => text.literal(
            "Downloads de arquivos compactados do código-fonte que um endereço IP pode ter em andamento ao mesmo tempo.",
        ),
        ConfigText::Label { section: "repository", key: "archive_idle_timeout_seconds" } => {
            text.literal("Tempo limite de inatividade do arquivo compactado (segundos)");
        }
        ConfigText::Help { section: "repository", key: "archive_idle_timeout_seconds" } => text.literal(
            "Segundos que o download de um arquivo compactado pode ficar parado (o cliente sem ler ou o git sem \
             escrever).",
        ),
        ConfigText::Label { section: "repository", key: "archive_max_seconds" } => {
            text.literal("Duração máxima do arquivo compactado (segundos)");
        }
        ConfigText::Help { section: "repository", key: "archive_max_seconds" } => {
            text.literal("Limite superior, em segundos, do download de um arquivo compactado.");
        }
    }
}
