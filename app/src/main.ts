import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal as XtermTerminal } from "@xterm/xterm";
import {
  ArrowLeft,
  ChevronRight,
  Copy,
  createIcons,
  Download,
  FileText,
  Folder,
  MessageSquare,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Server,
  Settings,
  Square,
  Terminal,
  Trash2,
  X,
} from "lucide";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";

type ServerSummary = { id: string; name: string; base_url: string; has_sk: boolean; is_default: boolean };
type ServerProfile = { id: string; name: string; base_url: string; sk: string; is_default: boolean };
type TerminalInstance = {
  id: string;
  window_label: string;
  workdir: string;
  server_id: string | null;
  resume: boolean;
  codex_version: string | null;
  pid: number | null;
  status: string;
};
type CodexSettings = {
  model: string;
  model_reasoning_effort: string;
  execution_mode: string;
  web_search: string;
  personality: string;
  config_error: string | null;
};
type Snapshot = {
  app_version: string;
  codex_version: string | null;
  code_home: string;
  config_toml: string;
  codex_settings: CodexSettings;
  servers: ServerSummary[];
  terminals: TerminalInstance[];
  pending_codex_versions: string[];
  staged_app_updates: string[];
};
type ReleaseAsset = { name: string; download_url: string; size: number; digest?: string | null };
type ReleaseInfo = {
  tag_name: string;
  name: string;
  html_url: string;
  published_at: string | null;
  update_available: boolean;
  assets: ReleaseAsset[];
};
type CodexUpdateInfo = { current_version: string | null; latest_version: string; update_available: boolean };
type UpdateResult = { version: string; path: string; kind: string };
type ControlView = "sessions" | "api" | "settings" | "updates";
type StatusTone = "neutral" | "error" | "success";
type UpdateState = { release?: ReleaseInfo; codex?: CodexUpdateInfo; checking: boolean; checkedAt?: number };

const iconSet = {
  ArrowLeft,
  ChevronRight,
  Copy,
  Download,
  FileText,
  Folder,
  MessageSquare,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Server,
  Settings,
  Square,
  Terminal,
  Trash2,
  X,
};
const app = document.querySelector<HTMLDivElement>("#app");
const WORKDIRS_STORAGE_KEY = "recent-workdirs";

type EmbeddedTerminal = {
  instance: TerminalInstance;
  terminal: XtermTerminal;
  fit: FitAddon;
  pane: HTMLElement;
  status: HTMLElement;
  overflow: boolean;
  writing: boolean;
  unlisten: Array<() => void>;
  disposeInput: () => void;
};

type BrowserBridgeResponse<T> = { ok: boolean; result?: T; error?: string };
type BrowserBridgeEvent = { sequence: number; event: string; payload: unknown };
const tauriRuntime = "__TAURI_INTERNALS__" in window;
const bridgeListeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
let bridgeEventCursor: number | null = null;
let bridgeEventPump: Promise<void> | null = null;
let bridgeDisconnected = false;
let toastTimer: number | undefined;

async function waitForBridgeReady(): Promise<void> {
  if (tauriRuntime) return;
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch("/__tauri_codex__/health", { cache: "no-store" });
      const body = await response.json() as BrowserBridgeResponse<{ ready: boolean }>;
      if (response.ok && body.ok && body.result?.ready) return;
    } catch {
      // Vite can be ready briefly before the Rust development bridge starts.
    }
    await new Promise((resolve) => window.setTimeout(resolve, 250));
  }
  throw new Error("本地应用服务启动超时，请确认 Tauri 开发进程仍在运行");
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (tauriRuntime) return invoke<T>(command, args);
  let response: Response;
  try {
    response = await fetch("/__tauri_codex__/call", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ command, args: args ?? {} }),
    });
  } catch {
    throw new Error("本地应用服务暂不可用，请等待开发服务启动后重试");
  }
  const body = await response.json() as BrowserBridgeResponse<T>;
  if (!response.ok || !body.ok) throw new Error(body.error || `本地开发桥接请求失败（${response.status}）`);
  return body.result as T;
}

async function onEvent<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> {
  if (tauriRuntime) return listen<T>(event, handler);
  const listeners = bridgeListeners.get(event) ?? new Set();
  const wrapped = handler as (event: { payload: unknown }) => void;
  listeners.add(wrapped);
  bridgeListeners.set(event, listeners);
  startBridgeEventPump();
  return () => {
    listeners.delete(wrapped);
    if (listeners.size === 0) bridgeListeners.delete(event);
  };
}

function startBridgeEventPump(): void {
  if (bridgeEventPump) return;
  bridgeEventPump = (async () => {
    while ([...bridgeListeners.values()].some((listeners) => listeners.size > 0)) {
      try {
        const query = bridgeEventCursor === null ? "" : `?after=${bridgeEventCursor}`;
        const response = await fetch(`/__tauri_codex__/events${query}`, { cache: "no-store" });
        const body = await response.json() as BrowserBridgeResponse<{ cursor: number; events: BrowserBridgeEvent[] }>;
        if (!response.ok || !body.ok || !body.result) throw new Error(body.error || "本地开发事件桥接失败");
        const reconnected = bridgeDisconnected;
        bridgeDisconnected = false;
        if (reconnected) {
          window.location.reload();
          return;
        }
        bridgeEventCursor = body.result.cursor;
        for (const item of body.result.events) {
          for (const listener of bridgeListeners.get(item.event) ?? []) listener({ payload: item.payload });
        }
      } catch (error) {
        if (!bridgeDisconnected) console.warn("本地应用服务暂时断开，正在重连", error);
        bridgeDisconnected = true;
        await new Promise((resolve) => window.setTimeout(resolve, 1000));
      }
    }
  })().finally(() => { bridgeEventPump = null; });
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function mountIcons(): void { createIcons({ icons: iconSet }); }
function valueOf(id: string): string { return document.querySelector<HTMLInputElement>(`#${id}`)?.value ?? ""; }
function valueOfSelect(id: string): string { return document.querySelector<HTMLSelectElement>(`#${id}`)?.value ?? ""; }
function isControlView(value: string | null): value is ControlView {
  return value === "sessions" || value === "api" || value === "settings" || value === "updates";
}
function setStatus(message: string, tone: StatusTone = "neutral"): void {
  const toast = document.querySelector<HTMLElement>("#app-toast");
  if (!toast) return;
  window.clearTimeout(toastTimer);
  toast.textContent = message;
  toast.dataset.tone = tone;
  toast.hidden = false;
  toastTimer = window.setTimeout(() => { toast.hidden = true; }, tone === "error" ? 6500 : 3500);
}
function workdirLabel(value: string): string {
  const parts = value.replace(/\\+$/, "").split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || value;
}

function workdirKey(value: string): string {
  const normalized = value.trim().replace(/\//g, "\\");
  const withoutTrailingSeparator = /^[a-z]:\\$/i.test(normalized)
    ? normalized
    : normalized.replace(/\\+$/, "");
  return withoutTrailingSeparator.toLocaleLowerCase();
}

function normalizeWorkdir(value: string): string {
  const normalized = value.trim().replace(/\//g, "\\");
  return /^[a-z]:\\$/i.test(normalized) ? normalized : normalized.replace(/\\+$/, "");
}

function dedupeWorkdirs(values: string[]): string[] {
  const seen = new Set<string>();
  return values.reduce<string[]>((result, value) => {
    const normalized = normalizeWorkdir(value);
    const key = workdirKey(normalized);
    if (!normalized || seen.has(key)) return result;
    seen.add(key);
    result.push(normalized);
    return result;
  }, []);
}

function loadWorkdirs(lastWorkdir: string): string[] {
  try {
    const stored = JSON.parse(window.localStorage.getItem(WORKDIRS_STORAGE_KEY) ?? "[]");
    return dedupeWorkdirs([lastWorkdir, ...(Array.isArray(stored) ? stored.filter((value): value is string => typeof value === "string") : [])]);
  } catch {
    return dedupeWorkdirs([lastWorkdir]);
  }
}

function saveWorkdirs(workdirs: string[], activeWorkdir: string): void {
  window.localStorage.setItem(WORKDIRS_STORAGE_KEY, JSON.stringify(workdirs));
  if (activeWorkdir) window.localStorage.setItem("last-workdir", activeWorkdir);
  else window.localStorage.removeItem("last-workdir");
}

async function renderControl(): Promise<void> {
  if (!app) return;
  document.body.dataset.window = "control";
  app.innerHTML = `
    <main class="app-shell">
      <aside class="app-sidebar">
        <div class="brand"><span class="brand-mark">CX</span><strong>tauri-codex</strong></div>
        <nav class="primary-nav" aria-label="主导航">
          <button class="nav-item" data-view="sessions"><i data-lucide="terminal"></i><span>会话</span><span class="nav-count" id="nav-session-count"></span></button>
        </nav>
        <section class="session-tree-panel" id="session-tree-panel" aria-label="工作目录和运行中的会话"></section>
        <nav class="primary-nav utility-nav" aria-label="配置导航">
          <button class="nav-item" data-view="api"><i data-lucide="server"></i><span>配置模型</span></button>
          <button class="nav-item" data-view="settings"><i data-lucide="settings"></i><span>设置</span></button>
          <button class="nav-item" data-view="updates"><i data-lucide="download"></i><span>更新</span><span class="nav-status" id="nav-update-status"></span></button>
        </nav>
        <div class="sidebar-meta"><span>Codex</span><strong id="sidebar-codex-version">--</strong></div>
      </aside>
      <section class="app-main">
        <section class="control-workspace" id="control-workspace">
          <header class="view-header">
            <div><h1 id="view-title">会话</h1><span id="view-meta"></span></div>
            <div id="view-actions"></div>
          </header>
          <div class="view-content" id="view-content"></div>
        </section>
        <section class="terminal-workspace" id="terminal-workspace" aria-label="Codex 终端" hidden>
          <div class="terminal-deck" id="terminal-deck"></div>
        </section>
      </section>
      <div class="toast" id="app-toast" data-tone="neutral" hidden></div>
      <div class="session-launcher-backdrop" id="session-launcher" hidden>
        <section class="session-launcher-dialog" role="dialog" aria-modal="true" aria-labelledby="session-launcher-title">
          <div class="session-launcher-heading"><div><h2 id="session-launcher-title">新建会话</h2><small id="session-launcher-workdir"></small></div><button class="icon-button" id="session-launcher-close" type="button" title="关闭" aria-label="关闭"><i data-lucide="x"></i></button></div>
          <div id="session-launcher-body"></div>
        </section>
      </div>
    </main>`;

  let snapshot = await call<Snapshot>("get_snapshot");
  // The product contract defines sessions as the startup view. Navigation state
  // is intentionally transient so a previous visit to settings cannot produce
  // an unexpected or incomplete first screen after relaunch.
  let activeView: ControlView = "sessions";
  let updateState: UpdateState = { checking: false };
  let draftWorkdir = normalizeWorkdir(window.localStorage.getItem("last-workdir") ?? "");
  let recentWorkdirs = loadWorkdirs(draftWorkdir);
  if (!draftWorkdir && recentWorkdirs.length > 0) draftWorkdir = recentWorkdirs[0];
  let draftServerId = window.localStorage.getItem("last-server") ?? snapshot.servers.find((server) => server.is_default)?.id ?? "";
  let activeTerminalId: string | null = null;
  let resizeTimer: number | undefined;
  const terminals = new Map<string, EmbeddedTerminal>();
  const terminalPromises = new Map<string, Promise<EmbeddedTerminal>>();
  const collapsedWorkdirs = new Set<string>();
  const terminalOrdinals = new Map<string, number>();
  const nextOrdinalByWorkdir = new Map<string, number>();
  let openSessionLauncher: (workdir: string, resume?: boolean) => void = () => undefined;
  const controlWorkspace = document.querySelector<HTMLElement>("#control-workspace")!;
  const terminalWorkspace = document.querySelector<HTMLElement>("#terminal-workspace")!;
  const terminalDeck = document.querySelector<HTMLElement>("#terminal-deck")!;

  const updateChrome = (): void => {
    const version = document.querySelector<HTMLElement>("#sidebar-codex-version");
    if (version) version.textContent = snapshot.codex_version ?? "未安装";
    const count = document.querySelector<HTMLElement>("#nav-session-count");
    if (count) {
      count.textContent = snapshot.terminals.length > 0 ? String(snapshot.terminals.length) : "";
      count.hidden = snapshot.terminals.length === 0;
    }
    renderSessionTree();
  };
  const renderSessionTree = (): void => {
    const panel = document.querySelector<HTMLElement>("#session-tree-panel");
    if (!panel) return;
    const groups = new Map<string, { workdir: string; sessions: TerminalInstance[] }>();
    for (const workdir of recentWorkdirs) {
      groups.set(workdirKey(workdir), { workdir, sessions: [] });
    }
    for (const terminal of snapshot.terminals) {
      const key = workdirKey(terminal.workdir);
      const group = groups.get(key) ?? { workdir: terminal.workdir, sessions: [] };
      group.sessions.push(terminal);
      groups.set(key, group);
    }
    const directories = [...groups.entries()];
    panel.innerHTML = `
      <div class="tree-heading">
        <span>工作目录</span>
        <span class="tree-heading-meta">${directories.length} 个目录 · ${snapshot.terminals.length} 个运行中</span>
        <button class="icon-button tree-add-workdir" type="button" title="添加工作目录" aria-label="添加工作目录"><i data-lucide="plus"></i></button>
      </div>
      ${directories.length > 0 ? `<div class="workspace-tree">
        ${directories.map(([key, { workdir, sessions }]) => `
          <div class="workspace-node-shell ${workdirKey(draftWorkdir) === key ? "is-selected" : ""}" data-workdir="${escapeHtml(key)}">
            <details class="workspace-node" ${collapsedWorkdirs.has(key) ? "" : "open"}>
              <summary class="workspace-summary ${workdirKey(draftWorkdir) === key ? "is-selected" : ""}" data-workdir="${escapeHtml(workdir)}" title="${escapeHtml(workdir)}">
                <i class="tree-disclosure" data-lucide="chevron-right"></i>
                <i class="tree-folder" data-lucide="folder"></i>
                <span class="workspace-copy"><strong>${escapeHtml(workdirLabel(workdir))}</strong><small>${escapeHtml(workdir)}</small></span>
                <span class="workspace-count">${sessions.length}</span>
              </summary>
              <div class="tree-session-list">
                ${sessions.length > 0 ? sessions.map((terminal) => {
                  const server = snapshot.servers.find((item) => item.id === terminal.server_id);
                  let ordinal = terminalOrdinals.get(terminal.id);
                  if (ordinal === undefined) {
                    ordinal = (nextOrdinalByWorkdir.get(key) ?? 0) + 1;
                    nextOrdinalByWorkdir.set(key, ordinal);
                    terminalOrdinals.set(terminal.id, ordinal);
                  }
                  const label = terminal.resume ? `恢复会话 ${ordinal}` : `会话 ${ordinal}`;
                  const detail = server?.name || "配置已删除";
                  return `
                    <button class="tree-session is-running ${terminal.id === activeTerminalId ? "is-selected" : ""}" type="button" data-terminal-id="${escapeHtml(terminal.id)}" aria-pressed="${terminal.id === activeTerminalId}" title="${escapeHtml(label)} · ${escapeHtml(workdir)}">
                      <span class="tree-session-indicator" data-status="${escapeHtml(terminal.status)}"></span>
                      <span><strong>${escapeHtml(label)}</strong><small>${escapeHtml(detail)}</small></span>
                    </button>`;
                }).join("") : '<div class="tree-state"><span>暂无运行中的会话</span></div>'}
              </div>
            </details>
            <div class="workspace-node-actions">
              <button class="icon-button tree-new-session" type="button" data-workdir="${escapeHtml(workdir)}" title="在此目录新建会话" aria-label="在 ${escapeHtml(workdirLabel(workdir))} 新建会话"><i data-lucide="plus"></i></button>
              ${recentWorkdirs.some((item) => workdirKey(item) === key) ? `<button class="icon-button tree-remove-workdir" type="button" data-workdir="${escapeHtml(workdir)}" title="从列表移除" aria-label="移除 ${escapeHtml(workdirLabel(workdir))}"><i data-lucide="x"></i></button>` : ""}
            </div>
          </div>`).join("")}
      </div>` : '<div class="tree-state tree-empty"><i data-lucide="folder"></i><strong>还没有工作目录</strong><button class="button button-secondary tree-add-workdir" type="button">添加工作目录</button></div>'}`;
    panel.querySelectorAll<HTMLDetailsElement>(".workspace-node").forEach((node) => {
      node.addEventListener("toggle", () => {
        const workdir = node.closest<HTMLElement>(".workspace-node-shell")?.dataset.workdir;
        if (!workdir) return;
        if (node.open) collapsedWorkdirs.delete(workdir);
        else collapsedWorkdirs.add(workdir);
      });
    });
    panel.querySelectorAll<HTMLElement>(".workspace-summary").forEach((summary) => {
      summary.addEventListener("click", () => {
        const workdir = summary.dataset.workdir;
        if (!workdir) return;
        selectWorkdir(workdir);
      });
    });
    panel.querySelectorAll<HTMLButtonElement>(".tree-session").forEach((button) => {
      button.addEventListener("click", () => {
        const instance = snapshot.terminals.find((terminal) => terminal.id === button.dataset.terminalId);
        if (instance) void showTerminal(instance);
      });
    });
    panel.querySelectorAll<HTMLButtonElement>(".tree-add-workdir").forEach((button) => {
      button.addEventListener("click", () => void chooseWorkdir());
    });
    panel.querySelectorAll<HTMLButtonElement>(".tree-new-session").forEach((button) => {
      button.addEventListener("click", () => {
        const workdir = button.dataset.workdir;
        if (!workdir) return;
        selectWorkdir(workdir);
        openSessionLauncher(workdir);
      });
    });
    panel.querySelectorAll<HTMLButtonElement>(".tree-remove-workdir").forEach((button) => {
      button.addEventListener("click", (event) => {
        event.stopPropagation();
        if (button.dataset.workdir) removeWorkdir(button.dataset.workdir);
      });
    });
    mountIcons();
  };
  const setEmbeddedStatus = (entry: EmbeddedTerminal, message: string, tone: StatusTone): void => {
    entry.status.textContent = message;
    entry.status.dataset.tone = tone;
  };
  const disposeTerminal = (id: string): void => {
    const entry = terminals.get(id);
    if (!entry) return;
    entry.unlisten.forEach((unlisten) => unlisten());
    entry.disposeInput();
    entry.terminal.dispose();
    entry.pane.remove();
    terminals.delete(id);
    terminalPromises.delete(id);
  };
  const resizeTerminal = (entry: EmbeddedTerminal): void => {
    if (activeTerminalId !== entry.instance.id || terminalWorkspace.hidden) return;
    entry.fit.fit();
    window.clearTimeout(resizeTimer);
    resizeTimer = window.setTimeout(() => {
      void call("terminal_resize", {
        id: entry.instance.id,
        request: { rows: entry.terminal.rows, cols: entry.terminal.cols, pixel_width: 0, pixel_height: 0 },
      }).catch((error) => setEmbeddedStatus(entry, String(error), "error"));
    }, 50);
  };
  const createEmbeddedTerminal = async (instance: TerminalInstance): Promise<EmbeddedTerminal> => {
    const pane = document.createElement("section");
    pane.className = "embedded-terminal-pane";
    pane.dataset.terminalId = instance.id;
    pane.innerHTML = `
      <header class="terminal-toolbar">
        <div class="terminal-identity">
          <button class="icon-button terminal-action terminal-back" type="button" title="返回会话" aria-label="返回会话"><i data-lucide="arrow-left"></i></button>
          <i data-lucide="terminal"></i>
          <strong>${escapeHtml(workdirLabel(instance.workdir))}</strong>
          <span class="terminal-status">正在连接</span>
        </div>
        <div class="terminal-actions">
          <button class="icon-button terminal-action terminal-interrupt" type="button" title="中断" aria-label="中断"><i data-lucide="square"></i></button>
          <button class="icon-button terminal-action terminal-restart" type="button" title="重新启动" aria-label="重新启动"><i data-lucide="rotate-ccw"></i></button>
          <button class="icon-button terminal-action terminal-terminate" type="button" title="停止" aria-label="停止"><i data-lucide="x"></i></button>
          <button class="icon-button terminal-action is-danger terminal-force-terminate" type="button" title="强制停止" aria-label="强制停止"><i data-lucide="trash-2"></i></button>
        </div>
      </header>
      <div class="terminal-surface"></div>`;
    terminalDeck.append(pane);
    mountIcons();

    const terminal = new XtermTerminal({
      cursorBlink: true,
      convertEol: false,
      scrollback: 4000,
      fontFamily: "Cascadia Mono, Consolas, monospace",
      theme: { background: "#111311", foreground: "#e6e9e3", cursor: "#d9b44a", selectionBackground: "#425249" },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    const surface = pane.querySelector<HTMLElement>(".terminal-surface")!;
    terminal.open(surface);
    const entry: EmbeddedTerminal = {
      instance,
      terminal,
      fit,
      pane,
      status: pane.querySelector<HTMLElement>(".terminal-status")!,
      overflow: false,
      writing: false,
      unlisten: [],
      disposeInput: () => {},
    };
    terminals.set(instance.id, entry);

    const markOverflow = (): void => {
      entry.overflow = true;
      terminal.options.disableStdin = true;
      setEmbeddedStatus(entry, "输出过载", "error");
    };
    entry.unlisten = await Promise.all([
      onEvent<{ sequence: number; data: string }>(`terminal-output:${instance.window_label}`, (event) => {
        if (entry.overflow) return;
        if (entry.writing) {
          markOverflow();
          return;
        }
        entry.writing = true;
        terminal.write(event.payload.data, () => {
          entry.writing = false;
          void call("terminal_rendered", { id: instance.id, request: { sequence: event.payload.sequence } })
            .catch((error) => setEmbeddedStatus(entry, String(error), "error"));
        });
      }),
      onEvent<string>(`terminal-error:${instance.window_label}`, (event) => setEmbeddedStatus(entry, `Host 错误：${event.payload}`, "error")),
      onEvent<{ code: number | null }>(`terminal-exit:${instance.window_label}`, (event) => {
        setEmbeddedStatus(entry, `已退出${event.payload.code === null ? "" : ` (${event.payload.code})`}`, "error");
        terminal.options.disableStdin = true;
      }),
      onEvent(`terminal-overflow:${instance.window_label}`, markOverflow),
      onEvent<{ responsive: boolean }>(`terminal-heartbeat:${instance.window_label}`, (event) => {
        setEmbeddedStatus(entry, event.payload.responsive ? "已连接" : "Host 无响应", event.payload.responsive ? "success" : "error");
      }),
    ]);
    const input = terminal.onData((data) => {
      if (!entry.overflow) {
        void call("terminal_input", { id: instance.id, data }).catch((error) => setEmbeddedStatus(entry, String(error), "error"));
      }
    });
    const pasteImageFromClipboard = (event: ClipboardEvent): void => {
      const items = Array.from(event.clipboardData?.items ?? []);
      const hasImage = items.some((item) => item.kind === "file" && item.type.toLowerCase().startsWith("image/"));
      if (!hasImage || entry.overflow) return;
      // Codex TUI owns image encoding and attachment state. Forward its native Ctrl+V binding;
      // do not read or serialize clipboard bytes in the desktop UI.
      event.preventDefault();
      event.stopImmediatePropagation();
      void call("terminal_input", { id: instance.id, data: "\u0016" })
        .catch((error) => setEmbeddedStatus(entry, String(error), "error"));
    };
    surface.addEventListener("paste", pasteImageFromClipboard, true);
    entry.disposeInput = () => {
      input.dispose();
      surface.removeEventListener("paste", pasteImageFromClipboard, true);
    };
    pane.querySelector<HTMLButtonElement>(".terminal-back")?.addEventListener("click", () => showControl("sessions"));
    pane.querySelector<HTMLButtonElement>(".terminal-interrupt")?.addEventListener("click", () => {
      void call("interrupt_terminal", { id: instance.id }).catch((error) => setEmbeddedStatus(entry, String(error), "error"));
    });
    pane.querySelector<HTMLButtonElement>(".terminal-restart")?.addEventListener("click", async () => {
      if (!window.confirm("重新启动当前 Codex 会话？")) return;
      try {
        setEmbeddedStatus(entry, "正在重新启动", "neutral");
        const replacement = await call<TerminalInstance>("restart_terminal", { id: instance.id });
        snapshot.terminals = snapshot.terminals.filter((item) => item.id !== instance.id).concat(replacement);
        disposeTerminal(instance.id);
        updateChrome();
        await showTerminal(replacement);
      } catch (error) {
        setEmbeddedStatus(entry, String(error), "error");
      }
    });
    pane.querySelector<HTMLButtonElement>(".terminal-terminate")?.addEventListener("click", () => {
      if (!window.confirm("停止当前 Codex 进程树？")) return;
      setEmbeddedStatus(entry, "正在停止", "neutral");
      void call("terminate_terminal", { id: instance.id }).catch((error) => setEmbeddedStatus(entry, String(error), "error"));
    });
    pane.querySelector<HTMLButtonElement>(".terminal-force-terminate")?.addEventListener("click", () => {
      if (!window.confirm("强制停止 Session Host 和 Codex 进程树？")) return;
      setEmbeddedStatus(entry, "正在强制停止", "neutral");
      void call("force_terminate_terminal", { id: instance.id }).catch((error) => setEmbeddedStatus(entry, String(error), "error"));
    });

    try {
      if (activeTerminalId === instance.id && !terminalWorkspace.hidden) fit.fit();
      await call("terminal_ready", {
        id: instance.id,
        request: { rows: terminal.rows, cols: terminal.cols, pixel_width: 0, pixel_height: 0 },
      });
      setEmbeddedStatus(entry, "已连接", "success");
    } catch (error) {
      setEmbeddedStatus(entry, String(error), "error");
    }
    return entry;
  };
  const ensureTerminal = (instance: TerminalInstance): Promise<EmbeddedTerminal> => {
    const current = terminals.get(instance.id);
    if (current) return Promise.resolve(current);
    const pending = terminalPromises.get(instance.id);
    if (pending) return pending;
    const created = createEmbeddedTerminal(instance);
    terminalPromises.set(instance.id, created);
    return created;
  };
  const showTerminal = async (instance: TerminalInstance): Promise<void> => {
    activeTerminalId = instance.id;
    renderSessionTree();
    document.body.dataset.window = "terminal";
    controlWorkspace.hidden = true;
    terminalWorkspace.hidden = false;
    for (const item of terminals.values()) item.pane.classList.toggle("is-active", item.instance.id === activeTerminalId);
    const entry = await ensureTerminal(instance);
    if (activeTerminalId !== instance.id || terminalWorkspace.hidden) return;
    for (const item of terminals.values()) item.pane.classList.toggle("is-active", item.instance.id === activeTerminalId);
    window.requestAnimationFrame(() => {
      if (activeTerminalId !== instance.id || terminalWorkspace.hidden) return;
      resizeTerminal(entry);
      entry.terminal.focus();
    });
  };
  const cleanupEndedTerminals = (): boolean => {
    const running = new Set(snapshot.terminals.map((terminal) => terminal.id));
    const activeTerminalEnded = activeTerminalId !== null && !running.has(activeTerminalId);
    if (activeTerminalEnded) activeTerminalId = null;
    for (const id of terminals.keys()) {
      if (!running.has(id)) disposeTerminal(id);
    }
    return activeTerminalEnded;
  };
  const refreshSnapshot = async (rerender = true): Promise<void> => {
    snapshot = await call<Snapshot>("get_snapshot");
    if (!snapshot.servers.some((server) => server.id === draftServerId)) {
      draftServerId = snapshot.servers.find((server) => server.is_default)?.id ?? "";
    }
    updateChrome();
    const activeTerminalEnded = cleanupEndedTerminals();
    await Promise.all(snapshot.terminals.map((terminal) => ensureTerminal(terminal)));
    if (activeTerminalEnded) {
      showControl("sessions");
      return;
    }
    if (rerender && activeTerminalId === null) renderView(activeView);
    else if (activeView === "updates") renderUpdatePanel(snapshot, updateState);
  };
  const launchSession = async (workdir: string, serverId: string, resume: boolean): Promise<void> => {
    const normalizedWorkdir = normalizeWorkdir(workdir);
    if (!normalizedWorkdir) {
      setStatus("请先选择工作目录", "error");
      document.querySelector<HTMLButtonElement>("#choose-workdir")?.focus();
      return;
    }
    if (!serverId) {
      setStatus("请选择配置模型", "error");
      return;
    }
    draftWorkdir = normalizedWorkdir;
    recentWorkdirs = dedupeWorkdirs([normalizedWorkdir, ...recentWorkdirs]);
    draftServerId = serverId;
    saveWorkdirs(recentWorkdirs, draftWorkdir);
    window.localStorage.setItem("last-server", serverId);
    document.querySelectorAll<HTMLButtonElement>(".session-launch-action").forEach((button) => { button.disabled = true; });
    try {
      const instance = await call<TerminalInstance>("start_terminal", {
        request: { workdir: normalizedWorkdir, server_id: serverId || null, resume },
      });
      snapshot.terminals.push(instance);
      updateChrome();
      setStatus(resume ? "已打开 Codex 会话选择器" : "已打开 Codex 新会话", "success");
      await showTerminal(instance);
    } catch (error) {
      setStatus(String(error), "error");
      renderView("sessions");
    }
  };
  const chooseWorkdir = async (): Promise<void> => {
    try {
      const selected = tauriRuntime
        ? await openDialog({ directory: true, multiple: false, title: "选择工作目录", defaultPath: draftWorkdir || undefined })
        : window.prompt("工作目录", draftWorkdir || "C:\\");
      if (typeof selected !== "string" || !selected.trim()) return;
      draftWorkdir = normalizeWorkdir(selected);
      recentWorkdirs = dedupeWorkdirs([draftWorkdir, ...recentWorkdirs]);
      saveWorkdirs(recentWorkdirs, draftWorkdir);
      renderSessionTree();
      renderSessionPage();
    } catch (error) {
      setStatus(`无法选择工作目录：${String(error)}`, "error");
    }
  };
  const selectWorkdir = (workdir: string): void => {
    draftWorkdir = normalizeWorkdir(workdir);
    saveWorkdirs(recentWorkdirs, draftWorkdir);
    if (activeTerminalId !== null || activeView !== "sessions") {
      showControl("sessions");
      return;
    }
    document.querySelectorAll<HTMLElement>(".workspace-summary").forEach((summary) => {
      summary.classList.toggle("is-selected", workdirKey(summary.dataset.workdir ?? "") === workdirKey(draftWorkdir));
    });
    renderSessionPage();
  };
  const removeWorkdir = (workdir: string): void => {
    const removedKey = workdirKey(workdir);
    recentWorkdirs = recentWorkdirs.filter((item) => workdirKey(item) !== removedKey);
    if (workdirKey(draftWorkdir) === removedKey) draftWorkdir = recentWorkdirs[0] ?? "";
    saveWorkdirs(recentWorkdirs, draftWorkdir);
    renderSessionTree();
    renderSessionPage();
    setStatus("已从工作目录列表移除", "success");
  };
  openSessionLauncher = (workdir: string, resume = false): void => {
    const backdrop = document.querySelector<HTMLElement>("#session-launcher");
    const body = document.querySelector<HTMLElement>("#session-launcher-body");
    const workdirLabelNode = document.querySelector<HTMLElement>("#session-launcher-workdir");
    if (!backdrop || !body || !workdirLabelNode) return;
    const options = snapshot.servers
      .map((server) => `<option value="${escapeHtml(server.id)}">${escapeHtml(server.name)}${server.is_default ? "（默认）" : ""}</option>`)
      .join("");
    workdirLabelNode.textContent = workdir || "未选择工作目录";
    body.innerHTML = snapshot.servers.length > 0
      ? `<form id="session-launch-form" class="session-launcher-form">
          <label class="field"><span>配置模型</span><select id="session-server" required>${options}</select></label>
          <div class="session-launcher-actions">
            <button class="button button-secondary" id="resume-session" type="button"><i data-lucide="rotate-ccw"></i><span>恢复会话</span></button>
            <button class="button button-primary session-launch-action" type="submit"><i data-lucide="play"></i><span>新会话</span></button>
          </div>
        </form>`
      : `<div class="session-launcher-empty"><i data-lucide="server"></i><strong>还没有配置模型</strong><small>先完成配置，才能启动 Codex 会话。</small><button class="button button-primary" id="session-configure-model" type="button"><i data-lucide="settings"></i><span>配置模型</span></button></div>`;
    backdrop.hidden = false;
    mountIcons();
    const server = document.querySelector<HTMLSelectElement>("#session-server");
    if (server) server.value = snapshot.servers.some((item) => item.id === draftServerId)
      ? draftServerId
      : snapshot.servers.find((item) => item.is_default)?.id ?? snapshot.servers[0]?.id ?? "";
    document.querySelector<HTMLFormElement>("#session-launch-form")?.addEventListener("submit", (event) => {
      event.preventDefault();
      backdrop.hidden = true;
      void launchSession(workdir, valueOfSelect("session-server"), false);
    });
    document.querySelector<HTMLButtonElement>("#resume-session")?.addEventListener("click", () => {
      backdrop.hidden = true;
      void launchSession(workdir, valueOfSelect("session-server"), true);
    });
    body.querySelector<HTMLButtonElement>("#session-configure-model")?.addEventListener("click", () => {
      backdrop.hidden = true;
      showControl("api");
    });
    document.querySelector<HTMLButtonElement>("#session-launcher-close")?.focus();
    if (resume) document.querySelector<HTMLButtonElement>("#resume-session")?.focus();
    else window.requestAnimationFrame(() => document.querySelector<HTMLSelectElement>("#session-server")?.focus());
  };
  document.querySelector<HTMLButtonElement>("#session-launcher-close")?.addEventListener("click", () => {
    const launcher = document.querySelector<HTMLElement>("#session-launcher");
    if (launcher) launcher.hidden = true;
  });
  document.querySelector<HTMLElement>("#session-launcher")?.addEventListener("click", (event) => {
    if (event.target === event.currentTarget) (event.currentTarget as HTMLElement).hidden = true;
  });
  const checkUpdates = async (silent = false): Promise<void> => {
    if (updateState.checking) return;
    updateState.checking = true;
    renderUpdatePanel(snapshot, updateState);
    if (!silent) setStatus("正在检查更新");
    try {
      const [release, codex] = await Promise.all([
        call<ReleaseInfo>("check_app_update"),
        call<CodexUpdateInfo>("check_codex_update"),
      ]);
      updateState = { release, codex, checking: true, checkedAt: Date.now() };
      const staging: Promise<unknown>[] = [];
      if (release.update_available) staging.push(call("stage_app_update"));
      if (codex.update_available) {
        staging.push(call("stage_codex_update", { version: codex.latest_version }));
      }
      if (staging.length > 0) {
        const results = await Promise.allSettled(staging);
        const failures = results.filter((result): result is PromiseRejectedResult => result.status === "rejected");
        await refreshSnapshot(false);
        if (failures.length > 0 && !silent) setStatus(`自动更新暂存失败：${String(failures[0].reason)}`, "error");
      }
      if (!silent) setStatus(`${release.tag_name ? `桌面 ${release.tag_name}` : "桌面暂无发布版本"}，Codex ${codex.latest_version}`, "success");
    } catch (error) {
      if (!silent) setStatus(String(error), "error");
    } finally {
      updateState.checking = false;
      renderUpdatePanel(snapshot, updateState);
    }
  };
  const renderSessionPage = (): void => {
    renderSessionsView(snapshot, draftWorkdir);
    document.querySelectorAll<HTMLButtonElement>(".choose-workdir").forEach((button) => button.addEventListener("click", () => void chooseWorkdir()));
    mountIcons();
  };
  const renderView = (view: ControlView): void => {
    activeView = view;
    document.querySelectorAll<HTMLButtonElement>(".nav-item").forEach((button) => {
      button.classList.toggle("is-active", button.dataset.view === view);
      button.setAttribute("aria-current", button.dataset.view === view ? "page" : "false");
    });
    const title = document.querySelector<HTMLElement>("#view-title");
    const meta = document.querySelector<HTMLElement>("#view-meta");
    const actions = document.querySelector<HTMLElement>("#view-actions");
    const content = document.querySelector<HTMLElement>("#view-content");
    if (!title || !meta || !actions) return;
    content?.classList.toggle("is-session-view", view === "sessions");
    actions.innerHTML = "";
    if (view === "sessions") {
      title.textContent = "会话";
      meta.textContent = snapshot.terminals.length > 0 ? `${snapshot.terminals.length} 个会话运行中` : "";
      actions.innerHTML = '<button class="icon-button" id="header-new-session" title="新会话" aria-label="新会话"><i data-lucide="plus"></i></button>';
      renderSessionPage();
      document.querySelector<HTMLButtonElement>("#header-new-session")?.addEventListener("click", () => {
        if (draftWorkdir) openSessionLauncher(draftWorkdir);
        else void chooseWorkdir().then(() => { if (draftWorkdir) openSessionLauncher(draftWorkdir); });
      });
    } else if (view === "api") {
      title.textContent = "配置模型";
      meta.textContent = `${snapshot.servers.length} 个配置`;
      actions.innerHTML = '<button class="icon-button" id="new-server" title="新建配置模型" aria-label="新建配置模型"><i data-lucide="plus"></i></button>';
      renderApiView(snapshot);
      bindApiView(refreshSnapshot);
    } else if (view === "settings") {
      title.textContent = "设置";
      meta.textContent = `应用 ${snapshot.app_version}`;
      actions.innerHTML = "";
      renderSettingsView(snapshot);
      bindSettingsView(snapshot, refreshSnapshot, checkUpdates);
    } else {
      title.textContent = "更新";
      meta.textContent = `应用 ${snapshot.app_version}`;
      actions.innerHTML = "";
      renderUpdatesView(snapshot, updateState);
      bindUpdatesView(snapshot, refreshSnapshot, checkUpdates);
    }
    mountIcons();
  };
  function showControl(view: ControlView): void {
    activeTerminalId = null;
    renderSessionTree();
    document.body.dataset.window = "control";
    terminalWorkspace.hidden = true;
    controlWorkspace.hidden = false;
    cleanupEndedTerminals();
    renderView(view);
  }

  document.querySelectorAll<HTMLButtonElement>(".nav-item").forEach((button) => {
    button.addEventListener("click", () => {
      const view = button.dataset.view ?? null;
      if (isControlView(view)) showControl(view);
    });
  });
  updateChrome();
  renderView(activeView);
  await Promise.all(snapshot.terminals.map((terminal) => ensureTerminal(terminal)));
  const stateListener = await onEvent("terminal-state-changed", async () => {
    snapshot = await call<Snapshot>("get_snapshot");
    updateChrome();
    const activeTerminalEnded = cleanupEndedTerminals();
    await Promise.all(snapshot.terminals.map((terminal) => ensureTerminal(terminal)));
    if (activeTerminalEnded) {
      showControl("sessions");
      return;
    }
    if (activeTerminalId === null && activeView === "sessions") renderView(activeView);
    else if (activeTerminalId === null && activeView === "updates") renderUpdatePanel(snapshot, updateState);
  });
  window.addEventListener("resize", () => {
    const active = activeTerminalId ? terminals.get(activeTerminalId) : undefined;
    if (active) resizeTerminal(active);
  });
  window.addEventListener("unload", () => {
    stateListener();
    for (const id of [...terminals.keys()]) disposeTerminal(id);
  }, { once: true });
  void checkUpdates(true);
  window.setInterval(() => void checkUpdates(true), 6 * 60 * 60 * 1000);
}

function renderSessionsView(snapshot: Snapshot, draftWorkdir: string): void {
  const content = document.querySelector<HTMLElement>("#view-content");
  if (!content) return;
  const state = snapshot.servers.length === 0
    ? `<div class="session-empty-state"><i data-lucide="server"></i><strong>先配置模型</strong><small>配置完成后，从左侧工作目录创建或恢复会话。</small><button class="button button-primary" id="session-configure-model" type="button"><i data-lucide="settings"></i><span>配置模型</span></button></div>`
    : `<div class="session-empty-state"><i data-lucide="message-square"></i><strong>${draftWorkdir ? "选择一个工作目录开始" : "添加工作目录开始"}</strong><small>${draftWorkdir ? `当前目录：${escapeHtml(workdirLabel(draftWorkdir))}。使用左侧目录旁的 + 创建会话。` : "从左侧添加工作目录，然后创建会话。"}</small>${draftWorkdir ? "" : '<button class="button button-secondary" id="session-choose-workdir" type="button"><i data-lucide="folder"></i><span>添加工作目录</span></button>'}</div>`;
  content.innerHTML = `
    <div class="session-home">
      ${state}
    </div>`;
  content.querySelector<HTMLButtonElement>("#session-configure-model")?.addEventListener("click", () => {
    const launcher = document.querySelector<HTMLElement>("#session-launcher");
    if (launcher) launcher.hidden = true;
    const apiButton = document.querySelector<HTMLButtonElement>('.nav-item[data-view="api"]');
    apiButton?.click();
  });
  document.querySelector<HTMLButtonElement>("#session-choose-workdir")?.addEventListener("click", () => {
    document.querySelector<HTMLButtonElement>(".tree-add-workdir")?.click();
  });
}

function renderApiView(snapshot: Snapshot): void {
  const content = document.querySelector<HTMLElement>("#view-content");
  if (!content) return;
  const rows = snapshot.servers.length === 0
    ? '<div class="empty-state compact"><i data-lucide="server"></i><span>暂无配置模型</span></div>'
    : snapshot.servers.map((server) => `
      <button class="api-row" type="button" data-server-id="${escapeHtml(server.id)}">
        <span class="api-row-copy"><strong>${escapeHtml(server.name)}</strong><small>${escapeHtml(server.base_url)}</small></span>
        ${server.is_default ? '<span class="default-badge">默认</span>' : ""}
        <i data-lucide="chevron-right"></i>
      </button>`).join("");
  content.innerHTML = `
    <div class="api-layout">
      <section class="api-index">
        <div class="section-heading"><h2>配置模型</h2><span>${snapshot.servers.length}</span></div>
        <div class="api-list">${rows}</div>
      </section>
      <form class="api-editor" id="server-form">
        <div class="editor-heading"><h2 id="server-form-title">新建配置模型</h2></div>
        <input id="server-id" type="hidden" />
        <div class="form-fields">
          <label class="field"><span>名称</span><input id="server-name" autocomplete="off" required /></label>
          <label class="field"><span>URL</span><input id="server-base-url" type="url" placeholder="https://api.example.com/v1" autocomplete="off" required /></label>
          <label class="field"><span>API Key</span><input id="server-sk" type="password" autocomplete="off" required /></label>
          <label class="checkbox-field"><input id="server-default" type="checkbox" ${snapshot.servers.length === 0 ? "checked" : ""} /><span>默认配置</span></label>
        </div>
        <div class="form-actions">
          <button class="button button-danger" id="delete-server" type="button" disabled><i data-lucide="trash-2"></i><span>删除</span></button>
          <button class="button button-primary" type="submit"><i data-lucide="save"></i><span>保存</span></button>
        </div>
      </form>
    </div>`;
}

function bindApiView(refreshSnapshot: (rerender?: boolean) => Promise<void>): void {
  const reset = (): void => {
    document.querySelector<HTMLFormElement>("#server-form")?.reset();
    const id = document.querySelector<HTMLInputElement>("#server-id");
    if (id) id.value = "";
    const title = document.querySelector<HTMLElement>("#server-form-title");
    if (title) title.textContent = "新建配置模型";
    const remove = document.querySelector<HTMLButtonElement>("#delete-server");
    if (remove) remove.disabled = true;
    document.querySelector<HTMLInputElement>("#server-name")?.focus();
  };
  document.querySelector<HTMLButtonElement>("#new-server")?.addEventListener("click", reset);
  document.querySelectorAll<HTMLButtonElement>(".api-row").forEach((button) => {
    button.addEventListener("click", async () => {
      try {
        const profile = await call<ServerProfile>("get_server", { id: button.dataset.serverId });
        setServerForm(profile);
      } catch (error) {
        setStatus(String(error), "error");
      }
    });
  });
  document.querySelector<HTMLFormElement>("#server-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const profile: ServerProfile = {
      id: valueOf("server-id"),
      name: valueOf("server-name"),
      base_url: valueOf("server-base-url"),
      sk: valueOf("server-sk"),
      is_default: document.querySelector<HTMLInputElement>("#server-default")?.checked ?? false,
    };
    try {
      await call("save_server", { profile });
      setStatus("配置模型已保存", "success");
      await refreshSnapshot();
    } catch (error) {
      setStatus(String(error), "error");
    }
  });
  document.querySelector<HTMLButtonElement>("#delete-server")?.addEventListener("click", async () => {
    const id = valueOf("server-id");
    if (!id || !window.confirm("删除这个配置模型？")) return;
    try {
      await call("delete_server", { id });
      setStatus("配置模型已删除", "success");
      await refreshSnapshot();
    } catch (error) {
      setStatus(String(error), "error");
    }
  });
}

function setServerForm(profile: ServerProfile): void {
  const values: Record<string, string> = {
    "server-id": profile.id,
    "server-name": profile.name,
    "server-base-url": profile.base_url,
    "server-sk": profile.sk,
  };
  for (const [id, value] of Object.entries(values)) {
    const input = document.querySelector<HTMLInputElement>(`#${id}`);
    if (input) input.value = value;
  }
  const title = document.querySelector<HTMLElement>("#server-form-title");
  if (title) title.textContent = profile.name;
  const remove = document.querySelector<HTMLButtonElement>("#delete-server");
  if (remove) remove.disabled = false;
  const isDefault = document.querySelector<HTMLInputElement>("#server-default");
  if (isDefault) isDefault.checked = profile.is_default;
}

function renderChoiceOptions(current: string, choices: Array<{ value: string; label: string }>): string {
  const normalized = !choices.some((choice) => choice.value === current) && current
    ? [{ value: current, label: `高级配置：${current}` }, ...choices]
    : choices;
  return normalized.map((choice) => `<option value="${escapeHtml(choice.value)}" ${choice.value === current ? "selected" : ""}>${escapeHtml(choice.label)}</option>`).join("");
}

function renderExecutionOptions(current: string): string {
  const choices = [
    { value: "default", label: "Codex 默认" },
    { value: "standard", label: "标准" },
    { value: "automatic", label: "自动执行" },
    { value: "read-only", label: "只读" },
  ];
  if (current === "custom") choices.push({ value: "custom", label: "高级自定义" });
  return choices.map((choice) => `
    <label class="segment-option"><input type="radio" name="execution-mode" value="${escapeHtml(choice.value)}" ${choice.value === current ? "checked" : ""} /><span>${escapeHtml(choice.label)}</span></label>`).join("");
}

function renderSettingsView(snapshot: Snapshot): void {
  const content = document.querySelector<HTMLElement>("#view-content");
  if (!content) return;
  const settings = snapshot.codex_settings;
  content.innerHTML = `
    <div class="content-stack settings-stack">
      <section class="settings-section">
        <div class="section-heading settings-heading"><div><h2>Codex 默认设置</h2></div></div>
        ${settings.config_error ? `<div class="config-error">${escapeHtml(settings.config_error)}</div>` : ""}
        <form id="codex-settings-form">
          <div class="guided-grid">
            <label class="field guided-field"><span>模型</span><input id="setting-model" value="${escapeHtml(settings.model)}" placeholder="跟随 Codex 默认" /></label>
            <label class="field guided-field"><span>推理强度</span><select id="setting-reasoning">${renderChoiceOptions(settings.model_reasoning_effort, [
              { value: "", label: "跟随 Codex 默认" },
              { value: "minimal", label: "最少" },
              { value: "low", label: "低" },
              { value: "medium", label: "中" },
              { value: "high", label: "高" },
              { value: "xhigh", label: "最高（模型支持时）" },
            ])}</select></label>
            <label class="field guided-field"><span>联网搜索</span><select id="setting-web-search">${renderChoiceOptions(settings.web_search, [
              { value: "", label: "跟随 Codex 默认" },
              { value: "cached", label: "缓存结果" },
              { value: "indexed", label: "索引控制" },
              { value: "live", label: "实时联网" },
              { value: "disabled", label: "关闭" },
            ])}</select></label>
            <label class="field guided-field"><span>沟通风格</span><select id="setting-personality">${renderChoiceOptions(settings.personality, [
              { value: "", label: "跟随 Codex 默认" },
              { value: "friendly", label: "友好" },
              { value: "pragmatic", label: "务实" },
              { value: "none", label: "无指定风格" },
            ])}</select></label>
          </div>
          <fieldset class="execution-field"><legend>执行方式</legend><div class="segmented-control">${renderExecutionOptions(settings.execution_mode)}</div></fieldset>
          <div class="advanced-actions"><button class="button button-primary" type="submit"><i data-lucide="save"></i><span>保存设置</span></button></div>
        </form>
      </section>
      <details class="settings-section advanced-config">
        <summary><span><strong>高级配置</strong><code>config.toml</code></span><i data-lucide="chevron-right"></i></summary>
        <div class="advanced-body"><textarea id="config-editor" spellcheck="false">${escapeHtml(snapshot.config_toml)}</textarea><div class="advanced-actions"><button class="button button-secondary" id="save-config" type="button"><i data-lucide="file-text"></i><span>保存高级配置</span></button></div></div>
      </details>
      <section class="settings-section">
        <div class="section-heading settings-heading"><div><h2>数据目录</h2></div></div>
        <div class="setting-row path-row"><div class="setting-copy"><strong>CODEX_HOME</strong><code>${escapeHtml(snapshot.code_home)}</code></div><button class="icon-button" id="copy-codex-home" type="button" title="复制 CODEX_HOME" aria-label="复制 CODEX_HOME"><i data-lucide="copy"></i></button></div>
      </section>
    </div>`;
}

function renderUpdatesView(snapshot: Snapshot, updateState: UpdateState): void {
  const content = document.querySelector<HTMLElement>("#view-content");
  if (!content) return;
  content.innerHTML = `
    <div class="content-stack settings-stack updates-stack">
      <section class="settings-section">
        <div class="section-heading settings-heading"><div><h2>应用更新</h2><small>下载和应用会在这里明确显示</small></div><button class="button button-secondary" id="check-update" type="button"></button></div>
        <div class="setting-row"><div class="setting-copy"><strong>桌面应用</strong><small id="desktop-update-summary">尚未检查</small></div><span class="version-tag">${escapeHtml(snapshot.app_version)}</span><button class="button button-secondary update-button" id="desktop-update-action" type="button"></button></div>
        <div class="setting-row"><div class="setting-copy"><strong>内置 Codex</strong><small id="codex-update-summary">尚未检查</small></div><span class="version-tag">${escapeHtml(snapshot.codex_version ?? "未安装")}</span><button class="button button-secondary update-button" id="codex-update-action" type="button"></button></div>
      </section>
    </div>`;
  renderUpdatePanel(snapshot, updateState);
}

function bindUpdatesView(
  snapshot: Snapshot,
  refreshSnapshot: (rerender?: boolean) => Promise<void>,
  checkUpdates: (silent?: boolean) => Promise<void>,
): void {
  bindSettingsView(snapshot, refreshSnapshot, checkUpdates);
}

function bindSettingsView(
  snapshot: Snapshot,
  refreshSnapshot: (rerender?: boolean) => Promise<void>,
  checkUpdates: (silent?: boolean) => Promise<void>,
): void {
  document.querySelector<HTMLFormElement>("#codex-settings-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const execution = document.querySelector<HTMLInputElement>('input[name="execution-mode"]:checked')?.value ?? "default";
    const settings = {
      model: valueOf("setting-model"),
      model_reasoning_effort: valueOfSelect("setting-reasoning"),
      execution_mode: execution,
      web_search: valueOfSelect("setting-web-search"),
      personality: valueOfSelect("setting-personality"),
    };
    try {
      await call("save_codex_settings", { settings });
      setStatus("Codex 设置已保存", "success");
      await refreshSnapshot();
    } catch (error) {
      setStatus(String(error), "error");
    }
  });
  document.querySelector<HTMLButtonElement>("#save-config")?.addEventListener("click", async () => {
    const configToml = document.querySelector<HTMLTextAreaElement>("#config-editor")?.value ?? "";
    try {
      await call("save_config", { configToml });
      setStatus("高级配置已保存", "success");
      await refreshSnapshot();
    } catch (error) {
      setStatus(String(error), "error");
    }
  });
  document.querySelector<HTMLButtonElement>("#check-update")?.addEventListener("click", () => void checkUpdates());
  document.querySelector<HTMLButtonElement>("#copy-codex-home")?.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(snapshot.code_home);
      setStatus("CODEX_HOME 已复制", "success");
    } catch (error) {
      setStatus(String(error), "error");
    }
  });
  document.querySelector<HTMLButtonElement>("#desktop-update-action")?.addEventListener("click", async () => {
    const latest = await call<Snapshot>("get_snapshot");
    const staged = latest.staged_app_updates[latest.staged_app_updates.length - 1];
    if (staged) {
      try {
        await call("apply_app_update", { path: staged });
      } catch (error) {
        setStatus(String(error), "error");
      }
      return;
    }
    const release = await call<ReleaseInfo>("check_app_update");
    if (!release.update_available) return;
    try {
      const result = await call<UpdateResult>("stage_app_update");
      setStatus(`桌面更新 ${result.version} 已暂存`, "success");
      await refreshSnapshot();
    } catch (error) {
      setStatus(String(error), "error");
    }
  });
  document.querySelector<HTMLButtonElement>("#codex-update-action")?.addEventListener("click", async () => {
    try {
      const latest = await call<Snapshot>("get_snapshot");
      const update = await call<CodexUpdateInfo>("check_codex_update");
      const pending = latest.pending_codex_versions[latest.pending_codex_versions.length - 1];
      const result = pending
        ? await call<UpdateResult>("activate_codex_update", { version: pending })
        : await call<UpdateResult>("install_codex_update", { version: update.latest_version });
      setStatus(result.kind === "codex-waiting" ? `Codex ${result.version} 等待应用` : `Codex ${result.version} 已更新`, "success");
      await refreshSnapshot();
      await checkUpdates(true);
    } catch (error) {
      setStatus(String(error), "error");
    }
  });
}

function renderUpdatePanel(snapshot: Snapshot, updateState: UpdateState): void {
  const checkButton = document.querySelector<HTMLButtonElement>("#check-update");
  const desktopSummary = document.querySelector<HTMLElement>("#desktop-update-summary");
  const codexSummary = document.querySelector<HTMLElement>("#codex-update-summary");
  const desktopButton = document.querySelector<HTMLButtonElement>("#desktop-update-action");
  const codexButton = document.querySelector<HTMLButtonElement>("#codex-update-action");
  const updateBadge = document.querySelector<HTMLElement>("#nav-update-status");
  if (updateBadge) {
    const pending = snapshot.staged_app_updates.length > 0
      || snapshot.pending_codex_versions.length > 0
      || Boolean(updateState.release?.update_available || updateState.codex?.update_available);
    updateBadge.textContent = pending ? "可用" : "";
    updateBadge.hidden = !pending;
  }
  if (!checkButton || !desktopSummary || !codexSummary || !desktopButton || !codexButton) return;

  setButtonContent(checkButton, updateState.checking ? "正在检查" : updateState.checkedAt ? "再次检查" : "检查更新", "refresh-cw");
  checkButton.disabled = updateState.checking;

  const release = updateState.release;
  const desktopStaged = snapshot.staged_app_updates.length > 0;
  if (updateState.checking) desktopSummary.textContent = "正在检查";
  else if (desktopStaged) desktopSummary.textContent = `${snapshot.staged_app_updates[snapshot.staged_app_updates.length - 1]} 已暂存`;
  else if (release?.tag_name) {
    desktopSummary.textContent = release.update_available ? `${release.tag_name} 可用` : "已是最新版本";
  } else if (release) desktopSummary.textContent = "GitHub Releases 暂无发布版本";
  else desktopSummary.textContent = "尚未检查";
  if (desktopStaged) {
    setButtonContent(desktopButton, snapshot.terminals.length > 0 ? "会话运行中" : "重启并应用", "download");
    desktopButton.disabled = snapshot.terminals.length > 0;
  } else {
    setButtonContent(desktopButton, release?.update_available ? "准备更新" : "暂无更新", "download");
    desktopButton.disabled = updateState.checking || !release?.update_available;
  }

  const pendingCodex = snapshot.pending_codex_versions[snapshot.pending_codex_versions.length - 1];
  if (updateState.checking) codexSummary.textContent = "正在检查";
  else if (pendingCodex) codexSummary.textContent = `${pendingCodex} 已下载`;
  else if (updateState.codex) {
    codexSummary.textContent = updateState.codex.update_available
      ? `${updateState.codex.current_version ?? "未安装"} → ${updateState.codex.latest_version}`
      : `最新版本 ${updateState.codex.latest_version}`;
  } else codexSummary.textContent = "尚未检查";
  if (pendingCodex) {
    setButtonContent(codexButton, snapshot.terminals.length > 0 ? "会话运行中" : "应用 Codex 更新", "download");
    codexButton.disabled = snapshot.terminals.length > 0;
  } else {
    setButtonContent(codexButton, updateState.codex?.update_available ? "更新 Codex" : "暂无更新", "download");
    codexButton.disabled = updateState.checking || !updateState.codex?.update_available;
  }
  mountIcons();
}

function setButtonContent(button: HTMLButtonElement, label: string, icon: "download" | "refresh-cw"): void {
  button.innerHTML = `<i data-lucide="${icon}"></i><span>${escapeHtml(label)}</span>`;
}
window.addEventListener("DOMContentLoaded", () => {
  void waitForBridgeReady().then(() => renderControl()).catch((error) => {
    if (app) app.innerHTML = `<main class="fatal-error"><h1>tauri-codex 启动失败</h1><p>${escapeHtml(String(error))}</p></main>`;
  });
});
