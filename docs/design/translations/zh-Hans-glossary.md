# Simplified Chinese (zh-Hans) glossary

The terms the `zh_hans` translation tables use, one per concept, so every page says the
same thing the same way. The wording follows the Chinese interfaces of mainstream
developer tools (GitHub, GitLab). When a new message needs a concept that is not listed,
add it here in the same change. See "Translating a locale" in
[localization-contracts.md](../localization-contracts.md) for the table format.

## Typography

- Full-width punctuation inside Chinese text: `，。：；？！（）`, and `“…”` for quotes.
  A sentence written entirely in Chinese ends with `。`; short labels, buttons and
  headings have no final punctuation, as in English.
- One space between Han characters and Latin letters, digits, arguments or slots
  (`3 个仓库`, `推送到 main`, `SSH 密钥`). No space next to full-width punctuation or
  quotes (`删除“docs”？`), and none between a number and `%`.
- The ellipsis stays the single character `…`.
- Product names, protocols and machine values stay as they are: rbit, Git, GitHub, SSH,
  HTTPS, SHA-256, Git LFS, Markdown, README, YAML, JSON, OAuth, TOTP, WebAuthn, URL, API,
  ZIP, `git push` and other commands, file names, keys and route names.
- Counts never pluralise in Chinese; use a measure word where one reads naturally
  (`个`, `条`, `次`, `位`, `项`).

## Code and repositories

| English | 中文 |
|---|---|
| repository | 仓库 |
| branch | 分支 |
| default branch | 默认分支 |
| base branch | 基础分支 |
| head branch | 头部分支 |
| commit (noun, verb) | 提交 |
| tag (Git) | 标记 |
| annotated / lightweight tag | 附注标记 / 轻量标记 |
| release | 发行版 |
| pre-release | 预发行版 |
| draft | 草稿 |
| fork (noun, verb) | 复刻 |
| forked from | 复刻自 |
| mirror | 镜像 |
| upstream | 上游 |
| star / unstar | 加星标 / 取消星标 |
| starred | 已加星标 |
| stargazers | 星标用户 |
| watch | 关注 |
| clone | 克隆 |
| push / force push | 推送 / 强制推送 |
| pull (Git) | 拉取 |
| file / directory / path | 文件 / 目录 / 路径 |
| diff | 差异 |
| compare | 比较 |
| blame | 追溯 |
| raw | 原始 |
| history | 历史记录 |
| contributors / contributions | 贡献者 / 贡献 |
| activity | 动态 |
| archive / unarchive | 归档 / 取消归档 |
| archived | 已归档 |
| read-only | 只读 |
| visibility | 可见性 |
| public / private / internal | 公开 / 私有 / 内部 |
| transfer | 转移 |
| topic | 主题 |
| language | 语言 |
| signed / verified | 已签名 / 已验证 |
| deploy key | 部署密钥 |
| storage | 存储 |

## Collaboration

| English | 中文 |
|---|---|
| issue | 议题 |
| pull request | 拉取请求 |
| open (state) | 开放 |
| closed | 已关闭 |
| reopen | 重新打开 |
| completed / not planned | 已完成 / 不计划处理 |
| merge / merged | 合并 / 已合并 |
| merge commit | 合并提交 |
| squash and merge | 压缩并合并 |
| rebase and merge | 变基并合并 |
| conflict | 冲突 |
| mergeable | 可合并 |
| label | 标签 |
| milestone | 里程碑 |
| assignee / assign | 受理人 / 分配 |
| reviewer | 审查者 |
| review (noun, verb) | 审查 |
| request a review | 请求审查 |
| approve | 批准 |
| request changes | 请求更改 |
| dismiss (a review) | 驳回 |
| comment | 评论 |
| reply | 回复 |
| conversation | 对话 |
| resolve / resolved | 解决 / 已解决 |
| outdated | 已过时 |
| suggestion | 建议 |
| mention | 提及 |
| reaction | 回应 |
| lock / unlock (conversation) | 锁定 / 解锁 |
| subscribe / unsubscribe | 订阅 / 取消订阅 |
| notification | 通知 |
| inbox | 收件箱 |
| digest | 摘要 |
| check / status check | 检查 / 状态检查 |
| timeline | 时间线 |
| thread (an issue or pull request one is notified about) | 讨论 |
| ref | 引用 |
| author | 作者 |
| participant | 参与者 |

## Actions

| English | 中文 |
|---|---|
| Actions (the product area) | Actions |
| workflow | 工作流 |
| run (noun) / re-run | 运行 / 重新运行 |
| job | 作业 |
| step | 步骤 |
| runner | 运行器 |
| artifact | 制品 |
| cache | 缓存 |
| secret | 机密 |
| variable | 变量 |
| environment | 环境 |
| deployment | 部署 |
| concurrency | 并发 |
| matrix | 矩阵 |
| log | 日志 |
| trigger | 触发 |
| queued / in progress | 排队中 / 进行中 |
| succeeded / failed / cancelled / skipped | 成功 / 失败 / 已取消 / 已跳过 |
| waiting / awaiting approval | 等待中 / 等待批准 |
| actor (who triggered a run) | 执行者 |
| triggered by | 触发者 |
| duration | 耗时 |
| raw log | 原始日志 |
| summary (job) | 概要 |
| manual / schedule / dispatch (event) | 手动 / 定时 / 调度 |

## Accounts, security and administration

| English | 中文 |
|---|---|
| account | 账户 |
| profile | 个人资料 |
| settings | 设置 |
| appearance / theme | 外观 / 主题 |
| light / dark (mode) | 浅色 / 深色 |
| display language / preferred language | 显示语言 / 首选语言 |
| sign in / sign out / sign up | 登录 / 退出登录 / 注册 |
| username / password | 用户名 / 密码 |
| email address / email (message) | 邮箱地址 / 邮件 |
| verify / verified | 验证 / 已验证 |
| two-factor authentication | 双重身份验证 |
| authenticator app | 身份验证器应用 |
| recovery code | 恢复码 |
| passkey | 通行密钥 |
| security key | 安全密钥 |
| SSH key | SSH 密钥 |
| access token / personal access token | 访问令牌 / 个人访问令牌 |
| scope (token) | 权限范围 |
| session | 会话 |
| webhook | Webhook |
| webhook secret, client secret | 密钥 |
| delivery (webhook) | 投递 |
| sign-in / identity provider | 登录提供方 / 身份提供方 |
| linked account / unlink | 已关联的账户 / 取消关联 |
| registration token (runner) | 注册令牌 |
| revoke | 吊销 |
| payload | 负载 |
| organization | 组织 |
| team | 团队 |
| member | 成员 |
| owner | 所有者 |
| collaborator | 协作者 |
| permission / role | 权限 / 角色 |
| read / triage / write / maintain / admin | 读取 / 分类 / 写入 / 维护 / 管理 |
| administrator | 管理员 |
| branch protection / protected branch | 分支保护 / 受保护分支 |
| rule | 规则 |
| bypass | 绕过 |
| instance / server | 实例 / 服务器 |
| site administration | 站点管理 |
| dashboard | 仪表板 |
| monitoring | 监控 |
| audit log | 审计日志 |
| suspend | 停用 |
| configuration | 配置 |
| deployment default / override | 部署默认值 / 覆盖项 |
| insights cache | 洞察缓存 |
| danger zone | 危险区域 |
| explore | 探索 |
| wiki | Wiki |
| page (wiki) | 页面 |
| revision | 修订 |

## Common actions

| English | 中文 |
|---|---|
| save / cancel | 保存 / 取消 |
| create / new | 创建 / 新建 |
| edit / update | 编辑 / 更新 |
| delete / remove | 删除 / 移除 |
| add | 添加 |
| search / filter / sort | 搜索 / 筛选 / 排序 |
| copy / copied | 复制 / 已复制 |
| download / upload | 下载 / 上传 |
| enable / disable | 启用 / 禁用 |
| show / hide | 显示 / 隐藏 |
| preview | 预览 |
| learn more | 了解详情 |
