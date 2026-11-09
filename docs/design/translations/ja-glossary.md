# Japanese (ja) glossary

One term per concept for the Japanese catalog in
`crates/rbit-localization/src/catalog/translations/ja/`. The wording follows the
conventions of mainstream Japanese developer tools (GitHub and GitLab).

## Style

- Sentences addressed to the user are polite (です／ます). Buttons, labels, headings and
  table columns are concise noun phrases or verb stems: 「保存」「ブランチを削除」.
- Full-width punctuation: 、。：？！（）. Quotation marks are 「」 (『』 inside 「」).
  No space between Japanese text and full-width punctuation. The ellipsis stays `…`.
- A half-width space separates Japanese text from Latin words, digits and interpolated
  values (`text.argument`, `text.number`, slots): 「SSH キー」「3 件のコミット」
  「main を削除」. No space next to full-width punctuation or brackets.
- Counts use counters where natural: 件 (issues, comments, results, items), 個 (files,
  keys, objects), 人 (people), 日／時間／分／秒 (durations). Japanese has only
  `Plural::Other`, so a count has one form.
- Machine values, code, commands, file names, keys, environment variables and product
  names (rbit, Git, GitHub, Git LFS, SSH, OpenID Connect, SHA-256) are never translated.

## Terms

| English | Japanese | Note |
|---|---|---|
| repository | リポジトリ | |
| branch / default branch | ブランチ／デフォルトブランチ | |
| tag | タグ | |
| commit (noun / verb) | コミット／コミットする | |
| push / pull / fetch / clone | プッシュ／プル／フェッチ／クローン | |
| pull request | プルリクエスト | |
| issue | Issue | As in GitHub's Japanese UI |
| merge / merged | マージ／マージ済み | |
| squash and merge / rebase and merge | スカッシュしてマージ／リベースしてマージ | |
| conflict | コンフリクト | |
| base branch / head branch | ベースブランチ／ヘッドブランチ | |
| fork | フォーク | |
| star / watch | スター／ウォッチ | |
| release / asset | リリース／アセット | |
| milestone | マイルストーン | |
| label | ラベル | |
| assignee | 担当者 | |
| review / reviewer | レビュー／レビュアー | |
| approve / request changes | 承認／変更をリクエスト | |
| comment / reply | コメント／返信 | |
| conversation / thread | 会話／スレッド | |
| timeline | タイムライン | |
| open / closed (state) | オープン／クローズ | Verbs: クローズする、再オープンする |
| draft | ドラフト | |
| lock / unlock (conversation) | ロック／ロック解除 | |
| mention | メンション | |
| notification / inbox | 通知／受信トレイ | |
| subscribe / unsubscribe | 購読／購読解除 | |
| file / directory / folder | ファイル／ディレクトリ／フォルダー | |
| diff / compare | 差分／比較 | |
| history | 履歴 | |
| blame | Blame | |
| contributor / contributions | コントリビューター／コントリビューション | |
| archive (repository) | アーカイブ | |
| pull mirror / push mirror | プルミラー／プッシュミラー | |
| import | インポート | |
| visibility | 公開範囲 | |
| public / private / internal | パブリック／プライベート／内部 | |
| wiki / page / revision | Wiki／ページ／版 | |
| Actions (the feature) | Actions | Product feature name, kept |
| workflow / run / job / step | ワークフロー／実行／ジョブ／ステップ | |
| runner / label (runner) | ランナー／ラベル | |
| artifact / log | アーティファクト／ログ | |
| secret / variable | シークレット／変数 | |
| environment | 環境 | |
| check / status check / required check | チェック／ステータスチェック／必須チェック | |
| branch protection / protected branch / rule | ブランチ保護／保護ブランチ／ルール | |
| webhook / delivery | Webhook／配信 | |
| deploy key | デプロイキー | |
| SSH key / public key | SSH キー／公開鍵 | |
| access token / scope | アクセストークン／スコープ | |
| organization / team / member | 組織／チーム／メンバー | |
| collaborator | コラボレーター | |
| owner / administrator | オーナー／管理者 | |
| permission / role | 権限／ロール | |
| user / username / display name | ユーザー／ユーザー名／表示名 | |
| account / profile | アカウント／プロフィール | |
| sign in / sign out / sign up | サインイン／サインアウト／アカウント登録 | |
| password | パスワード | |
| two-factor authentication | 2 要素認証 | |
| authenticator app | 認証アプリ | |
| passkey / security key | パスキー／セキュリティキー | |
| recovery codes | リカバリーコード | |
| session | セッション | |
| email address | メールアドレス | "email" alone: メール |
| verify / verified | 確認／確認済み | |
| audit log | 監査ログ | |
| instance / server | インスタンス／サーバー | |
| configuration / settings | 構成／設定 | |
| dashboard | ダッシュボード | |
| appearance / theme | 外観／テーマ | |
| identity provider / sign-in provider | ID プロバイダー／サインインプロバイダー | |
| rate limiting | レート制限 | |
| restart required | 再起動が必要 | |
| default (value) | デフォルト | デフォルト値、デフォルトの… |
| author | 作成者 | |
| description | 説明 | |
| due date | 期日 | |
| resolve conversation | 会話を解決 | |
| dismiss review | レビューを却下 | |
| outdated | 古い | |
| pending | 保留中 | |
| not planned | 計画なし | |
| force push | フォースプッシュ | |
| merge method / merge commit / merge base | マージ方法／マージコミット／マージベース | |
| review request / ready for review | レビューリクエスト／レビュー可能 | |
| files changed | 変更されたファイル | |
| upstream / remote | アップストリーム／リモート | |
| submodule | サブモジュール | |
| pre-release | プレリリース | |
| refs | 参照 | |
| revision (compare page) | リビジョン | 版 is only for wiki revisions |
| stargazers | スターを付けたユーザー | |
| activity | アクティビティ | |
| commit message | コミットメッセージ | |
| danger zone | 危険な操作 | |
| leave (repository, organization) | 脱退 | |
| personal access token | 個人用アクセストークン | |
| registration token (runner) | 登録トークン | |
| setup token | セットアップトークン | |
| revoke / revoked | 取り消す／取り消し済み | |
| deployment / deploy | デプロイ | |
| wait timer | 待機タイマー | |
| trusted author | 信頼済みの作成者 | |
| approve and run / reject | 承認して実行／却下 | |
| online / offline | オンライン／オフライン | |
| ephemeral (runner) | 使い捨て | |
| explore | 探索 | |
| read / unread / mark as read | 既読／未読／既読にする | |
| pagination | ページ送り | |
| browser | ブラウザー | |
| credential | 認証情報 | |
| listen address | 待ち受けアドレス | |
| retention | 保持期間 | |
| timeout | タイムアウト | |
| concurrency | 同時実行数 | |
| issuer (OIDC) | 発行者 | |
| Branding and policies (admin page) | 「ブランディングとポリシー」 | |
| suspend / suspended / unsuspend | 停止／停止中／停止を解除 | |
| registration open / closed | 開放／停止 | |
| transfer (ownership) | 移管 | |
| uptime | 稼働時間 | |
| digest (email) | ダイジェスト | |
| namespace | 名前空間 | |
| bypass | バイパス | |
| payload / redeliver | ペイロード／再配信 | |
| active / inactive (webhook) | 有効／無効 | |
| base permission / read / write | 基本権限／読み取り／書き込み | |
| bio / website / location | 自己紹介／Web サイト／所在地 | |
| link / unlink (account) | リンク／リンク解除 | |
| Blame / Raw | Blame／Raw | Kept, as on GitHub |

Labels that introduce a value end with a full-width colon: 「作成：」「最終使用：」
「有効期限：」. Confirmation prompts read 「確認のため X と入力してください」.

## Common actions

| English | Japanese |
|---|---|
| Save / Save changes | 保存／変更を保存 |
| Cancel | キャンセル |
| Delete / Remove | 削除 |
| Edit | 編集 |
| Create / New … | 作成／新しい… |
| Add | 追加 |
| Update | 更新 |
| Copy | コピー |
| Search / Filter | 検索／絞り込み |
| Back / Next / Previous | 戻る／次へ／前へ |
| Show more / Load more | さらに表示 |
| Optional | 任意 |
| Required | 必須 |
