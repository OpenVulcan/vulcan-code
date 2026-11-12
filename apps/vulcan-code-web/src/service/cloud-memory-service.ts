import { serviceClient } from "@/service/webui-service-runtime"

// Names each phase supported by the memory-only API.
// 标识专用记忆 API 支持的各阶段。
export type CloudMemoryPhase = "pcl1" | "pcl2" | "ptl1" | "ptl2" | "profile_instruction" | "embedding" | "rerank"

// Describes the exact supplier, model and privacy terms for the phases it serves.
// 描述相应阶段所用的确切供应商、模型及隐私条款。
export interface CloudMemoryDisclosure {
  // Inference can cover all five phases or an explicit subset.
  // 推理可以覆盖全部五个阶段或明确指定的部分阶段。
  capability: "inference" | "embedding" | "rerank"
  purposes: CloudMemoryPhase[]
  // Values are published by the signed-in account's site.
  // 字段由已登录账号所属站点发布。
  model: string
  provider: string
  region: string
  data_policy: string
  privacy_url: string
}

// Represents one offered route and its exact consent revision.
// 表示一条已发布路由及其准确同意版本。
export interface CloudMemoryOffer {
  // Published identity, display name and consent revision.
  // 发布标识、显示名称及同意版本。
  id: string
  name: string
  consent_revision: string
  // Complete privacy disclosures for all supported phases.
  // 所有受支持阶段的完整隐私披露。
  providers: CloudMemoryDisclosure[]
}

// Holds the decisions explicitly confirmed in the picker.
// 保存用户在选择器中明确确认的决定。
export interface CloudMemoryChoice {
  // Exact offer and account context displayed before confirmation.
  // 确认前展示的准确模型和账号上下文。
  modelId: string
  consentRevision: string
  accountContext: string
  // Local filtering and optional reranking.
  // 本机过滤及可选重排。
  piiRegion: string
  rerank: boolean
}

// Separates a saved choice from current site availability.
// 区分已保存选择与站点当前可用性。
export interface CloudMemorySetup {
  // Status is derived by Core from the live account and catalog.
  // 状态由内核根据实时账号与目录得出。
  status: "available" | "selection_changed" | "signed_out" | "unavailable"
  selectedName: string | null
  selected: CloudMemoryChoice | null
  // Null when metadata cannot be read from the current site.
  // 无法从当前站点读取元数据时为空。
  accountContext: string | null
  catalog: { default_id: string; models: CloudMemoryOffer[] } | null
  // Canonical locale identifiers from the local PII resources.
  // 来自本机 PII 资源的标准地区标识。
  piiRegions: string[]
}

// Reads current cloud offers and local privacy controls without initiating inference.
// 读取当前云端模型与本机隐私控制，不触发推理。
export function loadCloudMemorySetup(): Promise<CloudMemorySetup> {
  return serviceClient.request("service_cloud_memory_setup")
}

// Stores the confirmed choice or explicitly switches native VMM to personal routes.
// 保存已确认的选择，或明确将原生 VMM 切换到自有路由。
export function saveCloudMemoryChoice(choice: CloudMemoryChoice | null): Promise<void> {
  return serviceClient.request("service_save_cloud_memory_choice", { choice })
}
