// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { toast } from "@/ui-kit/primitives/toast";
import { useWorkbenchShellChrome } from "./shell-chrome";
import { copyText } from "./types";

// nativeShell isolates native window effects while preserving actual hook lifecycle behavior.
// nativeShell 隔离原生窗口副作用，同时保留真实钩子生命周期行为。
const nativeShell = vi.hoisted(() => ({
  supportsWindowControls: vi.fn(() => true),
  isWindowMaximized: vi.fn(async () => false),
  toggleWindowMaximize: vi.fn(async () => {}),
  subscribeWindowResize: vi.fn(async (_handler: () => void) => () => {}),
}));
vi.mock("@/service/webui-service-runtime", () => ({ shellClient: nativeShell }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn() } }));
vi.mock("@/components/shell/app-chrome", () => ({
  // GlobalActions and WindowControls render nothing; this suite covers the hook's native window state.
  // GlobalActions 与 WindowControls 不渲染内容；本套件覆盖钩子的原生窗口状态。
  GlobalActions: () => null,
  WindowControls: () => null,
}));

// state exposes only public hook output; host and root own the isolated React mount.
// state 仅暴露钩子的公开输出；host 和 root 持有隔离的 React 挂载。
let state: ReturnType<typeof useWorkbenchShellChrome>;
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.clearAllMocks();
  nativeShell.subscribeWindowResize.mockImplementation(async () => () => {});
  nativeShell.isWindowMaximized.mockResolvedValue(false);
  nativeShell.toggleWindowMaximize.mockResolvedValue(undefined);
  nativeShell.supportsWindowControls.mockReturnValue(true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove();
  vi.restoreAllMocks(); vi.unstubAllGlobals();
  document.documentElement.classList.remove("dark");
});

// Harness renders actual shell state with a complete synthetic startup snapshot.
// Harness 使用完整合成启动快照渲染真实壳层状态。
// Parameters: none. Returns: the shell actions owned by the tested hook.
// 参数：无。返回值：被测钩子持有的壳层操作界面。
function Harness() {
  state = useWorkbenchShellChrome({ startup: { ready: true, phase: "ready", steps: [], recentLogs: [] }, locale: "zh-CN", onChangeLocale: () => {} });
  return state.shellChromeActions;
}

// mount uses the real stable translation provider so rerenders retain normal effect dependencies.
// mount 使用真实稳定翻译上下文，使重新渲染保持正常副作用依赖。
// Parameters: none. Returns: completion of the isolated React render.
// 参数：无。返回值：隔离 React 渲染完成的异步结果。
async function mount() { await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><Harness /></I18nProvider>)); }

it("releases native subscriptions that finish after unmount", async () => {
  // release and resolveSubscription model delayed native listener registration.
  // release 和 resolveSubscription 模拟延迟的原生监听器注册。
  const release = vi.fn();
  let resolveSubscription!: (value: () => void) => void;
  nativeShell.subscribeWindowResize.mockImplementation(() => new Promise((resolve) => { resolveSubscription = resolve; }));
  await mount();
  await act(async () => root.render(null));
  await act(async () => resolveSubscription(release));
  expect(release).toHaveBeenCalledTimes(1);
});

it("rejects duplicate titlebar toggles and reports failure without an unhandled promise", async () => {
  // rejectToggle holds one native command so two synchronous activations can be compared.
  // rejectToggle 挂起一个原生命令，以比较两次同步触发。
  let rejectToggle!: (error: Error) => void;
  nativeShell.toggleWindowMaximize.mockImplementation(() => new Promise((_, reject) => { rejectToggle = reject; }));
  await mount();
  // titlebarEvent contains only the target property consumed by the actual handler.
  // titlebarEvent 仅包含真实处理器消费的 target 属性。
  const titlebarEvent = { target: host } as unknown as React.MouseEvent<HTMLElement>;
  await act(async () => { state.handleTitlebarDoubleClick(titlebarEvent); state.handleTitlebarDoubleClick(titlebarEvent); });
  expect(nativeShell.toggleWindowMaximize).toHaveBeenCalledTimes(1);
  await act(async () => rejectToggle(new Error("Synthetic native denial")));
  expect(toast.error).toHaveBeenCalledTimes(1);
});

it("never reports successful copying when the clipboard API is absent or rejects", async () => {
  vi.stubGlobal("navigator", { clipboard: undefined });
  await expect(copyText("full transcript")).rejects.toThrow("unavailable");
  // writeText records exact text and returns a synthetic platform rejection.
  // writeText 记录准确文本并返回合成平台拒绝。
  const writeText = vi.fn().mockRejectedValue(new Error("denied"));
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  await expect(copyText("full transcript")).rejects.toThrow("denied");
  expect(writeText).toHaveBeenCalledExactlyOnceWith("full transcript");
});
