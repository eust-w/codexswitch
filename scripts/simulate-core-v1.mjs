import { createServer } from "node:http";
import { mkdtemp, readFile, rm, writeFile, chmod } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import process from "node:process";

const repoRoot = path.resolve(new URL("..", import.meta.url).pathname);
const rustPath = `${process.env.HOME || ""}/.cargo/bin:/opt/homebrew/opt/rustup/bin:${process.env.PATH || ""}`;

const results = [];
const upstreamRequests = [];

function pass(name, detail = "") {
  results.push({ name, status: "pass", detail });
  console.log(`PASS ${name}${detail ? ` - ${detail}` : ""}`);
}

function fail(name, detail) {
  results.push({ name, status: "fail", detail });
  throw new Error(`${name}: ${detail}`);
}

function assertCondition(name, condition, detail = "") {
  if (!condition) fail(name, detail || "assertion failed");
  pass(name, detail);
}

async function freePort() {
  return await new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => resolve(address.port));
    });
  });
}

async function readJson(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const text = Buffer.concat(chunks).toString("utf8");
  return text ? JSON.parse(text) : {};
}

function jsonResponse(res, status, body, headers = {}) {
  const payload = JSON.stringify(body);
  res.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(payload),
    ...headers,
  });
  res.end(payload);
}

async function startMockUpstream() {
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url || "/", "http://127.0.0.1");
      if (req.method === "GET" && url.pathname === "/v1/models") {
        upstreamRequests.push({ route: "models" });
        return jsonResponse(res, 200, {
          object: "list",
          data: [
            { id: "sim-chat-model", object: "model", owned_by: "codexswitch-sim" },
            { id: "sim-response-model", object: "model", owned_by: "codexswitch-sim" },
            { id: "sim-gemini-model", object: "model", owned_by: "codexswitch-sim" },
          ],
        });
      }

      if (req.method === "POST" && url.pathname === "/v1/chat/completions") {
        const body = await readJson(req);
        upstreamRequests.push({ route: "chat", body });
        return jsonResponse(res, 200, {
          id: "chatcmpl-sim",
          object: "chat.completion",
          model: body.model || "sim-chat-model",
          choices: [
            {
              index: 0,
              message: {
                role: "assistant",
                content: `sim chat reply for ${body.model || "unknown"}`,
              },
              finish_reason: "stop",
            },
          ],
          usage: { prompt_tokens: 12, completion_tokens: 5, total_tokens: 17 },
        });
      }

      if (req.method === "POST" && url.pathname === "/v1/responses") {
        const body = await readJson(req);
        upstreamRequests.push({ route: "responses", body });
        return jsonResponse(res, 200, {
          id: "resp-sim",
          object: "response",
          model: body.model || "sim-response-model",
          output_text: `sim response reply for ${body.input || "empty"}`,
          usage: { input_tokens: 9, output_tokens: 4, total_tokens: 13 },
          api_key: "must-be-redacted",
        });
      }

      if (req.method === "POST" && url.pathname.startsWith("/v1beta/models/")) {
        const body = await readJson(req);
        upstreamRequests.push({ route: "gemini", path: url.pathname, body });
        return jsonResponse(res, 200, {
          candidates: [
            {
              content: {
                parts: [{ text: "sim gemini reply" }],
                role: "model",
              },
            },
          ],
          usageMetadata: {
            promptTokenCount: 6,
            candidatesTokenCount: 3,
            totalTokenCount: 9,
          },
        });
      }

      jsonResponse(res, 404, { error: `unhandled mock route ${req.method} ${url.pathname}` });
    } catch (err) {
      jsonResponse(res, 500, { error: err instanceof Error ? err.message : String(err) });
    }
  });

  const port = await freePort();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  return { server, url: `http://127.0.0.1:${port}` };
}

async function waitForHealth(baseUrl) {
  const deadline = Date.now() + 60_000;
  let lastError = "";
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${baseUrl}/health`);
      if (response.ok) return;
      lastError = `${response.status} ${await response.text()}`;
    } catch (err) {
      lastError = err instanceof Error ? err.message : String(err);
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`core did not become healthy: ${lastError}`);
}

async function startCore(homeDir, port, cliDir) {
  const child = spawn(
    "cargo",
    [
      "run",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--bin",
      "codexswitch-core",
      "--",
      "--port",
      String(port),
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        PATH: rustPath,
        CODEXSWITCH_TEST_HOME: homeDir,
        CODEXSWITCH_CLAUDE_CLI_PATH: path.join(cliDir, "claude"),
        CODEXSWITCH_GEMINI_CLI_PATH: path.join(cliDir, "gemini"),
      },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  child.stdout.on("data", (chunk) => process.stdout.write(`[core] ${chunk}`));
  child.stderr.on("data", (chunk) => process.stderr.write(`[core] ${chunk}`));
  child.once("exit", (code, signal) => {
    if (code !== 0 && signal !== "SIGTERM") {
      console.error(`[core] exited with code=${code} signal=${signal}`);
    }
  });
  const baseUrl = `http://127.0.0.1:${port}`;
  await waitForHealth(baseUrl);
  return { child, baseUrl };
}

async function createFakeCli(cliDir) {
  const claude = path.join(cliDir, "claude");
  const gemini = path.join(cliDir, "gemini");
  await writeFile(
    claude,
    "#!/bin/sh\nprintf '{\"type\":\"result\",\"result\":\"sim claude cli ok\",\"session_id\":\"sim-session\"}\\n'\n",
  );
  await writeFile(
    gemini,
    "#!/bin/sh\nprintf '{\"response\":\"sim gemini cli ok\",\"stats\":{\"tools\":{\"totalCalls\":0}}}\\n'\n",
  );
  await chmod(claude, 0o755);
  await chmod(gemini, 0o755);
}

async function request(baseUrl, method, route, body, headers = {}) {
  const response = await fetch(`${baseUrl}${route}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let parsed = text;
  try {
    parsed = text ? JSON.parse(text) : {};
  } catch {
    // Keep raw text for streaming or plain text responses.
  }
  return { response, body: parsed, text };
}

async function expectJson(baseUrl, method, route, body, headers = {}, expectedStatus = 200) {
  const result = await request(baseUrl, method, route, body, headers);
  if (result.response.status !== expectedStatus) {
    fail(route, `expected ${expectedStatus}, got ${result.response.status}: ${result.text}`);
  }
  return result.body;
}

async function run() {
  const homeDir = await mkdtemp(path.join(tmpdir(), "codexswitch-sim-home-"));
  const cliDir = await mkdtemp(path.join(tmpdir(), "codexswitch-sim-cli-"));
  const mock = await startMockUpstream();
  const corePort = await freePort();
  let core;

  try {
    await createFakeCli(cliDir);
    core = await startCore(homeDir, corePort, cliDir);
    const baseUrl = core.baseUrl;

    const health = await expectJson(baseUrl, "GET", "/health");
    assertCondition("health", health.ok === true, `core ${health.version}`);

    const responseProvider = await expectJson(baseUrl, "POST", "/api/providers", {
      id: "sim-responses",
      appType: "codex",
      name: "Simulation Responses Provider",
      type: "openai",
      providerType: "openai",
      serviceType: "responses",
      baseUrl: mock.url,
      apiKey: "sk-sim-responses",
      apiKeys: ["sk-sim-responses"],
      status: "connected",
      isCurrent: true,
      priority: 0,
    });
    assertCondition("provider:create:responses", responseProvider.provider === "sim-responses");

    await expectJson(baseUrl, "POST", "/api/providers", {
      id: "sim-chat",
      appType: "codex",
      name: "Simulation Chat Provider",
      type: "openai",
      providerType: "openai",
      serviceType: "openai",
      baseUrl: mock.url,
      apiKey: "sk-sim-chat",
      apiKeys: ["sk-sim-chat"],
      status: "connected",
      priority: 1,
    });
    pass("provider:create:chat");

    await expectJson(baseUrl, "POST", "/api/providers", {
      id: "sim-gemini",
      appType: "gemini",
      name: "Simulation Gemini Provider",
      type: "gemini",
      providerType: "gemini",
      serviceType: "gemini",
      baseUrl: mock.url,
      apiKey: "sk-sim-gemini",
      apiKeys: ["sk-sim-gemini"],
      status: "connected",
      priority: 2,
    });
    pass("provider:create:gemini");

    const sync = await expectJson(baseUrl, "POST", "/api/providers/sim-chat/models/sync");
    assertCondition("models:sync", sync.success === true && sync.upstreamCount >= 3);

    await expectJson(baseUrl, "POST", "/api/models", {
      id: "sim-responses-model",
      name: "sim-response-model",
      providerId: "sim-responses",
      tags: ["Simulation"],
      isDefault: true,
    });
    await expectJson(baseUrl, "POST", "/api/models", {
      id: "sim-gemini-model",
      name: "sim-gemini-model",
      providerId: "sim-gemini",
      tags: ["Simulation"],
      isDefault: true,
    });
    pass("models:create");

    const tested = await expectJson(baseUrl, "POST", "/api/providers/sim-chat/test");
    assertCondition("provider:test", tested.success === true && tested.status === "connected");

    const claudeInvoke = await expectJson(baseUrl, "POST", "/api/agents/claude-code/invoke", {
      prompt: "simulate claude cli",
      model: "sim-chat-model",
    });
    assertCondition("agent:claude", claudeInvoke.success === true, claudeInvoke.invocation.response);

    const geminiInvoke = await expectJson(baseUrl, "POST", "/api/agents/gemini-cli/invoke", {
      prompt: "simulate gemini cli",
      model: "sim-gemini-model",
    });
    assertCondition("agent:gemini", geminiInvoke.success === true, geminiInvoke.invocation.response);

    await expectJson(baseUrl, "PUT", "/api/telemetry/config", {
      enabled: true,
      captureBodies: true,
      retentionDays: 7,
      maxBodyPreviewBytes: 4096,
    });
    pass("telemetry:config");

    const user = await expectJson(baseUrl, "POST", "/api/admin/users", {
      displayName: "Simulation User",
      email: "sim@example.com",
    });
    const project = await expectJson(baseUrl, "POST", "/api/admin/projects", {
      userId: user.user.id,
      name: "Simulation Project",
      slug: "simulation-project",
    });
    const key = await expectJson(baseUrl, "POST", "/api/admin/keys", {
      userId: user.user.id,
      projectId: project.project.id,
      name: "Simulation Key",
      quotaRequests: 50,
      quotaTokens: 100000,
    });
    assertCondition("admin:key", String(key.rawKey).startsWith("csw_"));

    await expectJson(baseUrl, "POST", "/api/proxy/start");
    pass("proxy:start");

    const blocked = await request(baseUrl, "GET", "/v1/models");
    assertCondition("proxy:auth-required", blocked.response.status === 401, `status ${blocked.response.status}`);

    const auth = { authorization: `Bearer ${key.rawKey}` };
    const models = await expectJson(baseUrl, "GET", "/v1/models", undefined, auth);
    assertCondition(
      "proxy:models",
      models.data.some((item) => item.id === "sim-response-model" || item.id === "sim-chat-model"),
    );

    const chat = await expectJson(baseUrl, "POST", "/v1/chat/completions", {
      model: "sim-chat-model",
      messages: [{ role: "user", content: "hello chat" }],
    }, auth);
    assertCondition("proxy:chat", chat.choices?.[0]?.message?.content?.includes("sim chat reply"));

    const claude = await expectJson(baseUrl, "POST", "/v1/messages", {
      model: "claude-3-5-sonnet-20241022",
      messages: [{ role: "user", content: [{ type: "text", text: "hello claude adapter" }] }],
      max_tokens: 128,
    }, auth);
    assertCondition("proxy:claude-adapter", claude.content?.[0]?.text?.includes("sim chat reply"));

    const tokens = await expectJson(baseUrl, "POST", "/v1/messages/count_tokens", {
      model: "claude-3-5-sonnet-20241022",
      messages: [{ role: "user", content: "hello token counter" }],
    });
    assertCondition("proxy:count-tokens", Number(tokens.input_tokens) > 0);

    const responseResult = await expectJson(baseUrl, "POST", "/v1/responses", {
      model: "sim-response-model",
      input: "hello responses",
      metadata: { api_key: "must-be-redacted", token: "must-be-redacted" },
    }, auth);
    assertCondition("proxy:responses", responseResult.output_text?.includes("hello responses"));

    const gemini = await expectJson(baseUrl, "POST", "/v1beta/models/sim-gemini-model:generateContent", {
      contents: [{ parts: [{ text: "hello gemini" }] }],
    }, { "x-goog-api-key": key.rawKey });
    assertCondition("proxy:gemini", gemini.candidates?.[0]?.content?.parts?.[0]?.text === "sim gemini reply");

    const summary = await expectJson(baseUrl, "GET", "/api/telemetry/summary");
    assertCondition("telemetry:summary", summary.totalRequests >= 4, `${summary.totalRequests} requests`);

    const traceProjectId = project.project.slug || project.project.id;
    const traces = await expectJson(
      baseUrl,
      "GET",
      `/api/telemetry/traces?projectId=${encodeURIComponent(traceProjectId)}&order=asc`,
    );
    assertCondition("audit:traces", Array.isArray(traces) && traces.length >= 3, `${traces.length} traces`);
    assertCondition(
      "audit:redaction",
      traces.some((trace) => String(trace.requestBodyPreview || "").includes("[REDACTED]")),
    );

    const usage = await expectJson(baseUrl, "GET", "/api/admin/usage/summary");
    assertCondition("admin:usage", usage.totalRequests >= 4 && usage.totalTokens > 0);

    const distill = await expectJson(baseUrl, "POST", "/api/admin/distillation/jobs", {
      name: "Simulation Distillation Export",
      projectId: project.project.id,
      limit: 100,
      includeFailures: false,
    });
    assertCondition("distillation:job", distill.job.status === "completed");
    const exportText = await readFile(distill.job.exportPath, "utf8");
    assertCondition("distillation:export", exportText.includes("hello responses"));
    assertCondition("distillation:redaction", !exportText.includes("must-be-redacted"));

    const logs = await expectJson(baseUrl, "GET", "/api/logs");
    assertCondition("logs", Array.isArray(logs) && logs.length > 0, `${logs.length} logs`);

    const state = await expectJson(baseUrl, "GET", "/api/state");
    assertCondition(
      "state:agents",
      state.agents.some((agent) => agent.id === "codex-desktop")
        && state.agents.some((agent) => agent.id === "claude-code")
        && state.agents.some((agent) => agent.id === "gemini-cli"),
    );

    assertCondition(
      "upstream:coverage",
      ["models", "chat", "responses", "gemini"].every((route) =>
        upstreamRequests.some((request) => request.route === route),
      ),
      `${upstreamRequests.length} upstream calls`,
    );

    console.log(JSON.stringify({
      status: "ok",
      core: baseUrl,
      mock: mock.url,
      homeDir,
      passed: results.filter((item) => item.status === "pass").length,
      upstreamCalls: upstreamRequests.length,
      distillationExport: distill.job.exportPath,
    }, null, 2));
  } finally {
    if (core?.child && !core.child.killed) core.child.kill("SIGTERM");
    await new Promise((resolve) => mock.server.close(resolve));
    if (process.env.CODEXSWITCH_KEEP_SIMULATION_HOME !== "1") {
      await rm(homeDir, { recursive: true, force: true });
      await rm(cliDir, { recursive: true, force: true });
    }
  }
}

run().catch((err) => {
  console.error(`FAIL ${err instanceof Error ? err.stack || err.message : String(err)}`);
  process.exit(1);
});
