//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/admin_pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage};

entries! { PtBr:
    AdminPageMessage => MessageWriter |text| {
        AdminPageMessage::UsersIntro => text.literal("Todas as pessoas com conta nesta instância."),
        AdminPageMessage::UsersSearch => text.literal("Pesquisar por nome de usuário, nome ou e-mail"),
        AdminPageMessage::NewUser => text.literal("Novo usuário"),
        AdminPageMessage::NoUsers => text.literal("Nenhum usuário encontrado"),
        AdminPageMessage::NoUsersText => text.literal("Tente outra pesquisa."),
        AdminPageMessage::User => text.literal("Usuário"),
        AdminPageMessage::Joined => text.literal("Entrou em"),
        AdminPageMessage::Admin => text.literal("Administrador"),
        AdminPageMessage::Suspended => text.literal("Suspenso"),
        AdminPageMessage::EmailVerified => text.literal("Endereço de e-mail verificado"),
        AdminPageMessage::Verified => text.literal("Verificado"),
        AdminPageMessage::You => text.literal("Você"),
        AdminPageMessage::RemoveAdmin => text.literal("Remover administrador"),
        AdminPageMessage::MakeAdmin => text.literal("Tornar administrador"),
        AdminPageMessage::LiftSuspension => text.literal("Remover suspensão"),
        AdminPageMessage::Suspend => text.literal("Suspender"),
        AdminPageMessage::SendReset => text.literal("Enviar redefinição de senha"),
        AdminPageMessage::SetPasswordMenu => text.literal("Definir senha…"),
        AdminPageMessage::DeleteMenu => text.literal("Excluir…"),
        AdminPageMessage::ResetSent => text.literal("E-mail de redefinição de senha enviado."),
        AdminPageMessage::PasswordSet => text.literal("Senha atualizada."),
        AdminPageMessage::NewUserIntro => {
            text.literal("Os administradores podem criar contas mesmo com o cadastro fechado.");
        }
        AdminPageMessage::Username => text.literal("Nome de usuário"),
        AdminPageMessage::UsernameHint => text.literal("Letras, dígitos, “-”, “_” e “.”"),
        AdminPageMessage::Email => text.literal("E-mail"),
        AdminPageMessage::InitialPassword => text.literal("Senha inicial"),
        AdminPageMessage::SiteAdministrator => text.literal("Administrador do site"),
        AdminPageMessage::SiteAdministratorText => {
            text.literal("Acesso total a todos os repositórios e a este painel.");
        }
        AdminPageMessage::CreateUser => text.literal("Criar usuário"),
        AdminPageMessage::RegistrationClosed => {
            text.literal("O cadastro público está fechado no momento; esta é a única forma de adicionar usuários.");
        }
        AdminPageMessage::SetPassword => text.literal("Definir senha"),
        AdminPageMessage::ResetHint => {
            text.literal(
                "Para deixar que o usuário escolha a própria senha, envie uma redefinição de senha pela lista de \
                 usuários.",
            );
        }
        AdminPageMessage::DeleteUser => text.literal("Excluir usuário"),
        AdminPageMessage::DeleteUserLead => text.literal("Isso exclui permanentemente"),
        AdminPageMessage::OrganizationsIntro => text.literal("Espaços de nomes compartilhados e os membros deles."),
        AdminPageMessage::SearchByName => text.literal("Pesquisar por nome"),
        AdminPageMessage::NoOrganizations => text.literal("Nenhuma organização encontrada"),
        AdminPageMessage::NoOrganizationsText => {
            text.literal("As organizações serão listadas aqui quando existirem.");
        }
        AdminPageMessage::Organization => text.literal("Organização"),
        AdminPageMessage::Members => text.literal("Membros"),
        AdminPageMessage::Created => text.literal("Criada"),
        AdminPageMessage::RepositoriesIntro => {
            text.literal("Todos os repositórios, públicos e privados, com os mais ativos recentemente primeiro.");
        }
        AdminPageMessage::RepositoriesSearch => text.literal("Pesquisar por proprietário/nome ou descrição"),
        AdminPageMessage::NoRepositories => text.literal("Nenhum repositório encontrado"),
        AdminPageMessage::NoRepositoriesText => {
            text.literal("Os repositórios serão listados aqui quando existirem.");
        }
        AdminPageMessage::Repository => text.literal("Repositório"),
        AdminPageMessage::Updated => text.literal("Atualizado"),
        AdminPageMessage::DeleteRepository => text.literal("Excluir repositório"),
        AdminPageMessage::DeleteRepositoryText => {
            text.literal(
                "Isso exclui permanentemente o repositório, incluindo todos os dados Git dele. Não pode ser desfeito.",
            );
        }
        AdminPageMessage::AuditIntro => {
            text.literal(
                "Entradas, alterações de dois fatores, sessões, tokens de acesso e ações de administradores, das mais \
                 recentes para as mais antigas.",
            );
        }
        AdminPageMessage::AuditSearch => text.literal("Usuário, alvo ou endereço IP"),
        AdminPageMessage::Category => text.literal("Categoria"),
        AdminPageMessage::Filter => text.literal("Filtrar"),
        AdminPageMessage::AllEvents => text.literal("Todos os eventos"),
        AdminPageMessage::Clear => text.literal("Limpar"),
        AdminPageMessage::NoEvents => text.literal("Nenhum evento"),
        AdminPageMessage::NoEventsText => text.literal("Nada corresponde a estes filtros ainda."),
        AdminPageMessage::As => text.literal("como"),
        AdminPageMessage::By => text.literal("por"),
        AdminPageMessage::On => text.literal("em"),
    }
    InstanceSettingsMessage => MessageWriter |text| {
        InstanceSettingsMessage::Intro => text.literal("As alterações valem imediatamente para toda a instância."),
        InstanceSettingsMessage::Saved => text.literal("Configurações salvas."),
        InstanceSettingsMessage::SiteName => text.literal("Nome do site"),
        InstanceSettingsMessage::SiteNameHint => {
            text.literal("Exibido no cabeçalho, nos títulos das páginas e quando instalado como aplicativo.");
        }
        InstanceSettingsMessage::Branding => text.literal("Marca e identidade"),
        InstanceSettingsMessage::BrandingHint => {
            text.literal(
                "Os textos são texto simples. Imagens e links aceitam URLs HTTPS ou /caminhos locais servidos pelo seu \
                 proxy reverso. Campos de imagem em branco usam os padrões incluídos.",
            );
        }
        InstanceSettingsMessage::PrimaryColor => {
            text.literal("Cor do botão principal (em branco usa a do tema)");
        }
        InstanceSettingsMessage::PrimaryTextColor => text.literal("Cor do texto do botão principal"),
        InstanceSettingsMessage::ColorExample => text.literal("#2563eb"),
        InstanceSettingsMessage::TextColorExample => text.literal("#ffffff"),
        InstanceSettingsMessage::ColorHint => {
            text.literal(
                "Escolha cores com contraste suficiente para que os rótulos dos botões fiquem legíveis. Defina as duas \
                 cores ou deixe ambas em branco.",
            );
        }
        InstanceSettingsMessage::Description => text.literal("Descrição do site"),
        InstanceSettingsMessage::WelcomeTitle => text.literal("Título de boas-vindas (em branco usa o nome do site)"),
        InstanceSettingsMessage::WelcomeMessage => text.literal("Mensagem de boas-vindas"),
        InstanceSettingsMessage::LogoUrl => text.literal("URL do logotipo"),
        InstanceSettingsMessage::FaviconUrl => text.literal("URL do favicon"),
        InstanceSettingsMessage::AppIconUrl => {
            text.literal("URL do ícone do aplicativo (quadrado, com pelo menos 512 pixels)");
        }
        InstanceSettingsMessage::FooterText => text.literal("Texto do rodapé"),
        InstanceSettingsMessage::SupportUrl => text.literal("URL de suporte"),
        InstanceSettingsMessage::TermsUrl => text.literal("URL dos termos"),
        InstanceSettingsMessage::PrivacyUrl => text.literal("URL de privacidade"),
        InstanceSettingsMessage::ShowPoweredBy => text.literal("Mostrar “Powered by rbit”"),
        InstanceSettingsMessage::ShowVersion => text.literal("Mostrar a versão do software"),
        InstanceSettingsMessage::ShowFeatures => {
            text.literal("Mostrar os recursos de auto-hospedagem na página de boas-vindas");
        }
        InstanceSettingsMessage::Explore => text.literal("Página Explorar"),
        InstanceSettingsMessage::ExploreEnabled => text.literal("Ativada"),
        InstanceSettingsMessage::ExploreEnabledText => text.literal("Qualquer pessoa que possa navegar no site pode pesquisar e listar todos os repositórios públicos."),
        InstanceSettingsMessage::ExploreDisabled => text.literal("Desativada"),
        InstanceSettingsMessage::ExploreDisabledText => text.literal("A página Explorar não é encontrada e seus links ficam ocultos, inclusive para administradores. Repositórios públicos continuam públicos."),
        InstanceSettingsMessage::Browsing => text.literal("Quem pode navegar"),
        InstanceSettingsMessage::BrowsingOpen => text.literal("Qualquer pessoa"),
        InstanceSettingsMessage::BrowsingOpenText => text.literal("Visitantes podem ler repositórios, perfis e organizações públicos sem uma conta."),
        InstanceSettingsMessage::BrowsingSignedIn => text.literal("Somente usuários conectados"),
        InstanceSettingsMessage::BrowsingSignedInText => text.literal("Visitantes são enviados para a página de entrada. Git, LFS e a API recusam leituras sem credenciais."),
        InstanceSettingsMessage::Registration => text.literal("Cadastro"),
        InstanceSettingsMessage::Open => text.literal("Aberto"),
        InstanceSettingsMessage::OpenText => {
            text.literal("Qualquer pessoa pode criar uma conta na página de cadastro.");
        }
        InstanceSettingsMessage::Closed => text.literal("Fechado"),
        InstanceSettingsMessage::ClosedText => text.literal("Somente administradores podem criar contas."),
        InstanceSettingsMessage::OrgCreation => text.literal("Quem pode criar organizações"),
        InstanceSettingsMessage::AnyUser => text.literal("Qualquer usuário"),
        InstanceSettingsMessage::AnyUserText => {
            text.literal("Todo usuário conectado pode criar organizações.");
        }
        InstanceSettingsMessage::AdminsOnly => text.literal("Somente administradores"),
        InstanceSettingsMessage::AdminsOnlyText => {
            text.literal("Os usuários ainda podem ser adicionados a organizações existentes.");
        }
        InstanceSettingsMessage::DefaultVisibility => text.literal("Visibilidade padrão dos repositórios"),
        InstanceSettingsMessage::Public => text.literal("Público"),
        InstanceSettingsMessage::PublicText => {
            text.literal("Os novos repositórios ficam visíveis para todos, a menos que isso seja alterado.");
        }
        InstanceSettingsMessage::Private => text.literal("Privado"),
        InstanceSettingsMessage::PrivateText => {
            text.literal(
                "Os novos repositórios começam privados; os proprietários ainda podem torná-los públicos.",
            );
        }
        InstanceSettingsMessage::VisibilityHint => text.literal("Pré-selecionada no formulário de novo repositório."),
        InstanceSettingsMessage::DigestTime => text.literal("Horário do resumo por e-mail"),
        InstanceSettingsMessage::DigestHint => {
            text.literal(
                "Quando os resumos diários e semanais de notificações são enviados aos usuários que não escolheram um \
                 horário. Os resumos semanais saem às segundas-feiras.",
            );
        }
        InstanceSettingsMessage::Save => text.literal("Salvar configurações"),
    }
    AdminPageText<'a> => MessageWriter |text| {
        AdminPageText::ShareSecurely(length) => {
            text.literal("Pelo menos ");
            text.number(length);
            text.literal(
                " caracteres. Compartilhe-a com o usuário de forma segura. Ele precisará trocá-la ao entrar pela \
                 primeira vez.",
            );
        }
        AdminPageText::ReplacesPassword(name) => {
            text.literal(
                "A nova senha substitui a atual imediatamente e todos os links de redefinição de senha deixam de \
                 funcionar. Compartilhe-a com ",
            );
            text.argument(name);
            text.literal(" de forma segura.");
        }
        AdminPageText::DeleteUserTail(repositories) => {
            text.literal(", os repositórios do usuário (");
            text.argument(repositories);
            text.literal(
                ", incluindo todos os dados Git), os tokens de acesso e as participações em organizações. Isso não \
                 pode ser desfeito.",
            );
        }
    }
}
