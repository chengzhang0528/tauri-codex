import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createIcons, Download, RefreshCw } from "lucide";
import "./launcher.css";

type LauncherStatus = {
  phase: string;
  component: string;
  downloaded: number;
  total: number;
  error: string | null;
  running: boolean;
};

const root = document.querySelector<HTMLElement>("#launcher");

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function render(status: LauncherStatus): void {
  if (!root) return;
  const hasBytes = status.total > 0;
  const progress = hasBytes ? Math.min(100, Math.round(status.downloaded / status.total * 100)) : 0;
  root.innerHTML = `
    <section class="launcher-shell">
      <div class="product-mark">TC</div>
      <div class="launcher-copy">
        <p class="eyebrow">tauri-codex</p>
        <h1>${status.error ? "组件准备失败" : "正在准备桌面应用"}</h1>
        <p class="phase">${escapeHtml(status.error ?? status.phase)}</p>
      </div>
      <div class="progress-block" aria-live="polite">
        <div class="progress-meta"><span>${escapeHtml(status.component || "初始化")}</span><span>${hasBytes ? `${formatBytes(status.downloaded)} / ${formatBytes(status.total)}` : ""}</span></div>
        <div class="progress-track"><span style="width:${hasBytes ? progress : 100}%" class="${hasBytes ? "" : "indeterminate"}"></span></div>
      </div>
      ${status.error ? '<button id="retry" type="button"><i data-lucide="refresh-cw"></i><span>重试</span></button>' : '<div class="status-note"><i data-lucide="download"></i><span>完成后将自动启动应用</span></div>'}
    </section>`;
  createIcons({ icons: { Download, RefreshCw } });
  document.querySelector<HTMLButtonElement>("#retry")?.addEventListener("click", async () => {
    try {
      await invoke("retry_launcher_setup");
    } catch (error) {
      render({ ...status, error: String(error), running: false });
    }
  });
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>'"]/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character] ?? character);
}

window.addEventListener("DOMContentLoaded", async () => {
  try {
    await listen<LauncherStatus>("launcher-status", (event) => render(event.payload));
    render(await invoke<LauncherStatus>("get_launcher_status"));
  } catch (error) {
    render({ phase: "Launcher 初始化失败", component: "桌面应用", downloaded: 0, total: 0, error: String(error), running: false });
  }
});
