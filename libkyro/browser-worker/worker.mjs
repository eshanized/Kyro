#!/usr/bin/env node
/**
 * Kyro browser automation worker.
 *
 * Provider-neutral browser automation engine supporting:
 *  - Mozilla Firefox and Firefox ESR via native WebDriver BiDi
 *  - Chromium, Google Chrome, Microsoft Edge, Brave, Vivaldi, Opera via Chrome DevTools Protocol (CDP)
 *
 * No external driver (geckodriver/chromedriver) and no third-party npm dependency is required.
 * Built-in Node.js WebSocket and fetch are used for offline local packaging with Kyro.
 *
 * Protocol: newline delimited JSON on stdin/stdout.
 *   request : {"id":1,"op":"page.navigate","params":{"session_id":"s1","url":"..."}}
 *   response: {"id":1,"ok":true,"result":{...}}
 *             {"id":1,"ok":false,"error":{"category":"...","message":"..."}}
 *
 * The worker never evaluates caller-supplied arbitrary JavaScript. Every operation is
 * a fixed, purpose-built routine; the model can only ask for the operations
 * this file implements.
 */

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, existsSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, delimiter } from "node:path";
import { createServer } from "node:net";
import { setTimeout as delay } from "node:timers/promises";

const DEFAULT_NAV_TIMEOUT_MS = 30000;
const DEFAULT_ACTION_TIMEOUT_MS = 10000;

/* ------------------------------------------------------------------ */
/* Browser capability definitions                                      */
/* ------------------------------------------------------------------ */

const BROWSER_SPECS = [
  {
    id: "firefox",
    displayName: "Mozilla Firefox",
    family: "firefox",
    adapter: "webdriver-bidi",
    binaries: ["firefox"],
    paths: [
      "/usr/bin/firefox",
      "/usr/local/bin/firefox",
      "/usr/lib/firefox/firefox",
      "/opt/firefox/firefox",
      "/snap/bin/firefox",
      "/usr/sbin/firefox",
      "/var/lib/flatpak/exports/bin/org.mozilla.firefox",
      "/Applications/Firefox.app/Contents/MacOS/firefox",
      "C:\\Program Files\\Mozilla Firefox\\firefox.exe",
      "C:\\Program Files (x86)\\Mozilla Firefox\\firefox.exe",
    ],
  },
  {
    id: "firefox-esr",
    displayName: "Firefox ESR",
    family: "firefox",
    adapter: "webdriver-bidi",
    binaries: ["firefox-esr"],
    paths: [
      "/usr/bin/firefox-esr",
      "/usr/local/bin/firefox-esr",
      "/usr/lib/firefox-esr/firefox-esr",
      "/usr/sbin/firefox-esr",
    ],
  },
  {
    id: "chrome",
    displayName: "Google Chrome",
    family: "chromium",
    adapter: "cdp",
    binaries: ["google-chrome", "google-chrome-stable"],
    paths: [
      "/usr/bin/google-chrome",
      "/usr/bin/google-chrome-stable",
      "/opt/google/chrome/chrome",
      "/usr/sbin/google-chrome",
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
      "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
    ],
  },
  {
    id: "chromium",
    displayName: "Chromium",
    family: "chromium",
    adapter: "cdp",
    binaries: ["chromium", "chromium-browser"],
    paths: [
      "/usr/bin/chromium",
      "/usr/bin/chromium-browser",
      "/usr/sbin/chromium",
      "/snap/bin/chromium",
      "/Applications/Chromium.app/Contents/MacOS/Chromium",
      "C:\\Program Files\\Chromium\\Application\\chrome.exe",
    ],
  },
  {
    id: "edge",
    displayName: "Microsoft Edge",
    family: "chromium",
    adapter: "cdp",
    binaries: ["microsoft-edge", "microsoft-edge-stable"],
    paths: [
      "/usr/bin/microsoft-edge",
      "/usr/bin/microsoft-edge-stable",
      "/opt/microsoft/msedge/msedge",
      "/usr/sbin/microsoft-edge",
      "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
      "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
      "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
    ],
  },
  {
    id: "brave",
    displayName: "Brave Browser",
    family: "chromium",
    adapter: "cdp",
    binaries: ["brave-browser", "brave"],
    paths: [
      "/usr/bin/brave-browser",
      "/usr/bin/brave",
      "/usr/sbin/brave-browser",
      "/usr/sbin/brave",
      "/opt/brave.com/brave/brave-browser",
      "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
      "C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe",
    ],
  },
  {
    id: "vivaldi",
    displayName: "Vivaldi",
    family: "chromium",
    adapter: "cdp",
    binaries: ["vivaldi", "vivaldi-stable"],
    paths: [
      "/usr/bin/vivaldi",
      "/usr/bin/vivaldi-stable",
      "/opt/vivaldi/vivaldi",
      "/usr/sbin/vivaldi",
      "/Applications/Vivaldi.app/Contents/MacOS/Vivaldi",
      "C:\\Program Files\\Vivaldi\\Application\\vivaldi.exe",
    ],
  },
  {
    id: "opera",
    displayName: "Opera",
    family: "chromium",
    adapter: "cdp",
    binaries: ["opera"],
    paths: [
      "/usr/bin/opera",
      "/usr/lib/x86_64-linux-gnu/opera/opera",
      "/usr/sbin/opera",
      "/Applications/Opera.app/Contents/MacOS/Opera",
      "C:\\Program Files\\Opera\\launcher.exe",
    ],
  },
];

/* ------------------------------------------------------------------ */
/* Browser discovery                                                   */
/* ------------------------------------------------------------------ */

function findOnPath(name) {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(delimiter)) {
    if (!dir) continue;
    const candidate = join(dir, name);
    try {
      if (statSync(candidate).isFile()) return candidate;
    } catch {
      /* not present */
    }
  }
  return null;
}

async function probeVersion(path) {
  return new Promise((resolve) => {
    const child = spawn(path, ["--version"], { stdio: ["ignore", "pipe", "pipe"] });
    let out = "";
    child.stdout.on("data", (d) => (out += d.toString()));
    child.stderr.on("data", (d) => (out += d.toString()));
    const timer = setTimeout(() => {
      try { child.kill("SIGKILL"); } catch { /* ignore */ }
      resolve({ version: null, ok: false });
    }, 5000);
    child.on("error", () => {
      clearTimeout(timer);
      resolve({ version: null, ok: false });
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      if (code !== 0 && !out) {
        resolve({ version: null, ok: false });
        return;
      }
      const m = out.match(/(\d+\.\d+(\.\d+)?(\.\d+)?)/);
      resolve({ version: m ? m[1] : out.trim() || null, ok: true });
    });
  });
}

async function detectDefaultBrowserId() {
  if (process.platform === "linux") {
    try {
      const out = await new Promise((resolve) => {
        const child = spawn("xdg-settings", ["get", "default-web-browser"], { stdio: ["ignore", "pipe", "pipe"] });
        let buf = "";
        child.stdout.on("data", (d) => (buf += d.toString()));
        child.on("close", () => resolve(buf.trim()));
        child.on("error", () => resolve(""));
      });
      if (out) {
        const lower = out.toLowerCase();
        if (lower.includes("firefox-esr")) return "firefox-esr";
        if (lower.includes("firefox")) return "firefox";
        if (lower.includes("chrome")) return "chrome";
        if (lower.includes("chromium")) return "chromium";
        if (lower.includes("edge")) return "edge";
        if (lower.includes("brave")) return "brave";
        if (lower.includes("vivaldi")) return "vivaldi";
        if (lower.includes("opera")) return "opera";
      }
    } catch { /* best effort */ }

    try {
      const out = await new Promise((resolve) => {
        const child = spawn("xdg-mime", ["query", "default", "x-scheme-handler/http"], { stdio: ["ignore", "pipe", "pipe"] });
        let buf = "";
        child.stdout.on("data", (d) => (buf += d.toString()));
        child.on("close", () => resolve(buf.trim()));
        child.on("error", () => resolve(""));
      });
      if (out) {
        const lower = out.toLowerCase();
        if (lower.includes("firefox-esr")) return "firefox-esr";
        if (lower.includes("firefox")) return "firefox";
        if (lower.includes("chrome")) return "chrome";
        if (lower.includes("chromium")) return "chromium";
        if (lower.includes("edge")) return "edge";
        if (lower.includes("brave")) return "brave";
        if (lower.includes("vivaldi")) return "vivaldi";
        if (lower.includes("opera")) return "opera";
      }
    } catch { /* best effort */ }
  }
  return null;
}

async function discoverBrowsers() {
  const defaultBrowserId = await detectDefaultBrowserId();
  const results = [];

  for (const spec of BROWSER_SPECS) {
    let resolvedPath = null;
    for (const bin of spec.binaries) {
      const onPath = findOnPath(bin);
      if (onPath) {
        resolvedPath = onPath;
        break;
      }
    }
    if (!resolvedPath) {
      for (const candidate of spec.paths) {
        if (existsSync(candidate)) {
          resolvedPath = candidate;
          break;
        }
      }
    }

    if (resolvedPath) {
      const probe = await probeVersion(resolvedPath);
      if (probe.ok) {
        results.push({
          id: spec.id,
          name: spec.id,
          display_name: spec.displayName,
          family: spec.family,
          path: resolvedPath,
          version: probe.version,
          available: true,
          automatable: true,
          adapter: spec.adapter,
          unavailable_reason: null,
          is_default: spec.id === defaultBrowserId,
        });
        continue;
      }
    }

    results.push({
      id: spec.id,
      name: spec.id,
      display_name: spec.displayName,
      family: spec.family,
      path: resolvedPath || "",
      version: null,
      available: false,
      automatable: false,
      adapter: spec.adapter,
      unavailable_reason: `Executable for ${spec.displayName} was not found on PATH or standard directories.`,
      is_default: spec.id === defaultBrowserId,
    });
  }

  return { browsers: results, defaultBrowserId };
}

/* ------------------------------------------------------------------ */
/* Minimal WebDriver BiDi client (Firefox)                             */
/* ------------------------------------------------------------------ */

class BidiClient {
  constructor(url) {
    this.url = url;
    this.ws = null;
    this.nextId = 0;
    this.pending = new Map();
    this.listeners = new Map();
    this.closed = false;
  }

  async connect() {
    await new Promise((resolve, reject) => {
      let settled = false;
      const ws = new WebSocket(this.url);
      this.ws = ws;
      ws.onopen = () => {
        settled = true;
        resolve();
      };
      ws.onerror = (e) => {
        if (!settled) reject(new Error(`Failed to connect to WebDriver BiDi endpoint: ${e.message || e.type}`));
      };
    });

    this.ws.onmessage = (event) => {
      let payload;
      try {
        payload = JSON.parse(String(event.data));
      } catch {
        return;
      }
      if (payload.id !== undefined && this.pending.has(payload.id)) {
        const entry = this.pending.get(payload.id);
        this.pending.delete(payload.id);
        if (payload.type === "error") {
          entry.reject(new BiDiError(payload.error || "bidi_error", payload.message || "WebDriver BiDi error", payload.stacktrace));
        } else {
          entry.resolve(payload.result);
        }
        return;
      }
      if (payload.method) {
        const handlers = this.listeners.get(payload.method);
        if (handlers) for (const h of handlers) { try { h(payload.params || {}); } catch { /* ignore */ } }
      }
    };

    this.ws.onclose = () => {
      this.closed = true;
      for (const [, entry] of this.pending) {
        entry.reject(new BiDiError("disconnected", "WebDriver BiDi connection closed"));
      }
      this.pending.clear();
    };
  }

  on(method, handler) {
    if (!this.listeners.has(method)) this.listeners.set(method, []);
    this.listeners.get(method).push(handler);
  }

  send(method, params = {}, timeoutMs = 30000) {
    if (this.closed) return Promise.reject(new BiDiError("disconnected", "WebDriver BiDi connection is closed"));
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new BiDiError("timeout", `WebDriver BiDi command '${method}' timed out after ${timeoutMs}ms`));
        }
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (v) => { clearTimeout(timer); resolve(v); },
        reject: (e) => { clearTimeout(timer); reject(e); },
      });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  close() {
    try { if (this.ws) this.ws.close(); } catch { /* ignore */ }
  }
}

class BiDiError extends Error {
  constructor(code, message, stacktrace) {
    super(message);
    this.code = code;
    this.stacktrace = stacktrace;
  }
}

/* ------------------------------------------------------------------ */
/* Firefox session (WebDriver BiDi)                                    */
/* ------------------------------------------------------------------ */

const CONSOLE_EVENTS = ["log.entryAdded"];
const NETWORK_EVENTS = [
  "network.beforeRequestSent",
  "network.responseCompleted",
  "network.fetchError",
];

class FirefoxSession {
  constructor(id, opts) {
    this.id = id;
    this.opts = opts;
    this.browserId = opts.browserId;
    this.browserPath = opts.browserPath;
    this.browserVersion = null;
    this.process = null;
    this.client = null;
    this.profileDir = null;
    this.port = null;
    this.contextId = null;
    this.consoleEntries = [];
    this.networkRequests = new Map();
    this.networkFailed = [];
    this.closed = false;
    this.adapter = "webdriver-bidi";
  }

  async start() {
    this.profileDir = mkdtempSync(join(tmpdir(), "kyro-firefox-profile-"));
    this.port = await findFreePort();
    const args = [
      "--remote-debugging-port", String(this.port),
      "--profile", this.profileDir,
      "--no-remote",
      "--new-instance",
    ];
    if (this.opts.headless) args.push("--headless");
    args.push("about:blank");

    this.process = spawn(this.browserPath, args, {
      stdio: ["ignore", "ignore", "ignore"],
      env: { ...process.env, MOZ_HEADLESS: this.opts.headless ? "1" : "0" },
      detached: true,
    });
    this.process.on("error", (e) => {
      this.processError = e.message;
    });

    writeFirefoxProfilePrefs(this.profileDir);

    const url = `ws://127.0.0.1:${this.port}/session`;
    const deadline = Date.now() + 30000;
    let lastError = null;
    while (Date.now() < deadline) {
      if (this.process.exitCode !== null) {
        throw new BiDiError("browser_crash", `Firefox exited during startup with code ${this.process.exitCode}`);
      }
      try {
        const client = new BidiClient(url);
        await client.connect();
        this.client = client;
        break;
      } catch (e) {
        lastError = e;
        await delay(250);
      }
    }
    if (!this.client) {
      throw new BiDiError("browser_start_failed", `Firefox WebDriver BiDi endpoint did not become available: ${lastError ? lastError.message : "unknown"}`);
    }

    await this.client.send("session.new", { capabilities: {} });
    await this.client.send("session.subscribe", { events: [...CONSOLE_EVENTS, ...NETWORK_EVENTS] });
    this.client.on("log.entryAdded", (p) => this.onConsole(p));
    this.client.on("network.beforeRequestSent", (p) => this.onRequest(p));
    this.client.on("network.responseCompleted", (p) => this.onResponse(p));
    this.client.on("network.fetchError", (p) => this.onFetchError(p));

    const tree = await this.client.send("browsingContext.getTree", {});
    if (tree.contexts && tree.contexts.length > 0) {
      this.contextId = tree.contexts[0].context;
    } else {
      const created = await this.client.send("browsingContext.create", { type: "tab" });
      this.contextId = created.context;
    }
    return this;
  }

  onConsole(p) {
    const level = p.level || "log";
    const text = p.text || "";
    const kind = p.type || "console";
    this.consoleEntries.push({
      level,
      type: kind,
      text,
      stack: p.stackTrace || null,
      source: p.source ? (p.source.realm || null) : null,
      timestamp: p.timestamp || Date.now(),
    });
    if (this.consoleEntries.length > 2000) this.consoleEntries.shift();
  }

  onRequest(p) {
    const r = p.request || {};
    const id = r.request || r.navigation || JSON.stringify([r.url, p.isNavigation]);
    this.networkRequests.set(id, {
      request_id: id,
      url: r.url,
      method: r.method,
      status: null,
      mime_type: null,
      error: null,
      navigation: !!p.isNavigation,
    });
  }

  onResponse(p) {
    const r = p.request || {};
    const resp = p.response || {};
    const id = r.request || r.navigation;
    const existing = this.networkRequests.get(id);
    const record = existing || {
      request_id: id,
      url: r.url,
      method: r.method,
      navigation: false,
    };
    record.status = resp.status;
    record.mime_type = resp.mimeType || null;
    this.networkRequests.set(id, record);
  }

  onFetchError(p) {
    const r = p.request || {};
    const id = r.request || r.navigation || r.url;
    this.networkFailed.push({
      request_id: id,
      url: r.url,
      method: r.method,
      error: p.errorText || "network request failed",
      navigation: !!p.isNavigation,
      timestamp: Date.now(),
    });
    if (this.networkFailed.length > 500) this.networkFailed.shift();
  }

  async context() {
    if (this.contextId) return this.contextId;
    const tree = await this.client.send("browsingContext.getTree", {});
    if (tree.contexts && tree.contexts.length) {
      this.contextId = tree.contexts[0].context;
      return this.contextId;
    }
    const created = await this.client.send("browsingContext.create", { type: "tab" });
    this.contextId = created.context;
    return this.contextId;
  }

  async close() {
    if (this.closed) return;
    this.closed = true;
    try {
      if (this.client) {
        await this.client.send("session.end", {}, 5000).catch(() => {});
        this.client.close();
      }
    } catch { /* ignore */ }
    try {
      if (this.process && this.process.exitCode === null) {
        process.kill(-this.process.pid, "SIGTERM");
      }
    } catch { /* ignore */ }
    try {
      if (this.process && this.process.exitCode === null) {
        this.process.kill("SIGTERM");
      }
    } catch { /* ignore */ }
    await delay(150);
    try {
      if (this.process && this.process.exitCode === null) {
        process.kill(-this.process.pid, "SIGKILL");
        this.process.kill("SIGKILL");
      }
    } catch { /* ignore */ }
    if (this.profileDir) {
      try { rmSync(this.profileDir, { recursive: true, force: true }); } catch { /* ignore */ }
    }
  }
}

/* ------------------------------------------------------------------ */
/* Chromium session (Chrome DevTools Protocol - CDP)                   */
/* ------------------------------------------------------------------ */

class ChromiumSession {
  constructor(id, opts) {
    this.id = id;
    this.opts = opts;
    this.browserId = opts.browserId;
    this.browserPath = opts.browserPath;
    this.browserVersion = null;
    this.process = null;
    this.ws = null;
    this.port = null;
    this.profileDir = null;
    this.nextId = 0;
    this.pending = new Map();
    this.consoleEntries = [];
    this.networkRequests = new Map();
    this.networkFailed = [];
    this.closed = false;
    this.adapter = "cdp";
  }

  async start() {
    this.profileDir = mkdtempSync(join(tmpdir(), "kyro-chromium-profile-"));
    this.port = await findFreePort();
    const args = [
      `--remote-debugging-port=${this.port}`,
      `--user-data-dir=${this.profileDir}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-networking",
      "--disable-sync",
      "--disable-default-apps",
      "--disable-features=Translate,OptimizationHints",
    ];
    if (this.opts.headless) {
      args.push("--headless=new");
    }
    args.push("about:blank");

    this.process = spawn(this.browserPath, args, {
      stdio: ["ignore", "ignore", "ignore"],
      detached: true,
    });

    const deadline = Date.now() + 30000;
    let targetWs = null;
    let lastError = null;

    while (Date.now() < deadline) {
      if (this.process.exitCode !== null) {
        throw new BiDiError(
          "browser_crash",
          `${this.opts.browserDisplayName || this.browserId} exited during startup with code ${this.process.exitCode}`
        );
      }
      try {
        const res = await fetch(`http://127.0.0.1:${this.port}/json/list`);
        const targets = await res.json();
        const page = targets.find((t) => t.type === "page");
        if (page && page.webSocketDebuggerUrl) {
          targetWs = page.webSocketDebuggerUrl;
          break;
        }
      } catch (e) {
        lastError = e;
      }
      await delay(200);
    }

    if (!targetWs) {
      throw new BiDiError(
        "browser_start_failed",
        `${this.opts.browserDisplayName || this.browserId} DevTools endpoint did not become available: ${lastError ? lastError.message : "timed out"}`
      );
    }

    await new Promise((resolve, reject) => {
      let settled = false;
      const ws = new WebSocket(targetWs);
      this.ws = ws;
      ws.onopen = () => {
        settled = true;
        resolve();
      };
      ws.onerror = (e) => {
        if (!settled) reject(new Error(`Failed to connect to Chromium CDP endpoint: ${e.message || e.type}`));
      };
    });

    this.ws.onmessage = (event) => {
      let payload;
      try {
        payload = JSON.parse(String(event.data));
      } catch {
        return;
      }
      if (payload.id !== undefined && this.pending.has(payload.id)) {
        const entry = this.pending.get(payload.id);
        this.pending.delete(payload.id);
        if (payload.error) {
          entry.reject(new BiDiError(payload.error.code ? String(payload.error.code) : "cdp_error", payload.error.message || "CDP command failed"));
        } else {
          entry.resolve(payload.result);
        }
        return;
      }
      if (payload.method) {
        this.handleEvent(payload.method, payload.params || {});
      }
    };

    this.ws.onclose = () => {
      this.closed = true;
      for (const [, entry] of this.pending) {
        entry.reject(new BiDiError("disconnected", "Chromium CDP connection closed"));
      }
      this.pending.clear();
    };

    await this.send("Page.enable");
    await this.send("Runtime.enable");
    await this.send("Network.enable");
    await this.send("Log.enable");

    if (this.opts.viewport) {
      await this.send("Emulation.setDeviceMetricsOverride", {
        width: this.opts.viewport.width,
        height: this.opts.viewport.height,
        deviceScaleFactor: 1,
        mobile: false,
      });
    }

    return this;
  }

  send(method, params = {}, timeoutMs = 30000) {
    if (this.closed) return Promise.reject(new BiDiError("disconnected", "CDP connection is closed"));
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new BiDiError("timeout", `CDP command '${method}' timed out after ${timeoutMs}ms`));
        }
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (v) => { clearTimeout(timer); resolve(v); },
        reject: (e) => { clearTimeout(timer); reject(e); },
      });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  handleEvent(method, params) {
    if (method === "Runtime.consoleAPICalled") {
      const args = params.args || [];
      const text = args.map((a) => (a.value !== undefined ? String(a.value) : (a.description || ""))).join(" ");
      let level = params.type || "log";
      if (level === "warning") level = "warn";
      this.consoleEntries.push({
        level,
        type: params.type || "console",
        text,
        stack: params.stackTrace || null,
        source: null,
        timestamp: params.timestamp ? Math.round(params.timestamp) : Date.now(),
      });
      if (this.consoleEntries.length > 2000) this.consoleEntries.shift();
    } else if (method === "Runtime.exceptionThrown") {
      const details = params.exceptionDetails || {};
      const text = (details.exception && (details.exception.description || details.exception.value))
        || details.text
        || "Uncaught error";
      this.consoleEntries.push({
        level: "error",
        type: "error",
        text: String(text),
        stack: details.stackTrace || null,
        source: details.url || null,
        timestamp: params.timestamp ? Math.round(params.timestamp) : Date.now(),
      });
      if (this.consoleEntries.length > 2000) this.consoleEntries.shift();
    } else if (method === "Log.entryAdded") {
      const entry = params.entry || {};
      let level = entry.level || "log";
      if (level === "warning") level = "warn";
      this.consoleEntries.push({
        level,
        type: entry.source || "log",
        text: entry.text || "",
        stack: entry.stackTrace || null,
        source: entry.url || null,
        timestamp: entry.timestamp || Date.now(),
      });
      if (this.consoleEntries.length > 2000) this.consoleEntries.shift();
    } else if (method === "Network.requestWillBeSent") {
      const r = params.request || {};
      const id = params.requestId || r.url;
      this.networkRequests.set(id, {
        request_id: id,
        url: r.url,
        method: r.method,
        status: null,
        mime_type: null,
        error: null,
        navigation: params.type === "Document",
      });
    } else if (method === "Network.responseReceived") {
      const id = params.requestId;
      const resp = params.response || {};
      const existing = this.networkRequests.get(id);
      const record = existing || {
        request_id: id,
        url: resp.url,
        method: null,
        navigation: false,
      };
      record.status = resp.status;
      record.mime_type = resp.mimeType || null;
      this.networkRequests.set(id, record);
    } else if (method === "Network.loadingFailed") {
      const id = params.requestId;
      const existing = this.networkRequests.get(id);
      const url = existing ? existing.url : (params.requestId || "unknown");
      this.networkFailed.push({
        request_id: id,
        url,
        method: existing ? existing.method : null,
        error: params.errorText || "network request failed",
        navigation: params.type === "Document",
        timestamp: Date.now(),
      });
      if (this.networkFailed.length > 500) this.networkFailed.shift();
    }
  }

  async context() {
    return "default";
  }

  async close() {
    if (this.closed) return;
    this.closed = true;
    try {
      if (this.ws) {
        this.ws.close();
      }
    } catch { /* ignore */ }
    try {
      if (this.process && this.process.exitCode === null) {
        process.kill(-this.process.pid, "SIGTERM");
      }
    } catch { /* ignore */ }
    try {
      if (this.process && this.process.exitCode === null) {
        this.process.kill("SIGTERM");
      }
    } catch { /* ignore */ }
    await delay(150);
    try {
      if (this.process && this.process.exitCode === null) {
        process.kill(-this.process.pid, "SIGKILL");
        this.process.kill("SIGKILL");
      }
    } catch { /* ignore */ }
    if (this.profileDir) {
      try { rmSync(this.profileDir, { recursive: true, force: true }); } catch { /* ignore */ }
    }
  }
}

async function findFreePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const port = server.address().port;
      server.close(() => resolve(port));
    });
  });
}

function writeFirefoxProfilePrefs(profileDir) {
  const prefs = [
    "user_pref(\"browser.shell.checkDefaultBrowser\", false);",
    "user_pref(\"browser.startup.homepage_override.mstone\", \"ignore\");",
    "user_pref(\"datareporting.policy.dataSubmissionEnabled\", false);",
    "user_pref(\"datareporting.healthreport.uploadEnabled\", false);",
    "user_pref(\"toolkit.telemetry.enabled\", false);",
    "user_pref(\"app.update.enabled\", false);",
    "user_pref(\"browser.sessionstore.resume_from_crash\", false);",
    "user_pref(\"browser.startup.page\", 0);",
    "user_pref(\"network.dns.disablePrefetch\", true);",
    "user_pref(\"dom.webnotifications.enabled\", false);",
    "user_pref(\"signon.rememberSignons\", false);",
  ].join("\n");
  try {
    writeFileSync(join(profileDir, "user.js"), prefs, "utf8");
  } catch { /* best effort */ }
}

/* ------------------------------------------------------------------ */
/* Page inspection (fixed routines, no caller supplied JavaScript)     */
/* ------------------------------------------------------------------ */

const ELEMENT_BUILDER = `
(() => {
  const isVisible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  };
  const visibleText = (el) => {
    const raw = (el.innerText || el.textContent || '').replace(/\\s+/g, ' ').trim();
    return raw.length > 160 ? raw.slice(0, 157) + '...' : raw;
  };
  const labelFor = (el) => {
    if (el.id) {
      const lbl = document.querySelector('label[for="' + CSS.escape(el.id) + '"]');
      if (lbl && lbl.innerText) return lbl.innerText.replace(/\\s+/g, ' ').trim();
    }
    const wrapping = el.closest('label');
    if (wrapping && wrapping.innerText) return wrapping.innerText.replace(/\\s+/g, ' ').trim();
    return '';
  };
  const roleOf = (el) => {
    const explicit = el.getAttribute && el.getAttribute('role');
    if (explicit) return explicit.split(' ')[0];
    const tag = el.tagName.toLowerCase();
    const type = ((el.getAttribute && el.getAttribute('type')) || '').toLowerCase();
    if (tag === 'a') return el.hasAttribute('href') ? 'link' : 'generic';
    if (tag === 'button') return 'button';
    if (tag === 'select') return 'combobox';
    if (tag === 'textarea') return 'textbox';
    if (tag === 'option') return 'option';
    if (tag === 'img') return 'img';
    if (tag === 'nav') return 'navigation';
    if (tag === 'main') return 'main';
    if (tag === 'header') return 'banner';
    if (tag === 'footer') return 'contentinfo';
    if (tag === 'aside') return 'complementary';
    if (tag === 'dialog') return 'dialog';
    if (tag === 'menu') return 'menu';
    if (tag === 'form') return 'form';
    if (tag === 'ul' || tag === 'ol') return 'list';
    if (tag === 'li') return 'listitem';
    if (tag === 'h1' || tag === 'h2' || tag === 'h3' || tag === 'h4' || tag === 'h5' || tag === 'h6') return 'heading';
    if (tag === 'input') {
      if (type === 'checkbox') return 'checkbox';
      if (type === 'radio') return 'radio';
      if (type === 'button' || type === 'submit' || type === 'reset') return 'button';
      if (type === 'range') return 'slider';
      if (type === 'number') return 'spinbutton';
      if (type === 'search') return 'searchbox';
      if (type === 'hidden') return 'none';
      return 'textbox';
    }
    if (tag === 'summary') return 'button';
    return 'generic';
  };
  const nameOf = (el) => {
    const aria = el.getAttribute && el.getAttribute('aria-label');
    if (aria && aria.trim()) return aria.trim();
    const labelledby = el.getAttribute && el.getAttribute('aria-labelledby');
    if (labelledby) {
      const parts = labelledby.split(/\\s+/).map((id) => {
        const n = document.getElementById(id);
        return n ? (n.innerText || n.textContent || '').trim() : '';
      }).filter(Boolean);
      if (parts.length) return parts.join(' ');
    }
    const tag = el.tagName.toLowerCase();
    if (tag === 'img') {
      const alt = el.getAttribute('alt');
      if (alt) return alt.trim();
    }
    if (tag === 'input' || tag === 'textarea' || tag === 'select') {
      const lbl = labelFor(el);
      if (lbl) return lbl;
      const ph = el.getAttribute('placeholder');
      if (ph) return ph.trim();
      const title = el.getAttribute('title');
      if (title) return title.trim();
      const idn = el.getAttribute('name');
      if (idn) return idn.trim();
      return '';
    }
    const text = visibleText(el);
    if (text) return text;
    const title = el.getAttribute && el.getAttribute('title');
    return title ? title.trim() : '';
  };
  const INTERACTIVE = new Set([
    'button','link','textbox','searchbox','checkbox','radio','combobox','select','option',
    'tab','menuitem','switch','slider','spinbutton','listbox','dialog','summary',
  ]);
  const MAX_ELEMENTS = 400;
  const elements = [];
  let index = 0;

  // Support open shadow roots traversal
  const collectElements = (root = document, list = []) => {
    const children = root.querySelectorAll('*');
    for (let i = 0; i < children.length; i++) {
      const el = children[i];
      list.push(el);
      if (el.shadowRoot && el.shadowRoot.mode === 'open') {
        collectElements(el.shadowRoot, list);
      }
    }
    return list;
  };

  const all = collectElements();
  for (let domIndex = 0; domIndex < all.length; domIndex++) {
    const el = all[domIndex];
    if (elements.length >= MAX_ELEMENTS) break;
    const role = roleOf(el);
    const tag = el.tagName.toLowerCase();
    const isHeading = /^h[1-6]$/.test(tag);
    if (!isHeading && role === 'generic') continue;
    if (!isVisible(el)) continue;
    const rect = el.getBoundingClientRect();
    const name = nameOf(el);
    const inViewport = rect.top < window.innerHeight && rect.bottom > 0 && rect.left < window.innerWidth && rect.right > 0;
    const clipped = rect.top < 0 || rect.left < 0 || rect.bottom > window.innerHeight || rect.right > window.innerWidth;
    const lbl = labelFor(el);

    elements.push({
      ref: 'e' + (++index),
      role,
      name: name,
      tag: tag,
      type: el.getAttribute ? (el.getAttribute('type') || null) : null,
      value: ('value' in el && typeof el.value === 'string') ? (el.type === 'password' ? '[REDACTED]' : el.value) : null,
      disabled: !!el.disabled,
      checked: !!el.checked,
      href: tag === 'a' ? (el.getAttribute('href') || null) : null,
      placeholder: el.getAttribute ? (el.getAttribute('placeholder') || null) : null,
      text: role === 'heading' || tag === 'p' || tag === 'li' ? visibleText(el) : '',
      x: Math.round(rect.left), y: Math.round(rect.top),
      width: Math.round(rect.width), height: Math.round(rect.height),
      interactive: INTERACTIVE.has(role),
      headingLevel: isHeading ? Number(tag.slice(1)) : null,
      in_viewport: inViewport,
      clipped: clipped,
      labels: lbl ? [lbl] : [],
      domIndex: domIndex,
    });
  }
  return elements;
})()
`;

const SNAPSHOT_SOURCE = `
(() => {
  const elements = ${ELEMENT_BUILDER};
  const docTitle = document.title || '';
  const snapId = 'snap_' + Date.now() + '_' + Math.random().toString(36).slice(2, 8);
  const now = Date.now();
  const headings = elements.filter((e) => e.headingLevel).map((e) => ({ ref: e.ref, level: e.headingLevel, text: e.text || e.name }));
  const links = elements.filter((e) => e.tag === 'a' && e.href).map((e) => ({ ref: e.ref, href: e.href, text: e.name || e.text }));
  const landmarks = elements.filter((e) => ['banner', 'navigation', 'main', 'contentinfo', 'complementary', 'search', 'form'].includes(e.role)).map((e) => ({
    ref: e.ref,
    role: e.role,
    tag: e.tag,
    name: e.name || e.text || ''
  }));
  const dialogs = elements.filter((e) => e.role === 'dialog' || e.role === 'alertdialog' || e.tag === 'dialog').map((e) => ({
    ref: e.ref,
    role: e.role,
    name: e.name || e.text || '',
    open: true
  }));
  const menus = elements.filter((e) => ['menu', 'menubar', 'menuitem'].includes(e.role)).map((e) => ({
    ref: e.ref,
    role: e.role,
    name: e.name || e.text || ''
  }));
  const forms = [...document.querySelectorAll('form')].map((f, i) => ({
    ref: 'f' + (i + 1),
    action: f.getAttribute('action') || null,
    method: (f.getAttribute('method') || 'get').toLowerCase(),
    field_count: f.elements ? f.elements.length : 0
  }));
  const anchors = [...document.querySelectorAll('a[href]')].map((a) => {
    const href = a.getAttribute('href') || '';
    let resolved = '';
    try { resolved = new URL(href, document.baseURI).href; } catch (e) { resolved = href; }
    return { href, resolved, text: (a.innerText || '').replace(/\\s+/g,' ').trim().slice(0, 120) };
  });
  const images = [...document.querySelectorAll('img')].map((i) => ({
    src: i.getAttribute('src') || null,
    alt: i.getAttribute('alt') || null,
    complete: i.complete,
    naturalWidth: i.naturalWidth,
    naturalHeight: i.naturalHeight,
  }));
  const failed_images = images.filter((img) => img.complete && img.naturalWidth === 0).map((img) => ({
    src: img.src,
    alt: img.alt
  }));
  const unnamed_interactive = elements.filter((e) => e.interactive && !e.name.trim()).map((e) => ({
    ref: e.ref,
    role: e.role,
    tag: e.tag
  }));

  let hasClosedShadowRoots = false;
  const crossOriginFrames = [];
  const frames = document.querySelectorAll('iframe, frame');
  for (let i = 0; i < frames.length; i++) {
    try {
      if (!frames[i].contentDocument) crossOriginFrames.push(frames[i].src || 'unknown');
    } catch {
      crossOriginFrames.push(frames[i].src || 'cross-origin');
    }
  }

  const scripts = [...document.querySelectorAll('script[src]')].map((s) => s.getAttribute('src'));
  const rawText = (document.body && document.body.innerText) || '';
  const bodyText = rawText.replace(/\\n{3,}/g, '\\n\\n').trim().slice(0, 4000);
  return JSON.stringify({
    snapshot_id: snapId,
    timestamp: now,
    url: location.href,
    title: docTitle,
    readyState: document.readyState,
    elements,
    headings,
    landmarks,
    dialogs,
    menus,
    links,
    forms,
    anchors,
    images,
    failed_images,
    unnamed_interactive,
    scripts,
    body_text: bodyText,
    viewport: { width: window.innerWidth, height: window.innerHeight },
    scroll: { x: window.scrollX, y: window.scrollY, width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight },
    page_dimensions: { width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight },
    overflowX: document.documentElement.scrollWidth > window.innerWidth + 1,
    has_closed_shadow_roots: hasClosedShadowRoots,
    cross_origin_frames: crossOriginFrames,
  });
})()
`;

function resolveScript(spec) {
  const json = JSON.stringify(spec);
  return `(() => {
    const spec = ${json};
    const collectElements = (root = document, list = []) => {
      const kids = root.querySelectorAll('*');
      for (let i = 0; i < kids.length; i++) {
        const el = kids[i];
        list.push(el);
        if (el.shadowRoot && el.shadowRoot.mode === 'open') {
          collectElements(el.shadowRoot, list);
        }
      }
      return list;
    };
    const all = collectElements();
    const textOf = (el) => (el.innerText || el.textContent || '').replace(/\\s+/g,' ').trim();

    let chosen = null;
    let matchCount = 0;
    let reason = 'no visible element matched the requested target';
    let isStale = false;
    let isAmbiguous = false;

    // 1. Coordinates targeting
    if (spec.x !== undefined && spec.y !== undefined && spec.x !== null && spec.y !== null) {
      if (spec.x < 0 || spec.x > window.innerWidth || spec.y < 0 || spec.y > window.innerHeight) {
        return JSON.stringify({ found: false, reason: 'Coordinates (' + spec.x + ', ' + spec.y + ') are outside the viewport (' + window.innerWidth + 'x' + window.innerHeight + ')' });
      }
      chosen = document.elementFromPoint(spec.x, spec.y);
      matchCount = chosen ? 1 : 0;
      if (!chosen) reason = 'No element found at coordinates (' + spec.x + ', ' + spec.y + ')';
    } else if (spec.ref) {
      // 2. Direct snapshot reference
      const elements = ${ELEMENT_BUILDER};
      const byRef = (ref) => {
        const index = Number(String(ref).replace(/^e/, ''));
        if (!Number.isFinite(index) || index < 1 || index > elements.length) return null;
        const candidate = elements[index - 1];
        return candidate ? all[candidate.domIndex] || null : null;
      };
      chosen = byRef(spec.ref);
      if (!chosen) {
        isStale = true;
        reason = "stale_element_reference: reference '" + spec.ref + "' is stale or no longer exists in DOM";
      }
      matchCount = chosen ? 1 : 0;
    } else if (spec.role || spec.name || spec.text) {
      // 3. Accessible role / name / text locator
      const elements = ${ELEMENT_BUILDER};
      const matches = elements.filter((candidate) => {
        if (spec.role && candidate.role.toLowerCase() !== String(spec.role).toLowerCase()) return false;
        if (spec.name) {
          const needle = String(spec.name).toLowerCase();
          const haystack = ((candidate.name || '') + ' ' + (candidate.text || '')).toLowerCase();
          if (!haystack.includes(needle)) return false;
        }
        if (spec.text) {
          const needle = String(spec.text).toLowerCase();
          if (!((candidate.text || '') + ' ' + (candidate.name || '')).toLowerCase().includes(needle)) return false;
        }
        if (spec.tag && candidate.tag !== String(spec.tag).toLowerCase()) return false;
        return true;
      });
      matchCount = matches.length;
      if (matchCount > 1 && spec.nth === undefined && !spec.ref) {
        isAmbiguous = true;
      }
      const candidate = spec.nth !== undefined && spec.nth !== null ? matches[spec.nth] : matches[0];
      chosen = candidate ? all[candidate.domIndex] || null : null;
      if (!chosen && !matchCount) {
        reason = 'no visible element matched the requested role/name';
      }
    } else if (spec.testId) {
      // 4. Test ID
      const matches = all.filter((el) => {
        const tid = el.getAttribute ? (el.getAttribute('data-testid') || el.getAttribute('data-test-id')) : null;
        return tid === spec.testId;
      });
      matchCount = matches.length;
      chosen = spec.nth !== undefined && spec.nth !== null ? matches[spec.nth] : matches[0];
      if (!chosen) reason = "no element matched testId '" + spec.testId + "'";
    } else if (spec.css) {
      // 5. CSS selector
      let matches = [];
      try { matches = Array.from(document.querySelectorAll(spec.css)); } catch (e) { matches = []; }
      const visible = matches.filter((el) => {
        const style = window.getComputedStyle(el);
        if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
        const r = el.getBoundingClientRect();
        return r.width > 0 && r.height > 0;
      });
      matchCount = visible.length;
      chosen = spec.nth !== undefined && spec.nth !== null ? visible[spec.nth] : visible[0];
      if (!chosen) reason = "no visible element matched the CSS selector '" + spec.css + "'";
    }

    if (!chosen) return JSON.stringify({ found: false, stale: isStale, reason: reason, matches: matchCount });

    chosen.scrollIntoView({ block: 'center', inline: 'center' });
    const rect = chosen.getBoundingClientRect();
    const tag = chosen.tagName.toLowerCase();
    const type = (chosen.getAttribute('type') || '').toLowerCase();
    let role = chosen.getAttribute('role') || tag;
    if (!chosen.getAttribute('role')) {
      if (tag === 'a') role = chosen.hasAttribute('href') ? 'link' : 'generic';
      else if (tag === 'button') role = 'button';
      else if (tag === 'select') role = 'combobox';
      else if (tag === 'textarea') role = 'textbox';
      else if (tag === 'input') {
        if (type === 'checkbox') role = 'checkbox';
        else if (type === 'radio') role = 'radio';
        else if (type === 'button' || type === 'submit' || type === 'reset') role = 'button';
        else if (type === 'search') role = 'searchbox';
        else if (type === 'number') role = 'spinbutton';
        else if (type === 'range') role = 'slider';
        else if (type !== 'hidden') role = 'textbox';
      }
    }
    return JSON.stringify({
      found: true,
      x: Math.round(rect.left + rect.width / 2),
      y: Math.round(rect.top + rect.height / 2),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
      tag: tag,
      role: role,
      name: (chosen.getAttribute('aria-label') || textOf(chosen) || '').slice(0, 160),
      disabled: !!chosen.disabled,
      value: ('value' in chosen && typeof chosen.value === 'string') ? (chosen.type === 'password' ? '[REDACTED]' : chosen.value) : null,
      matches: matchCount,
      ambiguous: isAmbiguous,
    });
  })()`;
}

function bidiValueToJs(v) {
  if (!v) return null;
  if (v.type === "string" || v.type === "number" || v.type === "boolean") return v.value;
  if (v.type === "null") return null;
  if (v.type === "undefined") return undefined;
  if (v.type === "array" && Array.isArray(v.value)) return v.value.map(bidiValueToJs);
  if (v.type === "object" && Array.isArray(v.value)) {
    const o = {};
    for (const [k, val] of v.value) o[k] = bidiValueToJs(val);
    return o;
  }
  return v.value !== undefined ? v.value : null;
}

async function evalInPage(session, source, awaitPromise = false, explicitContext) {
  if (session.adapter === "cdp") {
    const res = await session.send("Runtime.evaluate", {
      expression: source,
      returnByValue: true,
      awaitPromise: true,
    });
    if (res.exceptionDetails) {
      throw new BiDiError(
        "script_error",
        `Page evaluation failed: ${res.exceptionDetails.text || "unknown"}`
      );
    }
    const val = res.result ? res.result.value : null;
    if (typeof val === "string") {
      try { return JSON.parse(val); } catch { return val; }
    }
    return val;
  } else {
    const context = explicitContext || (await session.context());
    const result = await session.client.send("script.evaluate", {
      expression: source,
      target: { context },
      awaitPromise,
    });
    if (result.type === "exception") {
      throw new BiDiError("script_error", `Page evaluation failed: ${result.exceptionDetails ? result.exceptionDetails.text : "unknown"}`);
    }
    const value = result.result ? result.result.value : null;
    const jsVal = bidiValueToJs(result.result);
    if (typeof jsVal === "string") {
      try {
        return JSON.parse(jsVal);
      } catch {
        return jsVal;
      }
    }
    return jsVal;
  }
}

async function pageInfo(session) {
  const info = await evalInPage(
    session,
    `(() => JSON.stringify({ url: location.href, title: document.title, readyState: document.readyState }))()`
  );
  return info || { url: null, title: null, readyState: null };
}

/* ------------------------------------------------------------------ */
/* Keyboard helpers                                                    */
/* ------------------------------------------------------------------ */

const WEBDRIVER_KEYS = {
  enter: "\uE006",
  return: "\uE006",
  tab: "\uE004",
  backspace: "\uE003",
  delete: "\uE017",
  escape: "\uE00C",
  esc: "\uE00C",
  space: "\uE00D",
  arrowleft: "\uE012",
  arrowright: "\uE014",
  arrowup: "\uE013",
  arrowdown: "\uE015",
  home: "\uE011",
  end: "\uE010",
  pageup: "\uE00E",
  pagedown: "\uE00F",
  shift: "\uE008",
  control: "\uE009",
  ctrl: "\uE009",
  alt: "\uE00A",
  meta: "\uE03D",
};

const segmenter = typeof Intl !== "undefined" && Intl.Segmenter
  ? new Intl.Segmenter(undefined, { granularity: "grapheme" })
  : null;

function graphemes(text) {
  if (!segmenter) return Array.from(text);
  return Array.from(segmenter.segment(text), (s) => s.segment);
}

function typeActions(value) {
  const actions = [];
  for (const g of graphemes(String(value))) {
    actions.push({ type: "keyDown", value: g });
    actions.push({ type: "keyUp", value: g });
  }
  return actions;
}

function keyValue(name) {
  const key = String(name || "").trim();
  if (!key) return null;
  const lowered = key.toLowerCase();
  if (WEBDRIVER_KEYS[lowered]) return WEBDRIVER_KEYS[lowered];
  const graph = graphemes(key);
  if (graph.length === 1) return graph[0];
  return null;
}

const sessions = new Map();
let sessionCounter = 0;

/* ------------------------------------------------------------------ */
/* Operation dispatch                                                  */
/* ------------------------------------------------------------------ */

const OPS = {
  async capabilities() {
    const discovery = await discoverBrowsers();
    return {
      node_version: process.version,
      platform: process.platform,
      arch: process.arch,
      browsers: discovery.browsers,
      default_browser_id: discovery.defaultBrowserId,
      driver: {
        name: "provider-neutral",
        version: "2.0",
        external_driver_required: false,
      },
    };
  },

  async "session.create"(params) {
    const discovery = await discoverBrowsers();
    const browsers = discovery.browsers;
    const requested = (params.browser || "auto").trim().toLowerCase();

    let chosen = null;
    if (requested === "auto" || !requested) {
      if (discovery.defaultBrowserId) {
        chosen = browsers.find((b) => b.id === discovery.defaultBrowserId && b.available && b.automatable);
        if (!chosen) {
          const available = browsers
            .filter((b) => b.available && b.automatable)
            .map((b) => `${b.display_name} (${b.id})`)
            .join(", ") || "none";
          throw new BiDiError(
            "default_browser_not_automatable",
            `The system default browser '${discovery.defaultBrowserId}' is not installed or cannot be automated. Available supported alternatives: ${available}. Kyro will not silently substitute another browser.`
          );
        }
      } else {
        const available = browsers
          .filter((b) => b.available && b.automatable)
          .map((b) => `${b.display_name} (${b.id})`)
          .join(", ") || "none";
        if (available === "none") {
          throw new BiDiError(
            "no_browser_available",
            "No automatable browser is currently installed. Install Mozilla Firefox ('sudo apt install firefox-esr' or 'sudo dnf install firefox') or Chromium ('sudo apt install chromium-browser') and retry."
          );
        } else {
          throw new BiDiError(
            "no_default_browser",
            `The system default browser could not be determined. Available supported alternatives: ${available}. Please specify a browser explicitly or configure a default in Kyro settings. Kyro will not silently substitute another browser.`
          );
        }
      }
    } else {
      chosen = browsers.find(
        (b) => b.id === requested || b.name === requested || b.display_name.toLowerCase() === requested
      );
      if (!chosen) {
        const available = browsers
          .filter((b) => b.available && b.automatable)
          .map((b) => `${b.display_name} (${b.id})`)
          .join(", ") || "none";
        throw new BiDiError(
          "browser_unsupported",
          `Kyro does not support the browser '${requested}'. Available automatable browsers on this machine: ${available}. Kyro will not silently substitute another browser.`
        );
      }
      if (!chosen.available) {
        const available = browsers
          .filter((b) => b.available && b.automatable)
          .map((b) => `${b.display_name} (${b.id})`)
          .join(", ") || "none";
        throw new BiDiError(
          "browser_unavailable",
          `Requested browser '${chosen.display_name}' is not installed (${chosen.unavailable_reason || "executable missing"}). Available alternatives: ${available}. Kyro will not silently substitute another browser.`
        );
      }
      if (!chosen.automatable) {
        const available = browsers
          .filter((b) => b.available && b.automatable)
          .map((b) => `${b.display_name} (${b.id})`)
          .join(", ") || "none";
        throw new BiDiError(
          "browser_not_automatable",
          `Requested browser '${chosen.display_name}' is installed but cannot be automated by Kyro (${chosen.unavailable_reason || "unsupported automation adapter"}). Available alternatives: ${available}. Kyro will not silently substitute another browser.`
        );
      }
    }

    const id = `bs_${++sessionCounter}_${Date.now().toString(36)}`;
    const sessionOpts = {
      browserId: chosen.id,
      browserDisplayName: chosen.display_name,
      browserPath: chosen.path,
      headless: params.headless !== false,
      viewport: params.viewport || { width: 1280, height: 800 },
    };

    const session = chosen.family === "chromium"
      ? new ChromiumSession(id, sessionOpts)
      : new FirefoxSession(id, sessionOpts);

    await session.start();
    session.browserVersion = chosen.version;
    sessions.set(id, session);

    return {
      session_id: id,
      browser: chosen.id,
      browser_path: chosen.path,
      browser_version: chosen.version,
      headless: session.opts.headless,
      profile_isolated: true,
      driver: chosen.adapter,
    };
  },

  async "session.close"(params) {
    const session = sessions.get(params.session_id);
    if (!session) return { closed: false, reason: "unknown session" };
    await session.close();
    sessions.delete(params.session_id);
    return { closed: true };
  },

  async "page.navigate"(params) {
    const session = requireSession(params.session_id);
    const started = Date.now();
    const url = params.url;
    const timeout = params.timeout_ms || DEFAULT_NAV_TIMEOUT_MS;

    if (session.adapter === "cdp") {
      try {
        const navPromise = session.send("Page.navigate", { url }, timeout + 5000);
        const navResult = await navPromise;
        const readyDeadline = Date.now() + Math.min(timeout, 10000);
        while (Date.now() < readyDeadline) {
          const state = await session.send("Runtime.evaluate", {
            expression: "document.readyState",
            returnByValue: true,
          }).catch(() => null);
          if (state && state.result && (state.result.value === "complete" || state.result.value === "interactive")) {
            break;
          }
          await delay(100);
        }
        const info = await pageInfo(session);
        return {
          url: info.url || url,
          title: info.title,
          navigation: navResult ? navResult.frameId : null,
          navigation_status: "complete",
          ready_state: info.readyState,
          duration_ms: Date.now() - started,
        };
      } catch (e) {
        if (e instanceof BiDiError && e.code === "timeout") {
          throw new BiDiError("navigation_timeout", `Navigation to ${url} timed out after ${timeout}ms`);
        }
        throw new BiDiError("navigation_failed", `Navigation to ${url} failed: ${e.message}`);
      }
    } else {
      const context = await session.context();
      let result;
      try {
        result = await session.client.send(
          "browsingContext.navigate",
          { context, url, wait: "complete" },
          timeout + 5000
        );
      } catch (e) {
        if (e instanceof BiDiError && e.code === "timeout") {
          throw new BiDiError("navigation_timeout", `Navigation to ${url} timed out after ${timeout}ms`);
        }
        throw new BiDiError("navigation_failed", `Navigation to ${url} failed: ${e.message}`);
      }
      const info = await pageInfo(session);
      return {
        url: result.url || info.url,
        title: info.title,
        navigation: result.navigation || null,
        navigation_status: "complete",
        ready_state: info.readyState,
        duration_ms: Date.now() - started,
      };
    }
  },

  async "page.info"(params) {
    const session = requireSession(params.session_id);
    return pageInfo(session);
  },

  async "page.snapshot"(params) {
    const session = requireSession(params.session_id);
    const snap = await evalInPage(session, SNAPSHOT_SOURCE);
    if (!snap) throw new BiDiError("snapshot_failed", "The page snapshot could not be produced");
    session.currentSnapshotId = snap.snapshot_id;
    snap.console_errors = (session.consoleEntries || []).filter((e) => e.level === "error");
    snap.failed_requests = session.networkFailed || [];
    return snap;
  },

  async "page.inspect_element"(params) {
    const session = requireSession(params.session_id);
    const target = normaliseTarget(params.target);
    const timeout = params.timeout_ms || DEFAULT_ACTION_TIMEOUT_MS;
    const deadline = Date.now() + timeout;

    let located = null;
    let lastReason = "no visible element matched the requested target";
    while (Date.now() < deadline) {
      located = await evalInPage(session, resolveScript(target));
      if (located && located.found) break;
      if (located && located.reason) lastReason = located.reason;
      await delay(150);
    }
    if (!located || !located.found) {
      const code = located && located.stale ? "stale_element_reference" : "element_not_found";
      throw new BiDiError(code, `Could not inspect element: ${lastReason}`);
    }

    const script = `(() => {
      const spec = ${JSON.stringify(target)};
      const collectElements = (root = document, list = []) => {
        const kids = root.querySelectorAll('*');
        for (let i = 0; i < kids.length; i++) {
          const el = kids[i];
          list.push(el);
          if (el.shadowRoot && el.shadowRoot.mode === 'open') {
            collectElements(el.shadowRoot, list);
          }
        }
        return list;
      };
      const all = collectElements();
      let chosen = null;
      if (spec.x !== undefined && spec.y !== undefined) {
        chosen = document.elementFromPoint(spec.x, spec.y);
      } else if (spec.ref) {
        const index = Number(String(spec.ref).replace(/^e/, ''));
        const elements = ${ELEMENT_BUILDER};
        if (Number.isFinite(index) && index >= 1 && index <= elements.length) {
          const candidate = elements[index - 1];
          chosen = candidate ? all[candidate.domIndex] : null;
        }
      } else if (spec.css) {
        chosen = document.querySelector(spec.css);
      } else {
        const elements = ${ELEMENT_BUILDER};
        const candidate = elements.find(c => {
          if (spec.role && c.role.toLowerCase() !== String(spec.role).toLowerCase()) return false;
          if (spec.name && !((c.name || '') + ' ' + (c.text || '')).toLowerCase().includes(String(spec.name).toLowerCase())) return false;
          return true;
        });
        chosen = candidate ? all[candidate.domIndex] : null;
      }
      if (!chosen) return null;
      const rect = chosen.getBoundingClientRect();
      const style = window.getComputedStyle(chosen);
      const attrs = {};
      for (let i = 0; i < chosen.attributes.length; i++) {
        const a = chosen.attributes[i];
        if (a.name.toLowerCase().includes('password') || a.name.toLowerCase().includes('secret')) continue;
        attrs[a.name] = a.value;
      }
      return JSON.stringify({
        tag: chosen.tagName.toLowerCase(),
        role: chosen.getAttribute('role') || chosen.tagName.toLowerCase(),
        name: (chosen.getAttribute('aria-label') || chosen.innerText || '').slice(0, 160),
        dom_context: {
          tag: chosen.tagName.toLowerCase(),
          outer_html_snippet: chosen.outerHTML.slice(0, 300),
          parent_tag: chosen.parentElement ? chosen.parentElement.tagName.toLowerCase() : null,
          parent_class: chosen.parentElement ? chosen.parentElement.className : null,
          children_count: chosen.children ? chosen.children.length : 0,
        },
        attributes: attrs,
        computed_styles: {
          display: style.display,
          visibility: style.visibility,
          opacity: style.opacity,
          position: style.position,
          z_index: style.zIndex,
          color: style.color,
          background_color: style.backgroundColor,
          font_size: style.fontSize,
          box_sizing: style.boxSizing,
          width: style.width,
          height: style.height,
          overflow: style.overflow,
        },
        bounding_box: {
          x: Math.round(rect.left),
          y: Math.round(rect.top),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
          in_viewport: rect.top < window.innerHeight && rect.bottom > 0 && rect.left < window.innerWidth && rect.right > 0,
          clipped: rect.top < 0 || rect.left < 0 || rect.bottom > window.innerHeight || rect.right > window.innerWidth,
        },
        interactable: !chosen.disabled && style.display !== 'none' && style.visibility !== 'hidden',
        disabled: !!chosen.disabled,
      });
    })()`;

    const details = await evalInPage(session, script);
    let cropped_png = null;
    if (session.adapter === "cdp" && located.width > 0 && located.height > 0) {
      try {
        const clip = {
          x: Math.max(0, located.x - located.width / 2),
          y: Math.max(0, located.y - located.height / 2),
          width: Math.max(1, located.width),
          height: Math.max(1, located.height),
          scale: 1,
        };
        const shot = await session.send("Page.captureScreenshot", { format: "png", clip });
        if (shot && shot.data) cropped_png = shot.data;
      } catch { /* best effort */ }
    }

    return {
      element_ref: target.ref || null,
      ...details,
      cropped_screenshot: cropped_png,
    };
  },

  async "page.click"(params) {
    const session = requireSession(params.session_id);
    const target = normaliseTarget(params.target);
    const timeout = params.timeout_ms || DEFAULT_ACTION_TIMEOUT_MS;
    const started = Date.now();
    const before = await pageInfo(session);
    const activeBefore = await evalInPage(session, `(() => { const el = document.activeElement; return el ? (el.tagName + (el.id ? '#' + el.id : '')) : null; })()`);

    let located = null;
    const deadline = Date.now() + timeout;
    let lastReason = "no visible element matched the requested target";
    while (Date.now() < deadline) {
      located = await evalInPage(session, resolveScript(target));
      if (located && located.found) break;
      if (located && located.reason) lastReason = located.reason;
      await delay(150);
    }
    if (!located || !located.found) {
      const code = located && located.stale ? "stale_element_reference" : "element_not_found";
      throw new BiDiError(code, `Could not find a clickable element: ${lastReason}`);
    }
    if (located.disabled) {
      throw new BiDiError("element_disabled", `The element '${located.name || located.tag}' is disabled and cannot be clicked`);
    }

    if (session.adapter === "cdp") {
      const x = Math.round(located.x);
      const y = Math.round(located.y);
      await session.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
      await session.send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", clickCount: 1, x, y });
      await session.send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", clickCount: 1, x, y });
    } else {
      await session.client.send("input.performActions", {
        context: await session.context(),
        actions: [
          {
            type: "pointer",
            id: "mouse",
            parameters: { pointerType: "mouse" },
            actions: [
              { type: "pointerMove", x: located.x, y: located.y, duration: 0 },
              { type: "pointerDown", button: 0 },
              { type: "pointerUp", button: 0 },
            ],
          },
        ],
      });
    }

    await delay(params.settle_ms === undefined ? 250 : params.settle_ms);
    const after = await pageInfo(session);
    const activeAfter = await evalInPage(session, `(() => { const el = document.activeElement; return el ? (el.tagName + (el.id ? '#' + el.id : '')) : null; })()`);

    const navigated = before.url !== after.url;
    const titleChanged = before.title !== after.title;
    const focusChanged = activeBefore !== activeAfter;
    const stateChanged = navigated || titleChanged || focusChanged;
    const verificationStatus = stateChanged ? "succeeded" : "not_verified";

    return {
      status: "clicked",
      element: { name: located.name, role: located.role, tag: located.tag, x: located.x, y: located.y },
      matches: located.matches,
      ambiguous: !!located.ambiguous,
      url: after.url,
      title: after.title,
      url_before: before.url,
      navigated,
      verified_state_change: stateChanged,
      verification_status: verificationStatus,
      duration_ms: Date.now() - started,
    };
  },

  async "page.fill"(params) {
    const session = requireSession(params.session_id);
    const target = normaliseTarget(params.target);
    const timeout = params.timeout_ms || DEFAULT_ACTION_TIMEOUT_MS;
    const value = params.value === undefined || params.value === null ? "" : String(params.value);
    const started = Date.now();

    let located = null;
    const deadline = Date.now() + timeout;
    let lastReason = "no visible element matched the requested target";
    while (Date.now() < deadline) {
      located = await evalInPage(session, resolveScript(target));
      if (located && located.found) break;
      if (located && located.reason) lastReason = located.reason;
      await delay(150);
    }
    if (!located || !located.found) {
      const code = located && located.stale ? "stale_element_reference" : "element_not_found";
      throw new BiDiError(code, `Could not find an editable element: ${lastReason}`);
    }
    if (located.disabled) {
      throw new BiDiError("element_disabled", `The field '${located.name || located.tag}' is disabled`);
    }

    if (session.adapter === "cdp") {
      const x = Math.round(located.x);
      const y = Math.round(located.y);
      await session.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
      await session.send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", clickCount: 1, x, y });
      await session.send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", clickCount: 1, x, y });
      await delay(80);

      // Select existing text and delete
      await session.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: "a", code: "KeyA", modifiers: 2 });
      await session.send("Input.dispatchKeyEvent", { type: "keyUp", key: "a", code: "KeyA" });
      await session.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: "Backspace", code: "Backspace" });
      await session.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Backspace", code: "Backspace" });

      if (value) {
        await session.send("Input.insertText", { text: value });
      }
    } else {
      const context = await session.context();
      await session.client.send("input.performActions", {
        context,
        actions: [
          {
            type: "pointer",
            id: "mouse",
            parameters: { pointerType: "mouse" },
            actions: [
              { type: "pointerMove", x: located.x, y: located.y, duration: 0 },
              { type: "pointerDown", button: 0 },
              { type: "pointerUp", button: 0 },
            ],
          },
        ],
      });
      await delay(80);

      const keyActions = [
        { type: "keyDown", value: "a", modifiers: 2 },
        { type: "keyUp", value: "a", modifiers: 2 },
        { type: "keyDown", value: "\uE003" },
        { type: "keyUp", value: "\uE003" },
        ...typeActions(value),
      ];
      await session.client.send("input.performActions", {
        context,
        actions: [{ type: "key", id: "keyboard", actions: keyActions }],
      });
    }

    const after = await evalInPage(
      session,
      `(() => { const el = document.activeElement; return el ? JSON.stringify({ tag: el.tagName, value: ('value' in el ? el.value : null) }) : null; })()`
    );
    const valueAfter = after ? after.value : null;
    const valueChanged = valueAfter === value || (valueAfter !== null && valueAfter !== undefined);

    return {
      status: "filled",
      element: { name: located.name, role: located.role, tag: located.tag },
      value_after: valueAfter,
      verified_state_change: valueChanged,
      verification_status: valueChanged ? "succeeded" : "not_verified",
      duration_ms: Date.now() - started,
    };
  },

  async "page.check_validation"(params) {
    const session = requireSession(params.session_id);
    const target = normaliseTarget(params.target);
    const timeout = params.timeout_ms || DEFAULT_ACTION_TIMEOUT_MS;
    const started = Date.now();

    let located = null;
    const deadline = Date.now() + timeout;
    let lastReason = "no visible element matched the requested target";
    while (Date.now() < deadline) {
      located = await evalInPage(session, resolveScript(target));
      if (located && located.found) break;
      if (located && located.reason) lastReason = located.reason;
      await delay(150);
    }
    if (!located || !located.found) {
      throw new BiDiError("element_not_found", `Could not find the field to validate: ${lastReason}`);
    }

    const probe = await evalInPage(
      session,
      `(() => {
        const spec = ${JSON.stringify(target)};
        const elements = ${ELEMENT_BUILDER};
        const all = Array.from(document.querySelectorAll('*'));
        let el = null;
        for (const candidate of elements) {
          if (spec.role && candidate.role.toLowerCase() !== String(spec.role).toLowerCase()) continue;
          if (spec.name) {
            const needle = String(spec.name).toLowerCase();
            const hay = ((candidate.name || '') + ' ' + (candidate.text || '')).toLowerCase();
            if (!hay.includes(needle)) continue;
          }
          if (spec.tag && candidate.tag !== String(spec.tag).toLowerCase()) continue;
          if (spec.testId) {
            const n = all[candidate.domIndex];
            const tid = n ? (n.getAttribute('data-testid') || n.getAttribute('data-test-id')) : null;
            if (tid !== spec.testId) continue;
          }
          el = all[candidate.domIndex] || null;
          break;
        }
        if (!el) return JSON.stringify({ found: false, reason: 'field not found' });
        const type = (el.getAttribute('type') || 'text').toLowerCase();
        const original = 'value' in el ? el.value : null;
        const labelText = [
          el.getAttribute('aria-label') || '',
          el.getAttribute('placeholder') || '',
          el.getAttribute('name') || '',
          el.id || '',
          el.form && el.form.querySelector('label[for="' + CSS.escape(el.id) + '"]')
            ? el.form.querySelector('label[for="' + CSS.escape(el.id) + '"]').innerText
            : '',
        ].join(' ').toLowerCase();

        let expects = null;
        if (type === 'email' || /e-?mail/.test(labelText)) expects = 'email';
        else if (type === 'url' || /url|website|link/.test(labelText)) expects = 'url';
        else if (type === 'number' || /amount|price|quantity|number|age|count/.test(labelText)) expects = 'number';
        else if (type === 'tel' || /phone|telephone|mobile/.test(labelText)) expects = 'tel';

        let invalid;
        switch (expects) {
          case 'email': invalid = 'not-an-email'; break;
          case 'url': invalid = 'not a url'; break;
          case 'number': invalid = 'not-a-number'; break;
          case 'tel': invalid = 'not a phone number'; break;
          default: invalid = '';
        }
        const form = el.form;
        const novalidate = form ? !!form.noValidate : false;
        const setter = Object.getOwnPropertyDescriptor(
          el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype,
          'value'
        );
        if (setter && setter.set) setter.set.call(el, invalid);
        else el.value = invalid;
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.dispatchEvent(new Event('change', { bubbles: true }));
        let valid = null;
        let message = '';
        try {
          valid = el.checkValidity();
          message = el.validationMessage || '';
        } catch (e) {
          valid = null;
        }
        const required = !!el.required;
        const pattern = el.getAttribute('pattern');
        const minlength = el.getAttribute('minlength');
        const constrainedByType = type === 'email' || type === 'url' || type === 'number' || type === 'tel';
        if (setter && setter.set && original !== null) setter.set.call(el, original);
        else if (original !== null) el.value = original;
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.dispatchEvent(new Event('change', { bubbles: true }));
        return JSON.stringify({
          found: true,
          name: el.getAttribute('name') || el.getAttribute('aria-label') || el.id || 'field',
          type: type,
          required: required,
          pattern: pattern,
          minlength: minlength,
          novalidate: novalidate,
          invalid_value: invalid,
          valid: valid,
          validation_message: message,
          expects: expects,
          constrained_by_type: constrainedByType,
          accepts_invalid: valid === true && expects !== null,
        });
      })()`
    );

    const data = probe || { found: false };
    return {
      status: data.found ? "checked" : "failed",
      field: data,
      duration_ms: Date.now() - started,
    };
  },

  async "page.press"(params) {
    const session = requireSession(params.session_id);
    const key = params.key || "";
    if (!key) throw new BiDiError("invalid_request", "page.press requires a 'key' value");

    if (session.adapter === "cdp") {
      const lower = key.toLowerCase();
      let cdpKey = key;
      let code = "";
      if (lower === "enter" || lower === "return") { cdpKey = "Enter"; code = "Enter"; }
      else if (lower === "tab") { cdpKey = "Tab"; code = "Tab"; }
      else if (lower === "escape" || lower === "esc") { cdpKey = "Escape"; code = "Escape"; }
      else if (lower === "backspace") { cdpKey = "Backspace"; code = "Backspace"; }
      else if (lower === "arrowdown") { cdpKey = "ArrowDown"; code = "ArrowDown"; }
      else if (lower === "arrowup") { cdpKey = "ArrowUp"; code = "ArrowUp"; }
      else if (lower === "arrowleft") { cdpKey = "ArrowLeft"; code = "ArrowLeft"; }
      else if (lower === "arrowright") { cdpKey = "ArrowRight"; code = "ArrowRight"; }

      await session.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: cdpKey, code });
      await session.send("Input.dispatchKeyEvent", { type: "keyUp", key: cdpKey, code });
    } else {
      const value = keyValue(key);
      if (value === null) {
        throw new BiDiError(
          "invalid_request",
          `'${key}' is not a single key. Use a named key (Enter, Tab, Escape, Backspace, ArrowDown, ...) or a single character.`
        );
      }
      await session.client.send("input.performActions", {
        context: await session.context(),
        actions: [
          { type: "key", id: "keyboard", actions: [{ type: "keyDown", value }, { type: "keyUp", value }] },
        ],
      });
    }

    await delay(params.settle_ms === undefined ? 150 : params.settle_ms);
    const after = await pageInfo(session);
    return { status: "pressed", key, url: after.url, title: after.title };
  },

  async "page.screenshot"(params) {
    const session = requireSession(params.session_id);
    let data;

    if (session.adapter === "cdp") {
      const shot = await session.send("Page.captureScreenshot", {
        format: "png",
        captureBeyondViewport: !!params.full_page,
      });
      data = shot.data;
    } else {
      const context = await session.context();
      const shot = await session.client.send("browsingContext.captureScreenshot", {
        context,
        origin: params.full_page ? "document" : "viewport",
      });
      data = shot.data;
    }

    return {
      base64_png: data,
      width: params.width || null,
      height: params.height || null,
      origin: params.full_page ? "document" : "viewport",
    };
  },

  async "page.tabs"(params) {
    const session = requireSession(params.session_id);
    let tabs = [];

    if (session.adapter === "cdp") {
      try {
        const res = await fetch(`http://127.0.0.1:${session.port}/json/list`);
        const targets = await res.json();
        tabs = targets
          .filter((t) => t.type === "page")
          .map((t) => ({
            context_id: t.id,
            url: t.url,
            title: t.title || null,
            parent: null,
          }));
      } catch {
        tabs = [{ context_id: "default", url: "about:blank", title: null, parent: null }];
      }
    } else {
      const tree = await session.client.send("browsingContext.getTree", {});
      tabs = (tree.contexts || []).map((c) => ({
        context_id: c.context,
        url: c.url,
        title: undefined,
        parent: c.parent || null,
      }));
      for (const tab of tabs) {
        try {
          const info = await evalInPage(
            session,
            `(() => JSON.stringify({ title: document.title, url: location.href }))()`,
            false,
            tab.context_id
          );
          tab.title = info ? info.title : null;
          if (info && info.url) tab.url = info.url;
        } catch { /* title optional */ }
      }
    }
    return { tabs };
  },

  async "page.set_viewport"(params) {
    const session = requireSession(params.session_id);
    const width = Math.max(320, Math.min(3840, Number(params.width) || 1280));
    const height = Math.max(240, Math.min(2160, Number(params.height) || 800));

    if (session.adapter === "cdp") {
      await session.send("Emulation.setDeviceMetricsOverride", {
        width,
        height,
        deviceScaleFactor: 1,
        mobile: false,
      });
    } else {
      const context = await session.context();
      await session.client.send("browsingContext.setViewport", { context, viewport: { width, height } });
    }
    return { width, height };
  },

  async "diagnostics.console"(params) {
    const session = requireSession(params.session_id);
    const since = params.since || 0;
    const entries = session.consoleEntries.filter((e) => (e.timestamp || 0) >= since);
    const errors = entries.filter((e) => e.level === "error");
    const warnings = entries.filter((e) => e.level === "warn");
    return { entries, error_count: errors.length, warning_count: warnings.length };
  },

  async "diagnostics.network"(params) {
    const session = requireSession(params.session_id);
    const since = params.since || 0;
    const requests = [...session.networkRequests.values()].map((r) => ({
      request_id: r.request_id,
      url: r.url,
      method: r.method,
      status: r.status,
      mime_type: r.mime_type,
      error: r.error,
      navigation: r.navigation,
    }));
    const failed = session.networkFailed.filter((r) => (r.timestamp || 0) >= since);
    const httpErrors = requests.filter((r) => r.status !== null && r.status >= 400);
    return {
      requests,
      failed,
      http_error_count: httpErrors.length,
      failed_count: failed.length,
    };
  },

  async "diagnostics.reset"(params) {
    const session = requireSession(params.session_id);
    session.consoleEntries = [];
    session.networkRequests.clear();
    session.networkFailed = [];
    return { reset: true };
  },

  async "session.list"() {
    return {
      sessions: [...sessions.values()].map((s) => ({
        session_id: s.id,
        browser: s.browserId,
        browser_version: s.browserVersion,
        headless: s.opts.headless,
        closed: s.closed,
        adapter: s.adapter,
      })),
    };
  },
};

function requireSession(sessionId) {
  const session = sessions.get(sessionId);
  if (!session) throw new BiDiError("unknown_session", `Browser session '${sessionId}' does not exist`);
  if (session.closed) throw new BiDiError("session_closed", `Browser session '${sessionId}' has been closed`);
  return session;
}

function normaliseTarget(target) {
  if (!target || typeof target !== "object") {
    throw new BiDiError("invalid_request", "A 'target' object describing the element is required");
  }
  const out = {};
  for (const key of ["role", "name", "text", "tag", "testId", "css", "nth", "ref", "x", "y", "snapshot_id"]) {
    if (target[key] !== undefined && target[key] !== null) out[key] = target[key];
  }
  if (!out.role && !out.name && !out.text && !out.tag && !out.testId && !out.css && !out.ref && (out.x === undefined || out.y === undefined)) {
    throw new BiDiError("invalid_request", "The target must specify at least one of ref, role, name, text, tag, testId, css, or coordinates (x, y)");
  }
  return out;
}

/* ------------------------------------------------------------------ */
/* stdin/stdout protocol                                               */
/* ------------------------------------------------------------------ */

let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index).trim();
    buffer = buffer.slice(index + 1);
    if (line) handleLine(line);
  }
});
process.stdin.on("end", async () => {
  for (const session of sessions.values()) await session.close();
  process.exit(0);
});

async function handleLine(line) {
  let request;
  try {
    request = JSON.parse(line);
  } catch (e) {
    write({ id: null, ok: false, error: { category: "invalid_json", message: "Request was not valid JSON" } });
    return;
  }
  const id = request.id === undefined ? null : request.id;
  const op = request.op;
  const handler = OPS[op];
  if (!handler) {
    write({ id, ok: false, error: { category: "unknown_operation", message: `Unknown operation '${op}'` } });
    return;
  }
  try {
    const result = await handler(request.params || {});
    write({ id, ok: true, result });
  } catch (e) {
    const category = e instanceof BiDiError ? e.code : "operation_failed";
    write({ id, ok: false, error: { category, message: e.message } });
  }
}

function write(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

process.on("uncaughtException", (e) => {
  write({ id: null, ok: false, error: { category: "worker_crash", message: String(e && e.message ? e.message : e) } });
});

process.on("SIGTERM", async () => {
  for (const session of sessions.values()) await session.close();
  process.exit(0);
});
