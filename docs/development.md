# 开发与构建

[返回首页](../README.md) · [使用指南](usage.md) · [API 参考](api.md)

## 环境准备

当前构建和 CI 路径以 macOS 为准：

- Node.js 20+ 与 npm。
- Rust stable 与 Cargo；Rust 包声明 `rust-version = "1.85.0"`。
- Xcode Command Line Tools，以及 [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)。

```sh
git clone https://github.com/eust-w/codexswitch.git
cd codexswitch
npm ci
```

Web 服务默认同时启动 Rust core；只有前端而没有可达 core 时，管理与代理功能不可用。

## 本地运行

```sh
npm run dev:web
```

默认 Web 地址为 `http://localhost:3000`，core 地址为 `http://127.0.0.1:3900`。Web 监听 `0.0.0.0`，请在可信环境中运行。

启动桌面开发版：

```sh
npm run dev:mac
```

桌面应用启动内嵌 core，固定使用 `127.0.0.1:3900`；开发渲染器使用 `5173`。启动前检查是否已有应用或独立 core 占用同一端口。

单独运行 core：

```sh
cargo run --manifest-path src-tauri/Cargo.toml --bin codexswitch-core -- --port 3900
```

在另一个终端运行 Web 并连接已有 core：

```sh
CODEXSWITCH_CORE_URL=http://127.0.0.1:3900 npm run dev:web
```

调整 Web 与自动启动 core 的端口：

```sh
PORT=3001 CODEXSWITCH_CORE_PORT=3901 npm run dev:web
```

### 环境变量

| 变量 | 用途 |
| --- | --- |
| `PORT` | Node Web 服务端口，默认 `3000` |
| `CODEXSWITCH_CORE_PORT` | Web 自动启动的 core 端口，默认 `3900` |
| `CODEXSWITCH_CORE_URL` | 指向已有 core，同时跳过自动启动 |
| `CODEXSWITCH_SKIP_CORE_SPAWN=true` | 跳过自动启动；仍需 core 才能使用后端功能 |
| `CODEXSWITCH_CLAUDE_CLI_PATH` | `claude` 可执行文件路径 |
| `CODEXSWITCH_GEMINI_CLI_PATH` | `gemini` 可执行文件路径 |
| `CODEXSWITCH_CLAUDE_BASE_URL` | Claude Invoke 的网关地址，默认 `http://127.0.0.1:3900` |
| `CODEXSWITCH_CLAUDE_AUTH_TOKEN` | Claude Invoke 的入站 Key，默认本地占位值 |
| `CODEXSWITCH_CLAUDE_INHERIT_AUTH_ENV=1` | Claude Invoke 继承父进程 Anthropic 认证环境 |
| `CODEXSWITCH_KEEP_SIMULATION_HOME=1` | 模块仿真后保留临时目录，便于调试 |

环境变量应传给启动 core 的进程；连接已有 core 的 Web 进程不会修改那个 core 的环境。改用非默认 core 端口时，也需调整 Claude Invoke 的 Base URL。

## 验证

| 命令 | 检查内容 |
| --- | --- |
| `npm run lint` | TypeScript 类型检查（`tsc --noEmit`） |
| `npm run build:web` | 构建 Vite 前端与 Node 服务 |
| `npm run rust:test` | Rust 测试 |
| `npm run simulate:modules` | 隔离环境中的模块流程仿真 |
| `npm audit` | npm 依赖安全检查 |

模块仿真使用临时配置目录、本地 mock upstream 和 fake `claude/gemini` 可执行文件，覆盖 Provider 测试与模型同步、配置写入、代理协议、网关 Key/配额、遥测、请求审计和 JSONL 导出。仿真不写入真实的 `~/.claude`、`~/.codex` 或 `~/.gemini` 配置，也不能证明真实上游登录或调用成功。

## 构建产物

Web：

```sh
npm run build:web
NODE_ENV=production npm start
```

从仓库根目录运行。默认仍通过 Cargo 启动 core；若要复用已有 core，同样设置 `CODEXSWITCH_CORE_URL`。

macOS 应用和未签名 DMG：

```sh
npm run build:mac
npm run build:mac:dmg
```

`build:mac:dmg` 会自行构建应用，再使用 `hdiutil` 打包，无需先单独运行 `build:mac`。它不依赖 Finder AppleScript 布局操作。

| 产物 | 位置 |
| --- | --- |
| Web 前端与 Node 服务 | `dist/`，服务入口 `dist/server.cjs` |
| macOS 应用 | `src-tauri/target/release/bundle/macos/codexswitch.app` |
| macOS DMG | `src-tauri/target/release/bundle/dmg/` |

## CI 与发布

[CI workflow](../.github/workflows/ci.yml) 在 `main` push、面向 `main` 的 PR 或手动触发时运行：安装依赖、类型检查、Web 构建、Rust 测试、模块仿真和 macOS 应用构建。

[Release workflow](../.github/workflows/release.yml) 可由 `v*` tag 或手动输入 tag 触发。发布前先更新项目版本并验证产物，再选择尚未使用的 tag。

流程会验证代码、构建 DMG、上传 workflow artifact，并创建 GitHub Release 或更新其附件。签名凭据完整时会签名、公证，并执行 `codesign --verify` 和 `spctl --assess`；凭据不完整时生成未签名包。是否正式签名应以产物与 Release 说明为准，不能仅凭 tag 名判断。

### Apple 签名与公证

需要在仓库中配置以下 Actions secrets：

| Secret | 内容 |
| --- | --- |
| `APPLE_CERTIFICATE` | Developer ID Application `.p12` 证书的 base64 内容 |
| `APPLE_CERTIFICATE_PASSWORD` | 导出证书时设置的密码 |
| `KEYCHAIN_PASSWORD` | CI 临时 keychain 密码 |
| `APPLE_ID` | Apple Developer 账号邮箱 |
| `APPLE_PASSWORD` | Apple app-specific password |
| `APPLE_TEAM_ID` | Apple Developer Team ID |

本地签名构建需要先在 Keychain 安装有效 Developer ID Application 证书，并配置相应签名、公证环境，再执行：

```sh
npm run build:mac:dmg:signed
```

证书、私钥、凭据和含敏感信息的导出文件不应提交到仓库。

## 代码入口

| 路径 | 职责 |
| --- | --- |
| `src/` | React 控制台与 API 客户端 |
| `server.ts` | Web 服务、Rust core 启动和 HTTP 转发 |
| `src-tauri/src/core.rs` | Provider、配置、代理、SQLite、遥测和网关逻辑 |
| `src-tauri/src/core_server.rs` | 独立 core 入口 |
| `src-tauri/src/lib.rs` | Tauri 应用与托盘集成 |
| `scripts/simulate-core-v1.mjs` | 隔离模块仿真 |
| `scripts/build-macos-dmg.sh` | 未签名 DMG 打包 |
| `references/UPSTREAMS.md` | 上游实现参考及固定版本 |

`references/ccx` 与 `references/cc-switch` 仅用于本地研究，被 Git 忽略，不是项目运行依赖。
