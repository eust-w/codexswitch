import express from "express";
import path from "path";
import { spawn, type ChildProcessWithoutNullStreams } from "child_process";
import { Readable } from "stream";
import { createServer as createViteServer } from "vite";

const WEB_PORT = Number(process.env.PORT || 3000);
const CORE_PORT = Number(process.env.CODEXSWITCH_CORE_PORT || 3900);
const CORE_URL = process.env.CODEXSWITCH_CORE_URL || `http://127.0.0.1:${CORE_PORT}`;
const RUST_PATH = `${process.env.HOME || ""}/.cargo/bin:/opt/homebrew/opt/rustup/bin:${process.env.PATH || ""}`;

let coreProcess: ChildProcessWithoutNullStreams | undefined;

function startCoreIfAvailable() {
  if (process.env.CODEXSWITCH_CORE_URL || process.env.CODEXSWITCH_SKIP_CORE_SPAWN === "true") {
    return;
  }

  coreProcess = spawn(
    "cargo",
    [
      "run",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--bin",
      "codexswitch-core",
      "--",
      "--port",
      String(CORE_PORT),
    ],
    {
      cwd: process.cwd(),
      env: { ...process.env, PATH: RUST_PATH },
      stdio: "pipe",
    },
  );

  coreProcess.stdout.on("data", (chunk) => process.stdout.write(`[core] ${chunk}`));
  coreProcess.stderr.on("data", (chunk) => process.stderr.write(`[core] ${chunk}`));
  coreProcess.on("error", (err) => {
    console.error(`[core] Unable to start Rust core: ${err.message}`);
  });
  coreProcess.on("exit", (code, signal) => {
    if (code !== 0 && signal !== "SIGTERM") {
      console.error(`[core] Rust core exited with code=${code} signal=${signal}`);
    }
  });
}

function isHopByHop(header: string) {
  return [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
  ].includes(header.toLowerCase());
}

async function proxyToCore(req: express.Request, res: express.Response) {
  const target = `${CORE_URL}${req.originalUrl}`;
  const headers = new Headers();
  for (const [key, value] of Object.entries(req.headers)) {
    if (!value || key.toLowerCase() === "host" || key.toLowerCase() === "content-length" || isHopByHop(key)) {
      continue;
    }
    if (Array.isArray(value)) {
      for (const item of value) headers.append(key, item);
    } else {
      headers.set(key, value);
    }
  }

  let body: BodyInit | undefined;
  if (req.method !== "GET" && req.method !== "HEAD") {
    if (Buffer.isBuffer(req.body)) {
      body = req.body;
    } else if (req.body !== undefined) {
      body = JSON.stringify(req.body);
      headers.set("content-type", headers.get("content-type") || "application/json");
    }
  }

  try {
    const upstream = await fetch(target, {
      method: req.method,
      headers,
      body,
    });
    res.status(upstream.status);
    upstream.headers.forEach((value, key) => {
      if (!isHopByHop(key)) {
        res.setHeader(key, value);
      }
    });
    if (upstream.body) {
      Readable.fromWeb(upstream.body as unknown as import("stream/web").ReadableStream).pipe(res);
    } else {
      res.end();
    }
  } catch (err) {
    res.status(502).json({
      error: "codexswitch core is not reachable",
      coreUrl: CORE_URL,
      detail: err instanceof Error ? err.message : String(err),
    });
  }
}

async function startServer() {
  startCoreIfAvailable();

  const app = express();

  app.get("/health", proxyToCore);
  app.use("/api", express.json({ limit: "50mb" }), proxyToCore);
  app.use("/v1", express.raw({ type: "*/*", limit: "50mb" }), proxyToCore);
  app.use("/v1beta", express.raw({ type: "*/*", limit: "50mb" }), proxyToCore);

  if (process.env.NODE_ENV !== "production") {
    const vite = await createViteServer({
      server: { middlewareMode: true },
      appType: "spa",
    });
    app.use(vite.middlewares);
  } else {
    const distPath = path.join(process.cwd(), "dist");
    app.use(express.static(distPath));
    app.get("*", (req, res) => {
      res.sendFile(path.join(distPath, "index.html"));
    });
  }

  const server = app.listen(WEB_PORT, "0.0.0.0", () => {
    console.log(`codexswitch web listening at http://localhost:${WEB_PORT}`);
    console.log(`codexswitch core target: ${CORE_URL}`);
  });

  const shutdown = () => {
    server.close();
    if (coreProcess && !coreProcess.killed) {
      coreProcess.kill("SIGTERM");
    }
  };
  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

startServer();
