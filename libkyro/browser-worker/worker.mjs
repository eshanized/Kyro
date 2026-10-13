#!/usr/bin/env node
/**
 * Kyro browser automation worker.
 *
 * The worker drives the *installed* Firefox through the WebDriver BiDi
 * protocol that Firefox exposes natively via `--remote-debugging-port`.
 * No external driver (geckodriver) and no npm dependency is required, so the
 * subsystem works offline and can be packaged with Kyro.
 *
 * Protocol: newline delimited JSON on stdin/stdout.
 *   request : {"id":1,"op":"page.navigate","session_id":"s1","params":{...}}
 *   response: {"id":1,"ok":true,"result":{...}}
 *             {"id":1,"ok":false,"error":{"category":"...","message":"..."}}
 *
 * The worker never evaluates caller supplied JavaScript.  Every operation is
 * a fixed, purpose built routine; the model can only ask for the operations
 * this file implements.
 */

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, existsSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, delimiter } from "node:path";
import { createServer } from "node:net";
import { setTimeout as delay } from "node:timers/promises";

const FIREFOX_CANDIDATES = [
  "/usr/bin/firefox",
  "/usr/local/bin/firefox",
  "/usr/lib/firefox/firefox",
  "/opt/firefox/firefox",
  "/snap/bin/firefox",
  "/usr/bin/firefox-esr",
  "/usr/local/bin/firefox-esr",
  "/Applications/Firefox.app/Contents/MacOS/firefox",
];

const DEFAULT_NAV_TIMEOUT_MS = 30000;
const DEFAULT_ACTION_TIMEOUT_MS = 10000;

/* ------------------------------------------------------------------ */
/* Firefox discovery                                                   */
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

async function probeFirefox(path) {
  return new Promise((resolve) => {
    const child = spawn(path, ["--version"], { stdio: ["ignore", "pipe", "pipe"] });
    let out = "";
    child.stdout.on("data", (d) => (out += d.toString()));
    child.stderr.on("data", (d) => (out += d.toString()));
    const timer = setTimeout(() => {
      try { child.kill("SIGKILL"); } catch { /* ignore */ }
      resolve({ version: null, ok: false });
    }, 10000);
    child.on("error", () => {
      clearTimeout(timer);
      resolve({ version: null, ok: false });
    });
    child.on("close", () => {
      clearTimeout(timer);
      const m = out.match(/(\d+\.\d+(\.\d+)?)/);
      resolve({ version: m ? m[1] : out.trim() || null, ok: true });
    });
  });
}

async function discoverBrowsers() {
  const found = new Map();
  for (const candidate of FIREFOX_CANDIDATES) {
    if (found.has(candidate)) continue;
    if (!existsSync(candidate)) continue;
    const probe = await probeFirefox(candidate);
    if (probe.ok) {
      found.set(candidate, { name: "firefox", path: candidate, version: probe.version, available: true });
    }
  }
  const onPath = findOnPath("firefox");
  if (onPath && !found.has(onPath)) {
    const probe = await probeFirefox(onPath);
    if (probe.ok) {
      found.set(onPath, { name: "firefox", path: onPath, version: probe.version, available: true });
    }
  }
  const esr = findOnPath("firefox-esr");
  if (esr && !found.has(esr)) {
    const probe = await probeFirefox(esr);
    if (probe.ok) {
      found.set(esr, { name: "firefox", path: esr, version: probe.version, available: true });
    }
  }
  return [...found.values()];
}

/* ------------------------------------------------------------------ */
/* Minimal WebDriver BiDi client                                       */
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
/* Browser session                                                     */
/* ------------------------------------------------------------------ */

const CONSOLE_EVENTS = ["log.entryAdded"];
const NETWORK_EVENTS = [
  "network.beforeRequestSent",
  "network.responseCompleted",
  "network.fetchError",
];

class BrowserSession {
  constructor(id, opts) {
    this.id = id;
    this.opts = opts;
    this.process = null;
    this.client = null;
    this.profileDir = null;
    this.port = null;
    this.browserPath = opts.browserPath;
    this.browserVersion = null;
    this.contextId = null;
    this.consoleEntries = [];
    this.networkRequests = new Map();
    this.networkFailed = [];
    this.closed = false;
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

    await writeProfilePrefs(this.profileDir, this.opts);

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
    // Firefox's `log.entryAdded` event carries the level and text at the top
    // level of the event parameters.
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

function writeProfilePrefs(profileDir, opts) {
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
  } catch { /* preferences are best effort */ }
}

/* ------------------------------------------------------------------ */
/* Page inspection (fixed routines, no caller supplied JavaScript)     */
/* ------------------------------------------------------------------ */

/**
 * Builds the ordered list of semantic elements for the current document.
 *
 * The routine is a fixed constant: the caller cannot inject JavaScript.
 * Both the snapshot and the locator resolution use the *same* builder, so a
 * `ref` returned by a snapshot always refers to the same element later.
 */
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
  const all = document.querySelectorAll('*');
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
    elements.push({
      ref: 'e' + (++index),
      role,
      name: name,
      tag: tag,
      type: el.getAttribute ? (el.getAttribute('type') || null) : null,
      value: ('value' in el && typeof el.value === 'string' && el.type !== 'password') ? el.value : null,
      disabled: !!el.disabled,
      checked: !!el.checked,
      href: tag === 'a' ? (el.getAttribute('href') || null) : null,
      placeholder: el.getAttribute ? (el.getAttribute('placeholder') || null) : null,
      text: role === 'heading' || tag === 'p' || tag === 'li' ? visibleText(el) : '',
      x: Math.round(rect.left), y: Math.round(rect.top),
      width: Math.round(rect.width), height: Math.round(rect.height),
      interactive: INTERACTIVE.has(role),
      headingLevel: isHeading ? Number(tag.slice(1)) : null,
      domIndex: domIndex,
    });
  }
  return elements;
})()
`;

/**
 * Collects a semantic snapshot of the current document.
 */
const SNAPSHOT_SOURCE = `
(() => {
  const elements = ${ELEMENT_BUILDER};
  const docTitle = document.title || '';
  const headings = elements.filter((e) => e.headingLevel).map((e) => ({ ref: e.ref, level: e.headingLevel, text: e.text || e.name }));
  const links = elements.filter((e) => e.tag === 'a' && e.href).map((e) => ({ ref: e.ref, href: e.href, text: e.name || e.text }));
  const forms = [...document.querySelectorAll('form')].map((f, i) => ({
    ref: 'f' + (i + 1),
    action: f.getAttribute('action') || null,
    method: (f.getAttribute('method') || 'get').toLowerCase(),
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
  const scripts = [...document.querySelectorAll('script[src]')].map((s) => s.getAttribute('src'));
  const rawText = (document.body && document.body.innerText) || '';
  const bodyText = rawText.replace(/\\n{3,}/g, '\\n\\n').trim().slice(0, 4000);
  return JSON.stringify({
    url: location.href,
    title: docTitle,
    readyState: document.readyState,
    elements,
    headings,
    links,
    forms,
    anchors,
    images,
    scripts,
    body_text: bodyText,
    viewport: { width: window.innerWidth, height: window.innerHeight },
    scroll: { x: window.scrollX, y: window.scrollY, width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight },
    overflowX: document.documentElement.scrollWidth > window.innerWidth + 1,
  });
})()
`;

// The resolve routine is built per request by embedding a JSON literal of the
// spec.  Only known keys are read; nothing is evaluated from the caller.
//
// The candidate list is produced by the *same* builder that the snapshot uses,
// so a role, name or reference always refers to the same element in both.
function resolveScript(spec) {
  const json = JSON.stringify(spec);
  return `(() => {
    const spec = ${json};
    const all = Array.from(document.querySelectorAll('*'));
    const textOf = (el) => (el.innerText || el.textContent || '').replace(/\\s+/g,' ').trim();

    let chosen = null;
    let matchCount = 0;
    let reason = 'no visible element matched the requested target';

    if (spec.css) {
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
    } else {
      const elements = ${ELEMENT_BUILDER};
      const byRef = (ref) => {
        const index = Number(String(ref).replace(/^e/, ''));
        if (!Number.isFinite(index) || index < 1 || index > elements.length) return null;
        const candidate = elements[index - 1];
        return candidate ? all[candidate.domIndex] || null : null;
      };
      if (spec.ref) {
        chosen = byRef(spec.ref);
        if (!chosen) reason = "reference '" + spec.ref + "' no longer exists on this page";
        matchCount = chosen ? 1 : 0;
      } else {
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
          if (spec.testId) {
            const el = all[candidate.domIndex];
            const tid = el ? (el.getAttribute('data-testid') || el.getAttribute('data-test-id')) : null;
            if (tid !== spec.testId) return false;
          }
          return true;
        });
        matchCount = matches.length;
        const candidate = spec.nth !== undefined && spec.nth !== null ? matches[spec.nth] : matches[0];
        chosen = candidate ? all[candidate.domIndex] || null : null;
        if (!chosen && !matchCount) {
          reason = 'no visible element matched the requested role/name';
        }
      }
    }

    if (!chosen) return JSON.stringify({ found: false, reason: reason, matches: matchCount });

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
      value: ('value' in chosen && typeof chosen.value === 'string') ? chosen.value : null,
      matches: matchCount,
    });
  })()`;
}

async function evalInPage(session, source, awaitPromise = false, explicitContext) {
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
  if (typeof value === "string") {
    try {
      return JSON.parse(value);
    } catch {
      return value;
    }
  }
  return value;
}

async function pageInfo(session) {
  const info = await evalInPage(session, `(() => JSON.stringify({ url: location.href, title: document.title, readyState: document.readyState }))()`);
  return info || { url: null, title: null, readyState: null };
}

/* ------------------------------------------------------------------ */
/* Operation dispatch                                                  */
/* ------------------------------------------------------------------ */


/* ------------------------------------------------------------------ */
/* Keyboard helpers                                                    */
/* ------------------------------------------------------------------ */

/**
 * WebDriver key codes.  Firefox's WebDriver BiDi implementation requires a
 * single code point or grapheme cluster per key action, and expects the
 * `keyDown` / `keyUp` action subtypes.
 */
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

/**
 * Builds the keyDown/keyUp action list for a literal string.
 */
function typeActions(value) {
  const actions = [];
  for (const g of graphemes(String(value))) {
    actions.push({ type: "keyDown", value: g });
    actions.push({ type: "keyUp", value: g });
  }
  return actions;
}

/**
 * Resolves a key name to the value that WebDriver BiDi expects.
 */
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

const OPS = {
  async capabilities() {
    const browsers = await discoverBrowsers();
    return {
      node_version: process.version,
      platform: process.platform,
      arch: process.arch,
      browsers,
      driver: {
        name: "webdriver-bidi",
        version: "native",
        external_driver_required: false,
      },
    };
  },

  async "session.create"(params) {
    const browsers = await discoverBrowsers();
    const requested = (params.browser || "firefox").toLowerCase();
    const chosen = browsers.find((b) => b.name === requested && b.available);
    if (!chosen) {
      const available = browsers.map((b) => b.path).join(", ") || "none";
      throw new BiDiError(
        "browser_unavailable",
        `No usable ${requested} binary was found. Searched: ${available}. ` +
        `Install ${requested} (for example 'sudo apt install firefox-esr' or 'sudo dnf install firefox') and retry.`
      );
    }
    const id = `bs_${++sessionCounter}_${Date.now().toString(36)}`;
    const session = new BrowserSession(id, {
      browserPath: chosen.path,
      headless: params.headless !== false,
      viewport: params.viewport || { width: 1280, height: 800 },
    });
    await session.start();
    session.browserVersion = chosen.version;
    sessions.set(id, session);
    return {
      session_id: id,
      browser: "firefox",
      browser_path: chosen.path,
      browser_version: chosen.version,
      headless: session.opts.headless,
      profile_isolated: true,
      driver: "webdriver-bidi",
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
    const context = await session.context();
    const url = params.url;
    const timeout = params.timeout_ms || DEFAULT_NAV_TIMEOUT_MS;
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
  },

  async "page.info"(params) {
    const session = requireSession(params.session_id);
    return pageInfo(session);
  },

  async "page.snapshot"(params) {
    const session = requireSession(params.session_id);
    const snap = await evalInPage(session, SNAPSHOT_SOURCE);
    if (!snap) throw new BiDiError("snapshot_failed", "The page snapshot could not be produced");
    return snap;
  },

  async "page.click"(params) {
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
      throw new BiDiError("element_not_found", `Could not find a clickable element: ${lastReason}`);
    }
    if (located.disabled) {
      throw new BiDiError("element_disabled", `The element '${located.name || located.tag}' is disabled and cannot be clicked`);
    }

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

    await delay(params.settle_ms === undefined ? 250 : params.settle_ms);
    const after = await pageInfo(session);
    return {
      status: "clicked",
      element: { name: located.name, role: located.role, tag: located.tag, x: located.x, y: located.y },
      matches: located.matches,
      url: after.url,
      title: after.title,
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
      throw new BiDiError("element_not_found", `Could not find an editable element: ${lastReason}`);
    }
    if (located.disabled) {
      throw new BiDiError("element_disabled", `The field '${located.name || located.tag}' is disabled`);
    }

    const context = await session.context();
    // Focus the field with a real pointer click.
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

    // Select the existing contents and remove them, then type the new value
    // with real key events.
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

    const after = await evalInPage(
      session,
      `(() => { const el = document.activeElement; return el ? JSON.stringify({ tag: el.tagName, value: ('value' in el ? el.value : null) }) : null; })()`
    );
    return {
      status: "filled",
      element: { name: located.name, role: located.role, tag: located.tag },
      value_after: after ? after.value : null,
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

    // The check runs entirely inside the page: an obviously invalid value is
    // entered, the browser's own constraint validation is asked whether the
    // value is acceptable, and the original value is restored.  Nothing is
    // ever submitted.
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
        // The field's *name* often states the expected format even when the
        // declared type does not enforce it, so the probe value is derived
        // from both.
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
        // Restore the original value so that the page is left untouched.
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
          // A field that claims a constrained type (either through the type
          // attribute or through its name) but still accepts an obviously
          // invalid value does not validate its input.
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
    const context = await session.context();
    const key = params.key || "";
    if (!key) throw new BiDiError("invalid_request", "page.press requires a 'key' value");
    const value = keyValue(key);
    if (value === null) {
      throw new BiDiError(
        "invalid_request",
        `'${key}' is not a single key. Use a named key (Enter, Tab, Escape, Backspace, ArrowDown, ...) or a single character.`
      );
    }
    await session.client.send("input.performActions", {
      context,
      actions: [
        { type: "key", id: "keyboard", actions: [{ type: "keyDown", value }, { type: "keyUp", value }] },
      ],
    });
    await delay(params.settle_ms === undefined ? 150 : params.settle_ms);
    const after = await pageInfo(session);
    return { status: "pressed", key, url: after.url, title: after.title };
  },

  async "page.screenshot"(params) {
    const session = requireSession(params.session_id);
    const context = await session.context();
    const shot = await session.client.send("browsingContext.captureScreenshot", {
      context,
      origin: params.full_page ? "document" : "viewport",
    });
    const data = shot.data;
    return {
      base64_png: data,
      width: params.width || null,
      height: params.height || null,
      origin: params.full_page ? "document" : "viewport",
    };
  },

  async "page.tabs"(params) {
    const session = requireSession(params.session_id);
    const tree = await session.client.send("browsingContext.getTree", {});
    const tabs = (tree.contexts || []).map((c) => ({
      context_id: c.context,
      url: c.url,
      title: c.children ? undefined : undefined,
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
      } catch { /* title is optional */ }
    }
    return { tabs };
  },

  async "page.set_viewport"(params) {
    const session = requireSession(params.session_id);
    const width = Math.max(320, Math.min(3840, Number(params.width) || 1280));
    const height = Math.max(240, Math.min(2160, Number(params.height) || 800));
    const context = await session.context();
    await session.client.send("browsingContext.setViewport", { context, viewport: { width, height } });
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
        browser: "firefox",
        browser_version: s.browserVersion,
        headless: s.opts.headless,
        closed: s.closed,
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
  for (const key of ["role", "name", "text", "tag", "testId", "css", "nth", "ref"]) {
    if (target[key] !== undefined && target[key] !== null) out[key] = target[key];
  }
  if (!out.role && !out.name && !out.text && !out.tag && !out.testId && !out.css) {
    throw new BiDiError("invalid_request", "The target must specify at least one of role, name, text, tag, testId or css");
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
