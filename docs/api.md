# API 参考

[返回首页](../README.md) · [使用指南](usage.md)

Rust core 默认地址为 `http://127.0.0.1:3900`。源码 Web 服务通过 `http://localhost:3000` 转发相同的 `/health`、`/api`、`/v1` 和 `/v1beta` 路径。可用路由以 [core.rs](../src-tauri/src/core.rs) 为准。

## 鉴权边界

- 数据库中存在至少一条网关 Key 记录时，模型推理与模型列表接口要求有效 Key，同时检查 Key、使用者、项目的状态、有效期和配额。
- 支持 `Authorization: Bearer <key>`、`x-api-key`、`x-codexswitch-key` 和 `x-goog-api-key`。
- `/health`、`/api/*` 管理接口没有登录鉴权；`/v1/messages/count_tokens` 为本地估算接口，也不检查网关 Key。
- 管理接口能写配置、签发 Key 和导出数据。当前 API 面向可信本地环境，不应直接向不可信网络开放。

## 模型代理

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| POST | `/v1/messages` | Claude Messages；支持适配到 OpenAI-compatible Chat Completions |
| POST | `/v1/messages/count_tokens` | 本地输入 token 估算，不是 Anthropic 官方精确计数 |
| POST | `/v1/chat/completions` | OpenAI-compatible Chat Completions |
| POST | `/v1/responses` | OpenAI-compatible Responses |
| GET | `/v1/models` | 返回本地已配置模型列表，不会为每次请求重新同步上游 |
| POST | `/v1beta/models/{model}:generateContent` | Gemini 内容生成 |

模型推理请求需要先启动代理并配置匹配的 Provider。支持的模型和参数取决于上游，当前不提供图片接口。

## 状态与 Provider

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| GET | `/health` | core 健康检查 |
| GET | `/api/state` | 控制台状态 |
| GET / POST | `/api/providers` | 列表 / 创建 Provider |
| PUT / DELETE | `/api/providers/:id` | 更新 / 删除 Provider |
| POST | `/api/providers/test-all` | 测试所有 Provider |
| POST | `/api/providers/:id/test` | 测试单个 Provider |
| POST | `/api/providers/:id/switch` | 切换 Provider 并写入客户端配置 |
| POST | `/api/providers/reorder` | 调整顺序 |
| GET | `/api/providers/:id/models` | 获取上游模型列表 |
| POST | `/api/providers/:id/models/sync` | 同步上游模型到本地 |
| GET / POST | `/api/models` | 列表 / 创建本地模型 |
| PUT / DELETE | `/api/models/:id` | 更新 / 删除本地模型 |

## 网关与客户端

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| GET / PUT | `/api/proxy/config` | 查看 / 更新代理配置 |
| POST | `/api/proxy/start` | 启用代理 |
| POST | `/api/proxy/stop` | 停用代理 |
| GET | `/api/proxy/status` | 查看代理状态 |
| POST | `/api/gateway/toggle` | 切换启停状态 |
| POST | `/api/gateway/restart` | 重启网关状态 |
| POST | `/api/gateway/quick-task` | 控制台快速任务 |
| PUT | `/api/agents/:id` | 更新客户端配置记录 |
| POST | `/api/agents/:id/invoke` | 启动 Claude Code 或 Gemini CLI；Codex 暂不支持 |
| POST | `/api/chat/send` | 沙盒真实上游请求 |

`start/stop` 控制代理转发状态，管理 HTTP 服务仍可访问。

## 使用者、项目与 Key

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| GET / POST | `/api/admin/users` | 列表 / 创建使用者 |
| PUT | `/api/admin/users/:id` | 更新使用者 |
| GET / POST | `/api/admin/projects` | 列表 / 创建项目 |
| PUT | `/api/admin/projects/:id` | 更新项目 |
| GET / POST | `/api/admin/keys` | 列表 / 签发入站 Key |
| PUT | `/api/admin/keys/:id` | 更新 Key 属性与配额 |
| POST | `/api/admin/keys/:id/revoke` | 撤销 Key |
| GET | `/api/admin/usage/summary` | 用量与估算成本 |

只有签发接口返回 `rawKey`。Key 创建和请求示例见 [使用指南](usage.md#分发网关-key)。

## 遥测、日志与导出

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| GET / PUT | `/api/telemetry/config` | 查看 / 更新采集配置 |
| GET | `/api/telemetry/summary` | 统计摘要 |
| GET | `/api/telemetry/traces` | 查询请求记录；支持 `projectId`、`order`、`limit` |
| DELETE | `/api/telemetry/traces` | 清空请求遥测 |
| GET | `/api/logs` | 查看日志 |
| POST | `/api/logs/clear` | 清空日志 |
| GET / POST | `/api/admin/distillation/jobs` | 列表 / 创建 JSONL 导出任务 |
| GET | `/api/admin/distillation/jobs/:id` | 查看导出任务 |
| POST | `/api/database/reset` | 重置本地数据库 |

清空和重置接口会删除相应本地记录；调用前确认已保留需要的数据。
