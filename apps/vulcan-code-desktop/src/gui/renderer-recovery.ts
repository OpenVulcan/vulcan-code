import { dialog, type BrowserWindow } from 'electron';
import { appendFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { desktopText, formatDesktopText, type DesktopLocale } from './desktop-text.js';
import { loadWorkbenchPage } from './workbench-page.js';

// RendererFailure contains native exit facts or an explicitly identified reload failure.
// RendererFailure 保存原生退出事实，或明确标识的重新加载失败。
interface RendererFailure {
  // reason comes from Electron; reload-failed identifies a rejected loadURL operation.
  // reason 来自 Electron；reload-failed 表示 loadURL 操作被拒绝。
  reason: Electron.RenderProcessGoneDetails['reason'] | 'reload-failed';
  // exitCode is absent only when navigation failed without a process exit.
  // 仅在导航失败而进程未退出时，exitCode 为空。
  exitCode: number | null;
}

// registerRendererRecovery installs a main-process recovery surface that survives renderer loss.
// registerRendererRecovery 安装在渲染进程退出后仍然可用的主进程恢复界面。
// Parameters: window is the workbench, webUiUrl is its trusted launch URL, dataRoot is Core's
// declared data directory, and locale selects the shared desktop catalog. Returns nothing.
// 参数：window 是工作台，webUiUrl 是可信启动地址，dataRoot 是 Core 声明的数据目录，
// locale 选择共享桌面语言目录。无返回值。
export function registerRendererRecovery(
  window: BrowserWindow,
  webUiUrl: string,
  dataRoot: string,
  locale: DesktopLocale,
): void {
  // text is resolved before a crash, so recovery never depends on the failed React tree.
  // text 在崩溃前解析，使恢复绝不依赖已失效的 React 组件树。
  const text = desktopText(locale);
  // logPath uses the authoritative data root and contains no discovery credentials or page content.
  // logPath 使用权威数据目录，不保存服务发现凭据或页面内容。
  const logPath = path.join(dataRoot, 'logs', 'desktop-renderer.jsonl');
  // pendingFailure retains a new crash even if it occurs during a requested reload.
  // pendingFailure 保留用户要求重新加载期间发生的新崩溃。
  let pendingFailure: RendererFailure | null = null;
  // recovering serializes native dialogs and prevents duplicate recovery operations.
  // recovering 串行化原生对话框，防止重复恢复操作。
  let recovering = false;
  // closing suppresses recovery during an intentional window shutdown.
  // closing 在主动关闭窗口期间禁止发起恢复。
  let closing = false;

  // recordFailure persists bounded diagnostic fields before offering recovery.
  // recordFailure 在提供恢复入口前持久化有限的诊断字段。
  // Parameter: failure contains native facts; returns nothing, reporting log I/O errors to stderr.
  // 参数：failure 包含原生事实；无返回值，日志写入错误报告到标准错误输出。
  function recordFailure(failure: RendererFailure): void {
    // record excludes URLs, tokens, prompts and conversation content.
    // record 不包含地址、令牌、提示词及会话内容。
    const record = JSON.stringify({ timestamp: new Date().toISOString(), ...failure });
    process.stderr.write(`[vulcan-code-desktop] renderer failure: ${record}\n`);
    try {
      mkdirSync(path.dirname(logPath), { recursive: true });
      appendFileSync(logPath, record + '\n', 'utf8');
    } catch (error) {
      process.stderr.write(`[vulcan-code-desktop] renderer log write failed: ${String(error)}\n`);
    }
  }

  // recover presents one native choice per failure; it never retries a model request or restarts Core.
  // recover 为每次失败提供一次原生选择；绝不重试模型请求，也不重启 Core。
  // Takes no arguments; resolves after reload succeeds or the user closes only the desktop window.
  // 无参数；重新加载成功或用户仅关闭桌面窗口后完成。
  async function recover(): Promise<void> {
    if (recovering) return;
    recovering = true;
    try {
      while (pendingFailure && !closing && !window.isDestroyed()) {
        // failure is consumed once; a crash during the dialog or reload can enqueue another one.
        // failure 只消费一次；对话框或重载期间的新崩溃可以再次排队。
        const failure: RendererFailure = pendingFailure;
        pendingFailure = null;
        // response comes from an OS dialog, independent of WebUI IPC and its unavailable buttons.
        // response 来自系统对话框，独立于 WebUI IPC 及其已失效的按钮。
        const { response } = await dialog.showMessageBox(window, {
          type: 'error',
          title: text.rendererFailureTitle,
          message: text.rendererFailureMessage,
          detail: formatDesktopText(text.rendererFailureDetail, {
            reason: failure.reason, code: failure.exitCode ?? '—', path: logPath,
          }),
          buttons: [text.reloadWorkbench, text.closeWindow],
          defaultId: 0,
          cancelId: 1,
          noLink: true,
        });
        if (closing || window.isDestroyed()) return;
        if (response !== 0) {
          window.close();
          return;
        }
        try {
          // Reload the known workbench URL; the new renderer reconnects to the existing Core.
          // 重新加载已知工作台地址；新渲染进程会重新连接既有 Core。
          await loadWorkbenchPage(window, webUiUrl);
        } catch {
          if (closing || window.isDestroyed()) return;
          // Preserve an accompanying native crash reason instead of replacing it with a generic load error.
          // 保留同时发生的原生崩溃原因，避免用通用加载错误覆盖。
          if (!pendingFailure) {
            pendingFailure = { reason: 'reload-failed', exitCode: null };
            recordFailure(pendingFailure);
          }
        }
      }
    } finally {
      recovering = false;
    }
  }

  window.on('close', () => { closing = true; });
  window.webContents.on('render-process-gone', (_event, details) => {
    if (closing || window.isDestroyed()) return;
    pendingFailure = { reason: details.reason, exitCode: details.exitCode };
    recordFailure(pendingFailure);
    void recover().catch((error: unknown) => {
      // If even the OS dialog fails, release the dead client so the next launch can create a new one.
      // 如果连系统对话框也失败，释放失效客户端，让下次启动可以创建新窗口。
      process.stderr.write(`[vulcan-code-desktop] native recovery failed: ${String(error)}\n`);
      if (!window.isDestroyed()) window.destroy();
    });
  });
}
