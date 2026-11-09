# Russian (ru) glossary

One term per concept across the Russian tables in
`crates/rbit-localization/src/catalog/translations/ru/`. Keep new entries consistent with
this list; change a term here and in every table at once.

## Style

- Address the reader with the formal «вы», lowercase.
- Buttons and menu items use the infinitive: «Сохранить», «Удалить», «Создать задачу».
  Headings of forms use a noun: «Редактирование релиза», «Удаление: {path}».
- Quotation marks «…», the em dash with spaces ( — ), the ellipsis as one character (…).
- Count nouns follow `Plural::One`, `Few` and `Many`: 1 комментарий, 2 комментария,
  5 комментариев, 21 комментарий. A label that stands without its noun uses a colon
  instead: «Открыто: 12», «Выполнено: 40%».
- An inserted name stays in the nominative. Put it after a colon («Ветка: main»,
  «История: {title}») or after a noun that carries the case («пользователем {author}»).
- A sentence that starts with a person names them after «Пользователь» so the past-tense
  verb agrees with that noun whatever the person's gender: «Пользователь {actor} закрыл…».
- Fragments split around markup must read as one sentence and never start with a comma or
  stop when the template puts a space before them; rephrase instead.
- Never translate product names (rbit, Git, GitHub, Gitea, Git LFS, SSH, OpenID Connect),
  commands, keys, paths, header names or example values (`production`, `main`).

## Terms

| English | Russian |
| --- | --- |
| repository | репозиторий |
| branch / default branch | ветка / ветка по умолчанию |
| base branch / head branch | базовая ветка / исходная ветка |
| tag | тег |
| ref | ссылка |
| commit (noun) | коммит |
| commit (verb, button) | зафиксировать (изменения) |
| push (noun / verb) | отправка / отправить (изменения) |
| force push | принудительная отправка |
| fetch (mirror) | получение / получать |
| clone | клонировать |
| fork | форк / создать форк |
| star / stargazers | звезда / поставившие звезду |
| issue | задача |
| pull request | запрос на слияние |
| merge / merged | слияние, слить / слит(о) |
| squash | объединение коммитов |
| rebase | перебазирование, перебазировать |
| diff | различия |
| blame | авторство |
| review / reviewer | ревью / рецензент |
| approve / approval | одобрить / одобрение |
| request changes | запросить изменения |
| dismiss (a review) | отклонить |
| conversation / thread | обсуждение |
| resolve a conversation | отметить обсуждение решённым |
| label | метка |
| milestone | этап |
| assignee | исполнитель |
| collaborator | соавтор |
| member / owner | участник / владелец |
| organization / team | организация / команда |
| permission | права |
| draft | черновик |
| release / pre-release | релиз / предварительный релиз |
| release file (asset) | файл |
| mirror / push mirror | зеркало / push-зеркало |
| upstream | источник |
| remote | удалённый репозиторий |
| wiki / revision | вики / ревизия |
| webhook / delivery | вебхук / доставка |
| deploy key | ключ развёртывания |
| SSH key | SSH-ключ |
| access token | токен доступа |
| registration token | токен регистрации |
| passkey / security key | ключ доступа / ключ безопасности |
| two-factor authentication | двухфакторная аутентификация |
| recovery code | код восстановления |
| authenticator app | приложение-аутентификатор |
| sign in / sign out / sign up | войти / выйти / зарегистрироваться |
| account | учётная запись |
| session | сеанс |
| email | электронная почта |
| notification / inbox / digest | уведомление / входящие / сводка |
| subscribe / unsubscribe | подписаться / отписаться |
| settings / appearance / theme | настройки / оформление / тема |
| dashboard | панель управления |
| explore | обзор |
| instance | сервер |
| administrator | администратор |
| audit log | журнал аудита |
| suspend | приостановить |
| Actions (the feature) | Actions |
| workflow | рабочий процесс |
| run (noun) | запуск |
| job | задание |
| step | шаг |
| task (runner work unit), background job | задача (only in the configuration schema and administration; elsewhere «задача» is an issue) |
| runner / instance runner | раннер / раннер сервера |
| artifact | артефакт |
| secret / variable | секрет / переменная |
| environment | окружение |
| deploy / deployment | развернуть / развёртывание |
| check / status check | проверка / проверка статуса |
| branch protection / rule | защита веток / правило |
| danger zone | опасная зона |
| archive / unarchive | архивировать / разархивировать |
| transfer ownership | передача владения |
| identity provider | провайдер удостоверений |
| timeout | тайм-аут |
| log (job output) / raw log | журнал / полный журнал |
| re-run | перезапустить |
| actor (run filter) / triggered by | пользователь / инициатор |
| dispatch / schedule (workflow event) | по вызову / по расписанию |
| queued / in progress / skipped (run status) | в очереди / выполняется / пропущено |
| footer | подвал |
| colorblind (themes) / theme family | для дальтоников / семейство тем |
| display language / preferred language | язык интерфейса / предпочитаемый язык |

## Terms worth revisiting

- «ревью» for review: common in Russian developer speech; «рецензия» is the formal
  alternative and is already used for the reviewer («рецензент»).
- «слит» for merged: GitLab's Russian term, but «принят» may read better to some users.
- «push-зеркало» for push mirror, beside «зеркало» for a pull mirror.
- «сервер» for instance, chosen over «экземпляр» because the pages address people who run
  one server.
