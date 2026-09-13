import type { VmmMemoryActionResponse, VmmNativeRecallPolicySummary, VmmNativeRuntimePolicySummary } from "../../src/types";
import { extractionDefaults } from "../../src/components/features/memory/vmm-native-extraction-policy.test-fixture";
import { rankingDefaults } from "../../src/components/features/memory/vmm-native-retrieval-policy.test-fixture";
import { retentionDefaults } from "../../src/components/features/memory/vmm-native-retention-settings.test-fixture";
import { textDefaults } from "../../src/components/features/memory/vmm-native-text-policy.test-fixture";
import { profileDefaults } from "../../src/components/features/memory/vmm-native-profile-policy.test-fixture";
import { nativeCatalog } from "./fixtures";
import { memorySessionRead } from "./memory-session-fixtures";

// recall is an explicit synthetic policy using the production typed extraction fixture, never a claimed live default.
// recall 是使用生产强类型提炼夹具的明确合成策略，绝不宣称为实时默认。
const recall: VmmNativeRecallPolicySummary = { precheckHistoryTurnLimit: 3, precheckMaxInputTokens: 8000, precheckRecallTopK: 5,
  precheckReviewCandidateLimit: 20, precheckMinReviewScore: .5, maxSearchQueries: 3, postactionHistoryTurnLimit: 3,
  postactionMaxInputTokens: 12000, ...extractionDefaults };
// policy keeps current values and explicit reset values independently cloned, matching actual policy editors.
// policy 将当前值与显式重置值分别克隆，与实际策略编辑器匹配。
export const policy: VmmNativeRuntimePolicySummary = { recall, defaults: structuredClone(recall), retrieval: rankingDefaults,
  retrievalDefaults: structuredClone(rankingDefaults), retention: retentionDefaults, retentionDefaults: structuredClone(retentionDefaults),
  text: textDefaults, textDefaults: structuredClone(textDefaults), profiles: profileDefaults, profileDefaults: structuredClone(profileDefaults), userOverride: false };

// memoryRead handles only verified read contracts; callers reject undefined rather than fabricating a successful action.
// memoryRead 仅处理已核实读取契约；调用方拒绝未定义结果，不伪造成功动作。
// Parameters: action and payload are the exact memory RPC fields. Returns: a typed read response or unsupported marker.
// 参数：action 与 payload 是精确记忆 RPC 字段。返回值：强类型读取响应或未支持标记。
export function memoryRead(action: string, payload: Record<string, unknown>): VmmMemoryActionResponse | undefined {
  if (action === "native_project_catalog") return { kind: "native_project_catalog", value: nativeCatalog };
  if (action === "native_runtime_policy") return { kind: "native_runtime_policy", value: policy };
  if (action === "native_host_project_policy" && typeof payload.hostProjectId === "string") return { kind: "native_host_project_policy", value: { hostProjectId: payload.hostProjectId, overrides: { recall: null, retrieval: null, text: null }, effective: policy } };
  if (action === "load_users_projects") return { kind: "users_projects", value: { users: [], projects: [] } };
  // Native profile bundles resolve the exact catalog project reference and use the protocol's Full representation.
  // 原生画像组合解析准确目录项目引用，并使用协议的 Full 表示。
  if (action === "load_profile_bundle" && typeof payload.projectRef === "string" && nativeCatalog.projects.some((project) => String(project.projectId) === payload.projectRef))
    return { kind: "profile_bundle", value: { mode: "PROFILE_BUNDLE_MODE_FULL", combined_text: "隔离验收画像：这是合成项目背景，不来自真实记忆数据。",
      project_profile: "", user_profile: "", trace_id: `ui-audit-profile-${payload.projectRef}` } };
  if (action === "native_session_list" || action === "native_memory_records") return memorySessionRead(action, payload);
  if (action === "native_profile_records" && typeof payload.vmmProjectId === "number" && (payload.target === "user" || payload.target === "project")) return { kind: "native_profile_records", value: {
    projectId: payload.vmmProjectId, storageVersion: 1, target: payload.target, counts: { active: 0, inactive: 0, pending: 0, expired: 0, scheduled: 0 }, nodes: [] } };
  return undefined;
}
