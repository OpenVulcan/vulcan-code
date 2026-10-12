import { createContext } from "react";
import type { OpenVulcanAccountSnapshot, OpenVulcanRegion } from "@/service/openvulcan-account-service";

// OpenVulcanAccountError retains the actual failed operation without parsing diagnostic prose.
// OpenVulcanAccountError 保留实际失败操作，不解析诊断文案。
export interface OpenVulcanAccountError {
  // The operation that failed determines localized recovery guidance.
  // 失败操作决定本地化恢复说明。
  stage: "load" | "start" | "authorize" | "connect" | "logout";
  // Original service diagnostic, displayed only in the disclosure.
  // 原始服务诊断，仅在展开区显示。
  detail: string;
  // The chosen deployment, retained only for retrying this exact login.
  // 所选部署，仅用于重试此次准确登录。
  region: OpenVulcanRegion | null;
}

// OpenVulcanAccountController exposes public account actions to the shell and provider settings.
// OpenVulcanAccountController 向外壳与供应商设置提供公开账户操作。
export interface OpenVulcanAccountController {
  // Current host-owned public account observation.
  // 当前由宿主持有的公开账户观测。
  snapshot: OpenVulcanAccountSnapshot | null;
  // Whether initial account onboarding has been resolved.
  // 初始账户引导是否已解析。
  initialized: boolean;
  // Whether the shared account dialog is visible.
  // 共享账户弹窗是否可见。
  open: boolean;
  // Whether a login or logout mutation is being finalized.
  // 登录或退出变更是否正在完成。
  busy: boolean;
  // Whether the browser authorization is waiting for the user.
  // 浏览器授权是否正在等待用户。
  waiting: boolean;
  // Latest service diagnostic for the visible action.
  // 可见操作的最近服务诊断。
  error: OpenVulcanAccountError | null;
  // Re-reads the host account state without starting a new authorization.
  // 重读宿主账户状态，不开始新的授权。
  refresh: () => Promise<void>;
  // Opens or closes the dialog; closing cancels its exact pending login.
  // 打开或关闭弹窗；关闭时取消其准确待处理登录。
  setOpen: (open: boolean) => void;
  // Starts one selected regional flow and retains its transaction identity.
  // 启动所选区域流程，并保留其事务身份。
  login: (region: OpenVulcanRegion) => Promise<void>;
  // Signs out globally and refreshes state even if remote revocation fails.
  // 全局退出；即使远程撤销失败也刷新状态。
  logout: () => Promise<void>;
}

// Null denotes a standalone surface without the application account lifecycle, such as a component preview.
// null 表示未挂载应用账户生命周期的独立界面，例如组件预览。
export const OpenVulcanAccountContext = createContext<OpenVulcanAccountController | null>(null);
