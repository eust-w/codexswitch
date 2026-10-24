# 使用指南

[返回首页](../README.md) · [API 参考](api.md) · [开发与构建](development.md)

## 配置 Provider

在「AI 通道管理 / API Channels」添加上游服务：

| 字段 | 用途 |
| --- | --- |
| App Type | 选择 `claude`、`codex` 或 `gemini` 客户端配置类型 |
| Service Type | 选择上游协议：`claude`、`openai`、`responses` 或 `gemini` |
| Base URL | 上游 API 地址，按服务商的协议与路径要求填写 |
| API Key | 上游服务凭据，与 codexswitch 签发的入站 Key 不同 |
| Model Mapping | 将请求中的模型名映射为上游实际支持的模型 ID |
| Route Prefix | 上游网关需要的额外路径前缀 |
| Proxy URL | 此 Provider 使用的网络代理地址 |

点击「获取模型列表」同步上游模型，再使用单个 Provider 测试或 `Test Latencies` 检查连接。界面中的 `custom-*` 是本地绑定 ID；发往上游的请求使用模型名称，或 Model Mapping 指定的名称。

沙盒测试会发起真实上游请求。示例中的 `your-model-id` 均需替换为已配置 Provider 支持的模型。

## 切换客户端配置

core 启动时会导入已有客户端配置；执行切换操作才会写入配置。写入前备份旧文件，再原子替换。

| 客户端 | 写入位置 |
| --- | --- |
| Claude Code | `~/.claude/settings.json`；若已有旧版 `~/.claude/claude.json`，优先写入旧版文件 |
| Codex CLI / Desktop | `~/.codex/config.toml`，保留 `~/.codex/auth.json` |
| Gemini CLI | `~/.gemini/.env` 和 `~/.gemini/settings.json` |

备份在 `~/.codexswitch/backups/`。Codex CLI 与 Desktop 共用配置，切换其中一方也会影响另一方读取到的配置。

切换操作不会自动让所有客户端流量经过本地代理。需要统计、网关 Key 或协议适配时，请另外配置客户端的网关地址。

## 启停网关

侧栏 `START / STOP` 和 macOS 托盘菜单均可控制代理。也可以调用：

```sh
curl --fail -X POST http://127.0.0.1:3900/api/proxy/start
curl --fail http://127.0.0.1:3900/api/proxy/status
```

这两条命令依次启动代理并查询状态。结束使用后，可单独运行停止命令：

```sh
curl --fail -X POST http://127.0.0.1:3900/api/proxy/stop
```

停止后继续测试请求前，需要重新启动代理。Web 模式下，`3000` 端口也转发这些 API。

协议接口见 [API 参考](api.md)。core 地址通常是 `http://127.0.0.1:3900`；客户端若会自行拼接 `/chat/completions` 等路径，Base URL 通常需填写 `http://127.0.0.1:3900/v1`。

## 接入 Claude Code

先安装 `claude`，配置可用 Provider 并启动代理。尚未签发网关 Key 时，可使用本地占位认证值：

```sh
ANTHROPIC_BASE_URL=http://127.0.0.1:3900 \
ANTHROPIC_AUTH_TOKEN=codexswitch-local \
claude -p 'hello through codexswitch' --model your-model-id --output-format json
```

如果已经签发网关 Key，将 `ANTHROPIC_AUTH_TOKEN` 替换为有效的 `csw_...` 入站 Key。占位值不会绕过网关鉴权。

当匹配到的 Provider 使用 OpenAI-compatible 协议时，网关会将 Claude Messages 转成 Chat Completions，再将结果转回 Claude Messages。实际模型、工具调用和流式行为仍受上游实现影响。

界面内 Claude Code 的 `Invoke Agent` 默认注入：

```text
ANTHROPIC_BASE_URL=http://127.0.0.1:3900
ANTHROPIC_AUTH_TOKEN=codexswitch-local
```

可通过 `CODEXSWITCH_CLAUDE_BASE_URL`、`CODEXSWITCH_CLAUDE_AUTH_TOKEN`，或 invoke 请求体中的 `baseUrl`、`authToken` 覆盖。仅在设置 `CODEXSWITCH_CLAUDE_INHERIT_AUTH_ENV=1` 时继承父进程的 Anthropic 认证环境变量。

## 接入 Gemini CLI

界面内 Gemini CLI 的 `Invoke Agent` 会启动本机 `gemini` 进程，使用所选模型和 JSON 输出模式。可在启动 core 前指定 CLI 路径：

```sh
CODEXSWITCH_GEMINI_CLI_PATH=/absolute/path/to/gemini npm run dev:web
```

Gemini CLI 可使用已有的 `~/.gemini` OAuth 登录态。CLI 登录与 Gemini REST Provider 的 API Key 是独立认证路径：CLI 可以运行，不代表 REST Provider 已连接。

目前 Codex CLI/Desktop 的界面内进程启动尚未实现；两者支持 Provider 管理与配置切换。

## 分发网关 Key

「租户与密钥 / Tenants & Keys」中，按使用者、项目、Key 的顺序创建记录，配置请求/token 配额。Key 原文只展示一次；数据库仅存 SHA-256 哈希与短前缀。可修改配额、状态或撤销 Key；配额为 `0` 表示不限制。

只要数据库中存在已签发 Key 记录，模型列表及模型推理代理接口就要求有效入站 Key。撤销所有 Key 不会自动恢复免认证状态。

以下 API 示例需要 `jq`，并使用虚构用户数据。请在同一 shell 中按顺序执行：

```sh
USER_ID=$(curl --fail --silent --show-error \
  -X POST http://127.0.0.1:3900/api/admin/users \
  -H 'Content-Type: application/json' \
  -d '{"displayName":"Demo User","email":"demo@example.com"}' | jq -er '.user.id')

PROJECT_ID=$(jq -n --arg userId "$USER_ID" \
  '{userId:$userId,name:"Demo Project",slug:"demo-project"}' | \
  curl --fail --silent --show-error \
    -X POST http://127.0.0.1:3900/api/admin/projects \
    -H 'Content-Type: application/json' --data-binary @- | jq -er '.project.id')

RAW_KEY=$(jq -n --arg userId "$USER_ID" --arg projectId "$PROJECT_ID" \
  '{userId:$userId,projectId:$projectId,name:"Demo Client",quotaRequests:1000,quotaTokens:2000000}' | \
  curl --fail --silent --show-error \
    -X POST http://127.0.0.1:3900/api/admin/keys \
    -H 'Content-Type: application/json' --data-binary @- | jq -er '.rawKey')
```

然后使用签发的 Key 请求已配置的 Responses Provider：

```sh
curl --fail http://127.0.0.1:3900/v1/responses \
  -H "Authorization: Bearer $RAW_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"your-model-id","input":"hello"}'
```

还支持通过 `x-api-key`、`x-codexswitch-key` 或 `x-goog-api-key` 传入 Key。管理 API 和本地 `count_tokens` 估算接口不受这套鉴权保护，详见 [访问边界](../README.md#数据与访问边界)。

查询用量：

```sh
curl --fail http://127.0.0.1:3900/api/admin/usage/summary
```

用量可能包含 token 估算；成本按内置规则估算，不是服务商实际账单。

## 遥测与请求审计

「项目遥测」展示项目、Provider、模型维度的请求量、成功率、平均耗时和最近错误。「请求审计」按项目内的 `projectSequence` 查看 request/response 记录。本地摘要不调用外部 LLM。

通过以下请求头标识来源：

```text
x-codexswitch-project: my-project
x-codexswitch-workspace: /path/to/my-project
x-codexswitch-git-remote: git@github.com:owner/repo.git
x-codexswitch-session: local-session-id
```

对于可解析的小请求，也会尝试读取 JSON body 中的 `projectId`、`project_id`、`project`、`workspacePath`、`workspace_path`、`cwd`、`sessionId` 等字段。

```sh
curl --fail http://127.0.0.1:3900/api/telemetry/config
curl --fail http://127.0.0.1:3900/api/telemetry/summary
curl --fail 'http://127.0.0.1:3900/api/telemetry/traces?projectId=my-project&order=asc&limit=500'
```

### 正文预览

默认只保存元数据，不保存请求和响应正文。开启正文预览后，采集范围受预览长度、响应类型与保留时间限制：

```sh
curl --fail -X PUT http://127.0.0.1:3900/api/telemetry/config \
  -H 'Content-Type: application/json' \
  -d '{"captureBodies":true,"retentionDays":30}'
```

JSON 中字段名包含 `key`、`token`、`secret`、`password`、`authorization`、`cookie`、`credential` 或 `session` 的值会被替换。普通对话文本、无法解析的内容和非 JSON 文本不能据此保证脱敏，导出前应检查实际数据。

关闭后，后续请求不再采集正文；已有记录不会因此立即删除：

```sh
curl --fail -X PUT http://127.0.0.1:3900/api/telemetry/config \
  -H 'Content-Type: application/json' \
  -d '{"captureBodies":false}'
```

### JSONL 蒸馏导出

先开启正文预览，再产生需要导出的请求。也可以直接在「租户与密钥」中创建导出任务。

沿用前面创建项目得到的 `PROJECT_ID`：

```sh
jq -n --arg projectId "$PROJECT_ID" \
  '{name:"demo-export",projectId:$projectId,limit:1000,includeFailures:false}' | \
  curl --fail -X POST http://127.0.0.1:3900/api/admin/distillation/jobs \
    -H 'Content-Type: application/json' --data-binary @-
```

输出位于 `~/.codexswitch/distillation/`。导出基于正文预览，可能被截断或缺少某些响应内容，不等同于完整训练数据或完整会话备份。

## 常见问题

| 现象 | 检查方向 |
| --- | --- |
| Web 显示 core 不可达 / 502 | 等待首次 Rust 编译完成，查看终端错误，再请求 `3900/health` |
| 端口被占用 | 连接已有 core，或按开发指南更换 Web/core 端口 |
| 代理请求失败 | 检查网关是否启动、Provider 类型、模型映射与上游凭据 |
| 签发 Key 后原客户端返回 401 | 客户端需改用有效入站 Key，占位值不再有效 |
| 配额返回 429 | 查看 Key 已用请求/token 数以及配额设置 |
| CLI 命令找不到 | 安装对应客户端，或设置 CLI 路径环境变量 |
| Gemini CLI 正常但 REST 测试失败 | 检查 REST Provider 的 API Key；CLI OAuth 不等于 REST 凭据 |
| 请求审计没有正文 | 默认不采集正文；开启后只影响新请求，且预览可能有限 |
| macOS 阻止启动 | 当前包未签名公证；核实下载来源后在「隐私与安全」中处理 |

普通卸载：

```sh
brew uninstall --cask codexswitch
```

Homebrew 的 `--zap` 选项还会删除 `~/.codexswitch`，包括数据库、配置备份和导出，使用前请自行保留所需数据。
