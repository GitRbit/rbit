//! Russian (`ru`) entries for `catalog/repo/nav.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::nav::RepoNavSentence;
use crate::catalog::repo::nav::{RepoNavLabel, RepoNavMessage};
use crate::plural::Plural;

entries! { Ru:
    RepoNavLabel => MessageWriter |text| {
        RepoNavLabel::ArchivedNote => text.literal("Этот репозиторий архивирован. Он доступен только для чтения: его можно клонировать, форкнуть и отметить звездой, но нельзя изменить."),
        RepoNavLabel::Private => text.literal("Приватный"),
        RepoNavLabel::Public => text.literal("Публичный"),
        RepoNavLabel::Repository => text.literal("Репозиторий"),
        RepoNavLabel::Code => text.literal("Код"),
        RepoNavLabel::Issues => text.literal("Задачи"),
        RepoNavLabel::PullRequests => text.literal("Запросы на слияние"),
        RepoNavLabel::Commits => text.literal("Коммиты"),
        RepoNavLabel::Branches => text.literal("Ветки"),
        RepoNavLabel::Tags => text.literal("Теги"),
        RepoNavLabel::Releases => text.literal("Релизы"),
        RepoNavLabel::Activity => text.literal("Активность"),
        RepoNavLabel::Actions => text.literal("Actions"),
        RepoNavLabel::Settings => text.literal("Настройки"),
        RepoNavLabel::ForkThisRepository => text.literal("Создать форк репозитория"),
        RepoNavLabel::Fork => text.literal("Форк"),
        RepoNavLabel::Forks => text.literal("Форки"),
        RepoNavLabel::Clone => text.literal("Клонировать"),
        RepoNavLabel::CloneUrl => text.literal("URL для клонирования"),
        RepoNavLabel::CopyCloneUrl => text.literal("Скопировать URL для клонирования"),
        RepoNavLabel::SshCloneUrl => text.literal("SSH-URL для клонирования"),
        RepoNavLabel::CopySshCloneUrl => text.literal("Скопировать SSH-URL для клонирования"),
        RepoNavLabel::Copy => text.literal("Скопировать"),
        RepoNavLabel::YourSettings => text.literal("ваших настройках"),
        RepoNavLabel::DownloadZip => text.literal("Скачать ZIP"),
        RepoNavLabel::Star => text.literal("Звезда"),
        RepoNavLabel::Starred => text.literal("Со звездой"),
        RepoNavLabel::StarThisRepository => text.literal("Поставить звезду репозиторию"),
        RepoNavLabel::UnstarThisRepository => text.literal("Убрать звезду с репозитория"),
        RepoNavLabel::SignInToStar => text.literal("Войдите, чтобы поставить звезду репозиторию"),
        RepoNavLabel::Stargazers => text.literal("Поставившие звезду"),
    }
    RepoNavMessage => MessageWriter |text| {
        RepoNavMessage::ForkCount { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "форк",
                Plural::Few => "форка",
                _ => "форков",
            });
        }
        RepoNavMessage::StargazerCount { count } => {
            text.number(count.unsigned_abs());
            text.literal(" ");
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => "звезда",
                Plural::Few => "звезды",
                _ => "звёзд",
            });
        }
    }
    RepoNavSentence => RichWriter |text| {
        RepoNavSentence::ForkedFrom => {
            text.literal("форк репозитория ");
            text.slot(0);
        }
        RepoNavSentence::MirrorOf => {
            text.literal("зеркало репозитория ");
            text.slot(0);
        }
        RepoNavSentence::SshKeyNote => {
            text.literal("Для SSH нужен ключ в ");
            text.slot(0);
            text.literal(".");
        }
    }
}
