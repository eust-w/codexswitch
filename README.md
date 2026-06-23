# codexswitch

Local provider switching and proxy routing for Claude Code, Codex CLI, Codex
Desktop, and Gemini.

This v1 is core-first: one React/Vite cockpit is shared by the local web app and
the Tauri 2 macOS app, while a Rust local core owns SQLite state, provider
switching, config writes, proxy routing, health checks, and logs.

## Scope

Supported in v1:

- Local web app at `http://localhost:3000`
- Tauri 2 macOS app, macOS 12+, bundle id `com.codexswitch.desktop`
- Claude Code, Codex CLI, Codex Desktop, and Gemini provider records
- Local SQLite database at `~/.codexswitch/codexswitch.db`
- Atomic config writes with backups under `~/.codexswitch/backups/`
- Local proxy for Claude Messages, OpenAI Chat Completions, OpenAI Responses,
  OpenAI models, and Gemini `generateContent`
- Local project telemetry for request metadata, provider/model/project summary,
  recent errors, and optional redacted request/response body previews
- Multi-tenant gateway records for users, projects, issued inbound keys,
  per-key quotas, token/cost accounting, and redacted distillation exports

Default agent profiles:

- `Claude Code`
- `Codex Workspace CLI`
- `Codex Desktop`
- `Gemini CLI`

Deferred from v1: OpenCode, OpenClaw, MCP, Skills, Sessions, WebDAV sync,
image endpoints, auto-updater, and always-on signed plus notarized distribution.

## References

Implementation references are cloned under `references/` and ignored by git:

- `references/ccx`
- `references/cc-switch`

Pinned upstream SHAs and refresh commands are documented in
`references/UPSTREAMS.md`.

## Download And Install

End users do not need to build from source. The current macOS preview release
can be installed from GitHub Releases:

- Release page: <https://github.com/eust-w/codexswitch/releases/tag/preview-0.1.0>
- Direct DMG: <https://github.com/eust-w/codexswitch/releases/download/preview-0.1.0/codexswitch_0.1.0_arm64.dmg>

The current preview DMG is for Apple Silicon / arm64 and is unsigned and
unnotarized. On first launch, macOS may require opening it from `System Settings
-> Privacy & Security`.

Homebrew Cask preview install:

```sh
brew tap eust-w/tap
brew install --cask codexswitch
```

## Source Development

```sh
npm install
npm run dev:web
```

The web command starts the local web app on `PORT` or `3000`, and starts the
Rust core on `CODEXSWITCH_CORE_PORT` or `3900` when `cargo` is available.

Useful scripts:

```sh
npm run lint
npm run build:web
npm run rust:test
npm run simulate:modules
npm run dev:mac
npm run build:mac
npm run build:mac:dmg
```

`build:mac` produces the macOS `.app` bundle. `build:mac:dmg` wraps that app in
a plain `hdiutil` DMG without Finder AppleScript customization, which keeps the
build usable in non-interactive agent environments.

To point the web app at an already running core:

```sh
CODEXSWITCH_CORE_URL=http://127.0.0.1:3900 npm run dev:web
```

To run the web frontend without trying to spawn the Rust core:

```sh
CODEXSWITCH_SKIP_CORE_SPAWN=true npm run dev:web
```

Source prerequisites:

- Node.js 20+
- npm
- Rust toolchain with `cargo` for the Rust core and macOS app builds
- Tauri 2 system prerequisites for macOS builds

## Local API

Public local web/API surface:

- `GET /health`
- `GET /api/state`
- `GET /api/providers`
- `POST /api/providers`
- `PUT /api/providers/:id`
- `DELETE /api/providers/:id`
- `POST /api/providers/:id/test`
- `POST /api/providers/:id/switch`
- `GET /api/providers/:id/models`
- `POST /api/providers/:id/models/sync`
- `GET /api/proxy/config`
- `PUT /api/proxy/config`
- `POST /api/proxy/start`
- `POST /api/proxy/stop`
- `GET /api/proxy/status`
- `GET /api/logs`
- `GET /api/telemetry/config`
- `PUT /api/telemetry/config`
- `GET /api/telemetry/summary`
- `GET /api/telemetry/traces`
- `DELETE /api/telemetry/traces`
- `GET /api/admin/users`
- `POST /api/admin/users`
- `PUT /api/admin/users/:id`
- `GET /api/admin/projects`
- `POST /api/admin/projects`
- `PUT /api/admin/projects/:id`
- `GET /api/admin/keys`
- `POST /api/admin/keys`
- `PUT /api/admin/keys/:id`
- `POST /api/admin/keys/:id/revoke`
- `GET /api/admin/usage/summary`
- `GET /api/admin/distillation/jobs`
- `POST /api/admin/distillation/jobs`
- `GET /api/admin/distillation/jobs/:id`

Proxy surface:

- `POST /v1/messages`
- `POST /v1/chat/completions`
- `POST /v1/responses`
- `GET /v1/models`
- `POST /v1beta/models/{model}:generateContent`

When at least one admin-issued gateway key exists, the proxy surface requires an
inbound key through `Authorization: Bearer`, `x-api-key`,
`x-codexswitch-key`, or `x-goog-api-key`. The raw key is returned only once by
`POST /api/admin/keys`; the database stores a SHA-256 hash and a short prefix.

## Config Writes

The core imports existing local CLI config on startup but does not rewrite user
CLI config until an explicit switch action is requested.

Switch writes are atomic and backup the previous file first:

- Claude Code: `~/.claude/settings.json`, or `~/.claude/claude.json` when that
  legacy file already exists
- Codex CLI and Codex Desktop: `~/.codex/config.toml`, preserving OAuth/login material such as
  `~/.codex/auth.json`
- Gemini: `~/.gemini/.env` plus `~/.gemini/settings.json`

API keys are masked in API responses; raw secrets are only used internally by
the local core for config writes and upstream proxy calls.

## 当前可用状态

当前实现已经达到 `codexswitch Core v1` 的完整可用范围：

- 本地 Web 版本可用：默认访问 `http://localhost:3000`
- macOS App 可用：Tauri 2 打包 `.app`，并可生成 `.dmg`
- Rust local core 可用：默认监听 `127.0.0.1:3900`
- SQLite 本地状态可用：数据库路径为 `~/.codexswitch/codexswitch.db`
- Provider 管理可用：支持新增、编辑、删除、测试、切换、排序
- 支持 Claude Code、Codex Workspace CLI、Codex Desktop、Gemini CLI 四个 v1 agent profile
- 本地代理可用：支持 Claude Messages、OpenAI Chat Completions、OpenAI Responses、OpenAI Models、Gemini generateContent
- 多租户网关可用：支持管理员创建使用者、项目、分发入站 key、设置请求/token quota、按 key/使用者/项目统计用量与成本
- 蒸馏导出可用：可从已脱敏 request/response trace 生成 JSONL 数据集
- 沙盒测试可用：会发起真实上游请求，不再使用 mock 响应
- 日志可用：Provider 测试、代理启停、沙盒请求、失败重试都会写入本地日志
- 项目遥测可用：默认只保存请求元数据，可选开启脱敏正文预览
- 本地配置写入可用：切换 Provider 时会原子写入配置，并先备份旧文件

当前支持的 agent：

- Claude Code
- Codex Workspace CLI
- Codex Desktop
- Gemini CLI

没有包含在 v1 范围内的能力仍然是延后项：OpenCode、OpenClaw、MCP、Skills、Sessions、WebDAV 同步、图片接口、自动更新，以及正式签名公证发布。

## 中文使用方法

### 1. 选择安装方式

普通用户优先使用 Release 或 Homebrew；只有开发、调试、二次开发时才需要从源码运行或构建。

#### 方式 A：从 GitHub Release 下载

适合普通 macOS 用户，不需要安装 Node.js、Rust 或 Tauri 开发环境。

- Release 页面：<https://github.com/eust-w/codexswitch/releases/tag/preview-0.1.0>
- 直接下载 DMG：<https://github.com/eust-w/codexswitch/releases/download/preview-0.1.0/codexswitch_0.1.0_arm64.dmg>

安装步骤：

1. 下载 `codexswitch_0.1.0_arm64.dmg`
2. 打开 DMG，把 `codexswitch.app` 拖到 `Applications`
3. 从 `Applications` 启动 `codexswitch`

当前 `preview-0.1.0` 是未签名、未公证的预览版，支持 Apple Silicon / arm64，要求 macOS 12 Monterey 或更新版本。首次打开如果被 Gatekeeper 拦截，请到 `System Settings -> Privacy & Security` 允许打开。

#### 方式 B：通过 Homebrew 安装

适合已经使用 Homebrew 的 macOS 用户。

```sh
brew tap eust-w/tap
brew install --cask codexswitch
```

查看 cask 信息：

```sh
brew info --cask eust-w/tap/codexswitch
```

卸载：

```sh
brew uninstall --cask codexswitch
```

卸载并删除本地应用数据：

```sh
brew uninstall --cask --zap codexswitch
```

#### 方式 C：从源码运行 Web 版本

适合开发、调试、验证本地改动。

本机需要：

- Node.js 20 或更高版本
- npm
- Rust toolchain / cargo
- macOS 上构建 Tauri App 所需的系统依赖

安装依赖并启动：

```sh
npm install
npm run dev:web
```

启动后访问：

```text
http://localhost:3000
```

`npm run dev:web` 会同时启动：

- Web 服务：默认 `3000`
- Rust core：默认 `3900`

健康检查：

```sh
curl http://127.0.0.1:3000/health
curl http://127.0.0.1:3900/health
```

如果已经单独启动 core，可以让 Web 指向现有 core：

```sh
CODEXSWITCH_CORE_URL=http://127.0.0.1:3900 npm run dev:web
```

如果只想启动前端，不自动拉起 Rust core：

```sh
CODEXSWITCH_SKIP_CORE_SPAWN=true npm run dev:web
```

### 全模块本地仿真

开发者可以运行一条命令验证所有核心模块的端到端模拟流程：

```sh
npm run simulate:modules
```

该脚本会使用临时 HOME 和本地 mock upstream，不会写入真实 `~/.claude`、`~/.codex` 或 `~/.gemini` 配置。覆盖范围包括：

- Provider 新增、测试、模型同步
- Claude Code / Gemini CLI agent 真实进程调用的 fake CLI 仿真
- 本地代理启动和入站 key 鉴权
- `/v1/models`、`/v1/chat/completions`、`/v1/messages`、`/v1/messages/count_tokens`、`/v1/responses`、Gemini `generateContent`
- 多租户使用者、项目、key、quota 与用量统计
- 项目遥测、请求审计、脱敏正文预览
- 蒸馏 JSONL 导出和脱敏校验
- 网关日志与默认 agent 状态

### 2. 配置 Provider

打开 macOS App 或本地 Web 后，在 `API Channels` 页面添加 Provider。

常用字段：

- `App Type`：选择 `claude`、`codex` 或 `gemini`
- `Service Type`：选择协议类型，例如 `claude`、`openai`、`responses`、`gemini`
- `Base URL`：上游 API 地址
- `API Key`：上游密钥，界面和 API 响应中会自动脱敏
- `Model Mapping`：把本地模型名映射到上游真实模型名
- `Route Prefix`：需要经过网关路径前缀时填写
- `Proxy URL`：该 Provider 需要单独走代理时填写

在 Provider 详情页点击 `获取模型列表` 可以从上游 `/v1/models` 或 Gemini 模型列表接口拉取并导入模型。界面中的 `custom-*` 是本地绑定 ID，不会作为真实模型名发送到上游；真实请求会使用模型名称，或优先使用 `Model Mapping` 配置后的值。

点击 `Test Latencies` 或单个 Provider 的测试按钮可以检查连通性。测试结果会写入日志。

### 3. 切换 CLI 配置

Provider 配好后，点击对应 Provider 的切换按钮会写入本机 CLI 配置。

写入目标：

- Claude Code：`~/.claude/settings.json`
- Claude Code legacy：如果 `~/.claude/claude.json` 已存在，则优先写它
- Codex Workspace CLI / Codex Desktop：`~/.codex/config.toml`
- Gemini：`~/.gemini/.env` 和 `~/.gemini/settings.json`

所有写入都会先备份旧文件到：

```text
~/.codexswitch/backups/
```

Codex Workspace CLI 与 Codex Desktop 共用 Codex 本地配置写入目标。Codex 的
OAuth/login 材料例如 `~/.codex/auth.json` 不会被覆盖。

### 4. Gemini CLI 真实进程调用

`Gemini CLI` agent 的 `Invoke Agent` 会启动本机 `gemini` 可执行文件，而不是只更新界面状态。默认执行方式为：

```sh
gemini --model gemini-2.5-pro --output-format json -p 'Do not call tools. Reply exactly: CODEXSWITCH_GEMINI_AGENT_OK'
```

如果需要指定 Gemini CLI 路径，可以设置：

```sh
CODEXSWITCH_GEMINI_CLI_PATH=/absolute/path/to/gemini
```

Gemini CLI 可以使用 `~/.gemini` 中的 OAuth 登录态运行；这和 `Google Gemini` REST provider 的 API key 配置是两条路径。也就是说，CLI 真实进程可用时，REST provider 仍可能显示为 `disconnected`，直到你为 REST provider 配置 API key 或可用的 Gemini REST 入口。

### 5. Claude Code 通过 CodexSwitch 网关调用

Claude Code 不需要登录 Anthropic 才能走 CodexSwitch。把 Claude Code 的 Anthropic-compatible endpoint 指向本地网关即可：

```sh
ANTHROPIC_BASE_URL=http://127.0.0.1:3900 \
ANTHROPIC_AUTH_TOKEN=codexswitch-local \
claude -p 'hello through codexswitch' --model qwen3.6-plus --output-format json
```

如果已经创建管理员分发 key，把 `ANTHROPIC_AUTH_TOKEN` 换成对应的 `csw_...` key。CodexSwitch 会接收 Claude Code 的 `/v1/messages` 和 `/v1/messages/count_tokens` 请求；当后端 provider 是 OpenAI-compatible 时，会把 Anthropic Messages 请求转换为 Chat Completions，并把响应转换回 Claude Messages 格式。Claude Code 的 `Invoke Agent` 也默认注入：

```text
ANTHROPIC_BASE_URL=http://127.0.0.1:3900
ANTHROPIC_AUTH_TOKEN=codexswitch-local
```

可以通过 `CODEXSWITCH_CLAUDE_BASE_URL`、`CODEXSWITCH_CLAUDE_AUTH_TOKEN` 或 invoke 请求体里的 `baseUrl`、`authToken` 覆盖。只有设置 `CODEXSWITCH_CLAUDE_INHERIT_AUTH_ENV=1` 时，才会继承父进程里的 Anthropic 认证环境变量。

### 6. 启动本地代理

在左侧底部点击 `START`。也可以直接调用 core API；下面示例使用 `3900`，源码 Web 模式下也可以把端口换成 `3000`。

```sh
curl -X POST http://127.0.0.1:3900/api/proxy/start
```

停止代理：

```sh
curl -X POST http://127.0.0.1:3900/api/proxy/stop
```

查看状态：

```sh
curl http://127.0.0.1:3900/api/proxy/status
```

可用代理接口：

- `POST /v1/messages`
- `POST /v1/chat/completions`
- `POST /v1/responses`
- `GET /v1/models`
- `POST /v1beta/models/{model}:generateContent`

### 7. 管理员分发 Key、统计用量、导出蒸馏数据

打开左侧 `租户与密钥` 页面可以创建使用者、项目、访问 key，查看请求数、token 数和估算成本，并运行蒸馏导出任务。签发 key 后只会展示一次原始 key；后续数据库只保存 hash 和短前缀。

也可以直接调用 core API：

```sh
USER_ID=$(curl -s -X POST http://127.0.0.1:3900/api/admin/users \
  -H 'Content-Type: application/json' \
  -d '{"displayName":"Ada Lovelace","email":"ada@example.com"}' | jq -r '.user.id')

PROJECT_ID=$(curl -s -X POST http://127.0.0.1:3900/api/admin/projects \
  -H 'Content-Type: application/json' \
  -d "{\"userId\":\"$USER_ID\",\"name\":\"Compiler Lab\",\"slug\":\"compiler-lab\"}" | jq -r '.project.id')

RAW_KEY=$(curl -s -X POST http://127.0.0.1:3900/api/admin/keys \
  -H 'Content-Type: application/json' \
  -d "{\"userId\":\"$USER_ID\",\"projectId\":\"$PROJECT_ID\",\"name\":\"Ada Codex\",\"quotaRequests\":1000,\"quotaTokens\":2000000}" | jq -r '.rawKey')
```

当系统里存在至少一个管理员签发的 key 后，兼容代理接口会要求入站 key。Codex、Claude、Gemini 客户端可把本地网关作为 Base URL，并用下面任一 header 传入分发 key：

```text
Authorization: Bearer csw_xxx
x-api-key: csw_xxx
x-codexswitch-key: csw_xxx
x-goog-api-key: csw_xxx
```

示例请求：

```sh
curl http://127.0.0.1:3900/v1/responses \
  -H "Authorization: Bearer $RAW_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"gpt-5.4","input":"hello"}'
```

统计摘要：

```sh
curl http://127.0.0.1:3900/api/admin/usage/summary
```

开启正文预览后，蒸馏导出会从脱敏后的 request/response 预览生成 JSONL：

```sh
curl -X PUT http://127.0.0.1:3900/api/telemetry/config \
  -H 'Content-Type: application/json' \
  -d '{"captureBodies":true}'

curl -X POST http://127.0.0.1:3900/api/admin/distillation/jobs \
  -H 'Content-Type: application/json' \
  -d "{\"name\":\"compiler-lab-export\",\"projectId\":\"$PROJECT_ID\",\"limit\":1000,\"includeFailures\":false}"
```

导出文件写在本地应用数据目录的 `distillation/` 子目录下。正文预览会先进行字段级脱敏，不会把常见 `key`、`token`、`secret`、`password`、`authorization`、`cookie`、`credential`、`session` 字段原样写入数据集。

### 8. 项目遥测与统计

打开左侧 `项目遥测` 页面可以查看本地请求统计，包括：

- 项目维度请求量、成功率、平均耗时
- Provider 维度请求量、成功率、平均耗时
- 模型维度请求量、成功率、平均耗时
- 最近请求 trace 和最近失败摘要
- 本地启发式摘要，不会自动调用外部 LLM

打开左侧 `请求审计` 页面可以按项目筛选，并按 `projectSequence` 顺序逐条查看每一组 request / response 预览。

默认情况下只保存元数据，不保存请求正文和响应正文。默认记录字段包括项目、工作区、路由、模型、Provider、状态码、耗时、请求大小、可用时的响应大小和错误信息。

每一条 trace 对应一次上游调用的 request/response pair，并包含 `projectSequence`。同一个项目内，`projectSequence` 从 1 递增，可以用来按发生顺序还原该项目的请求链。

如果 Codex 或其他客户端想让请求归属到具体项目，可以在代理请求中添加 header：

```text
x-codexswitch-project: my-project
x-codexswitch-workspace: /path/to/my-project
x-codexswitch-git-remote: git@github.com:owner/repo.git
x-codexswitch-session: local-session-id
```

也支持从 JSON body 中识别 `projectId`、`project_id`、`project`、`workspacePath`、`workspace_path`、`cwd`、`sessionId` 等字段。为了性能，body 解析只用于小请求和正文预览，不会为了代理统计读取上游流式响应。

查看遥测配置：

```sh
curl http://127.0.0.1:3900/api/telemetry/config
```

查看统计摘要：

```sh
curl http://127.0.0.1:3900/api/telemetry/summary
```

查看最近请求：

```sh
curl 'http://127.0.0.1:3900/api/telemetry/traces?limit=100'
```

按项目顺序导出 request/response pair：

```sh
curl 'http://127.0.0.1:3900/api/telemetry/traces?projectId=my-project&order=asc&limit=500'
```

开启脱敏正文预览：

```sh
curl -X PUT http://127.0.0.1:3900/api/telemetry/config \
  -H 'Content-Type: application/json' \
  -d '{"captureBodies":true,"retentionDays":30}'
```

正文预览会递归脱敏字段名中包含 `key`、`token`、`secret`、`password`、`authorization`、`cookie`、`credential`、`session` 的内容。

关闭正文预览：

```sh
curl -X PUT http://127.0.0.1:3900/api/telemetry/config \
  -H 'Content-Type: application/json' \
  -d '{"captureBodies":false}'
```

清空本地请求遥测：

```sh
curl -X DELETE http://127.0.0.1:3900/api/telemetry/traces
```

### 9. 从源码构建

构建命令只面向开发者或发布维护者；普通用户直接使用 Release 或 Homebrew。

#### 构建 Web

```sh
npm run lint
npm run build:web
```

#### 构建 macOS App

构建 `.app`：

```sh
npm run build:mac
```

构建 `.dmg`：

```sh
npm run build:mac:dmg
```

产物路径：

```text
src-tauri/target/release/bundle/macos/codexswitch.app
src-tauri/target/release/bundle/dmg/
```

本地 `npm run build:mac:dmg` 生成的是未签名、未公证的开发测试包。面向普通用户发布时，应优先使用 GitHub Release 产物。

### 10. GitHub Actions 编译和发布

仓库包含两个 workflow：

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`

CI 会在 `main` 分支 push、pull request 和手动触发时运行：

```sh
npm ci
npm run lint
npm run build:web
npm run rust:test
npm run simulate:modules
npm run build:mac
```

当前公开下载入口是 `preview-0.1.0` unsigned 预览版。`Release` workflow 现在支持两种路径：如果已经配置 Apple Developer secrets，会构建 signed + notarized DMG；如果没有配置，会自动构建 unsigned preview DMG，并继续发布到 GitHub Release。

需要配置的 secrets：

- `APPLE_CERTIFICATE`：Developer ID Application `.p12` 证书的 base64 内容
- `APPLE_CERTIFICATE_PASSWORD`：导出 `.p12` 时设置的密码
- `KEYCHAIN_PASSWORD`：CI 临时 keychain 密码，可自行生成一个强密码
- `APPLE_ID`：Apple Developer 账号邮箱
- `APPLE_PASSWORD`：Apple ID 的 app-specific password，不是网页登录密码
- `APPLE_TEAM_ID`：Apple Developer Team ID

导出证书后可用以下命令生成 `APPLE_CERTIFICATE`：

```sh
openssl base64 -A -in /path/to/developer-id-application.p12 -out certificate-base64.txt
```

发布正式 Release 有两种方式。

方式一：推送 tag 自动发布：

```sh
git tag v0.1.0
git push origin v0.1.0
```

方式二：在 GitHub Actions 页面手动运行 `Release` workflow，并填写 tag，例如：

```text
v0.1.0
```

Release workflow 会：

- 安装 Node.js 和 Rust
- 运行 lint、Web build、Rust tests、全模块仿真
- 检测 Apple Developer signing/notarization secrets
- 如果 secrets 完整，则导入 Apple Developer ID 证书，构建 signed + notarized macOS DMG，并运行 `codesign --verify` 和 `spctl --assess`
- 如果 secrets 不完整，则构建 unsigned preview DMG，workflow 不会因此失败
- 上传 DMG 为 workflow artifact
- 创建或更新 GitHub Release
- 把 `src-tauri/target/release/bundle/dmg/*.dmg` 上传为 Release asset

如果只想验证本地未签名包，可以运行：

```sh
npm run build:mac:dmg
```

如果要在本机完成签名和公证，需要先在本机 keychain 安装 Developer ID Application 证书，并设置 Apple notarization 环境变量，然后运行：

```sh
npm run build:mac:dmg:signed
```
