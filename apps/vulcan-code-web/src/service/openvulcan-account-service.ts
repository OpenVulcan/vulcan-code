import { serviceClient } from "@/service/webui-service-runtime";
import type { PublicAiOAuthChallenge, PublicAiOAuthTransaction, PublicAiSettingsSnapshot } from "@/types";

// OpenVulcanRegion selects one independent user center; it never falls back across regions.
// OpenVulcanRegion 选择一个独立用户中心；绝不跨区域回退。
export type OpenVulcanRegion = "china" | "global";

// OpenVulcanAccountSnapshot contains public account state shared by all projects, without secrets.
// OpenVulcanAccountSnapshot 包含所有工程共享的公开账户状态，不含秘密。
export interface OpenVulcanAccountSnapshot {
  // Whether the stored account credential can currently be used.
  // 已保存账户凭据当前是否可用。
  connected: boolean;
  // Exact global connection identity, absent after sign-out.
  // 全局连接的准确身份；退出后不存在。
  connectionId: string | null;
  // Verified username saved at sign-in.
  // 登录时验证并保存的用户名。
  displayName: string | null;
  // User-selected independent service region.
  // 用户选择的独立服务区域。
  region: OpenVulcanRegion | null;
  // English service diagnostic, if account renewal failed.
  // 账户续期失败时的英文服务诊断。
  message: string | null;
}

// loadOpenVulcanAccount returns the current global account and performs due renewal on the host.
// loadOpenVulcanAccount 返回当前全局账户，并在宿主执行到期续期。
export function loadOpenVulcanAccount(): Promise<OpenVulcanAccountSnapshot> {
  return serviceClient.request("service_openvulcan_account");
}

// claimOpenVulcanPrompt returns true for the first installation-wide prompt claim only.
// claimOpenVulcanPrompt 仅为整个安装的首次提示认领返回 true。
export function claimOpenVulcanPrompt(): Promise<boolean> {
  return serviceClient.request("service_claim_openvulcan_prompt");
}

// startOpenVulcanLogin opens the selected region's browser flow and returns its exact challenge.
// startOpenVulcanLogin 打开所选区域的浏览器流程，并返回其准确挑战。
export function startOpenVulcanLogin(region: OpenVulcanRegion): Promise<PublicAiOAuthChallenge> {
  return serviceClient.request("service_start_openvulcan_login", { region });
}

// openVulcanLoginStatus observes transactionId without exposing its credential.
// openVulcanLoginStatus 观察 transactionId，不暴露其凭据。
export function openVulcanLoginStatus(transactionId: string): Promise<PublicAiOAuthTransaction> {
  return serviceClient.request("service_openvulcan_login_status", { transactionId });
}

// cancelOpenVulcanLogin cancels only transactionId, so stale windows cannot cancel newer logins.
// cancelOpenVulcanLogin 仅取消 transactionId，使过期窗口无法取消更新的登录。
export function cancelOpenVulcanLogin(transactionId: string): Promise<void> {
  return serviceClient.request("service_cancel_openvulcan_login", { transactionId });
}

// commitOpenVulcanLogin validates transactionId and returns the activated global provider settings.
// commitOpenVulcanLogin 验证 transactionId，并返回已激活的全局供应商设置。
export function commitOpenVulcanLogin(transactionId: string): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request("service_commit_openvulcan_login", { transactionId });
}

// logoutOpenVulcanAccount removes the global credential before attempting remote device revocation.
// logoutOpenVulcanAccount 先删除全局凭据，再尝试远程撤销设备。
export function logoutOpenVulcanAccount(): Promise<void> {
  return serviceClient.request("service_logout_openvulcan_account");
}

// OpenVulcanUsagePage retains the server's hourly ordering, exact decimal strings and continuation.
// OpenVulcanUsagePage 保留服务端小时排序、准确十进制字符串及续页游标。
export interface OpenVulcanUsagePage {
  // Inclusive server-owned date interval.
  // 服务端持有的包含式日期区间。
  from_date: string;
  // Inclusive final date of the server-owned page interval.
  // 服务端页面区间的包含式结束日期。
  to_date: string;
  // Opaque cursor; an empty value proves the final page.
  // 不透明游标；空值表示末页。
  next_cursor: string;
  // Public hourly aggregates; decimal strings never pass through Number.
  // 公开小时聚合；十进制字符串绝不经过 Number。
  usage: Array<{
    // Exact UTC hour and stable public model identity.
    // 准确 UTC 小时及稳定公开模型身份。
    hour_start: string;
    // Public model identity, independent of upstream routing secrets.
    // 公开模型身份，与上游路由秘密无关。
    model_id: string;
    // Exact request outcome counters.
    // 准确请求结果计数。
    request_count: string;
    // Exact successful-request count.
    // 准确成功请求数。
    successful_requests: string;
    // Exact failed-request count.
    // 准确失败请求数。
    failed_requests: string;
    // Actual input tokens and charge in millionths of a point.
    // 实际输入 Token 及以百万分之一积分表示的扣费。
    input_tokens: string;
    // Exact charge in integer millionths of a point.
    // 以百万分之一积分为整数单位的准确扣费。
    charged_micro_points: string;
  }>;
}

// loadOpenVulcanUsagePage returns one bounded page for the global account and opaque cursor.
// loadOpenVulcanUsagePage 根据全局账户和不透明 cursor 返回一页有界记录。
export function loadOpenVulcanUsagePage(cursor: string): Promise<OpenVulcanUsagePage> {
  return serviceClient.request("service_openvulcan_usage_page", { cursor });
}

// OpenVulcanEdgeUsage is the account's official direct judge calls per model, with every model the
// deployment serves listed, sorted by id.
// OpenVulcanEdgeUsage 是账户按模型的官方直连判定调用，并列出部署提供的每个模型，按 id 排序。
export interface OpenVulcanEdgeUsage {
  // Window of the totals in hours, ending at the current hour.
  // 合计的时间窗（小时），截至当前小时。
  hours: number;
  models: Array<{
    // Full edge model id.
    // 完整的边缘模型 id。
    model_id: string;
    // Whether the deployment still serves the model.
    // 部署是否仍提供该模型。
    enabled: boolean;
    // Calls, judged items and failed calls across all of the account's clients.
    // 该账户全部客户端的调用次数、判定条数与失败次数。
    requests: number;
    items: number;
    errors: number;
    // RFC 3339 start of the latest hour with calls; absent when the window has none.
    // 最近一个有调用的小时的 RFC 3339 起点；时间窗内没有调用时缺省。
    last_hour?: string;
  }>;
}

// loadOpenVulcanEdgeUsage reads the signed-in account's official direct judge calls per model.
// loadOpenVulcanEdgeUsage 读取已登录账户按模型的官方直连判定调用。
export function loadOpenVulcanEdgeUsage(): Promise<OpenVulcanEdgeUsage> {
  return serviceClient.request("service_openvulcan_edge_usage", {});
}
