export interface AiCapabilityTargetSummary {
  // Stable vendor/target selector used by prompt-level overrides.
  // 提示词级覆盖使用的稳定 vendor/target 选择器。
  targetSelector: string;
  // Provider or capability vendor identifier.
  // Provider 或能力供应商标识。
  vendorId: string;
  // User-facing vendor name.
  // 面向用户显示的供应商名称。
  vendorName: string;
  // Route target alias exposed to the runtime.
  // 暴露给运行时的路由目标别名。
  targetAlias: string;
  // User-facing target name.
  // 面向用户显示的目标名称。
  targetName: string;
  // Provider model identifier used by this target.
  // 当前目标使用的 Provider 模型标识。
  modelId: string;
  // Whether this target is globally enabled.
  // 当前目标是否全局启用。
  enabled: boolean;
}

// AiCapabilitySummary describes one semantic AI capability exposed by the composer.
// AiCapabilitySummary 描述输入区暴露的单个语义 AI 能力。
export interface AiCapabilitySummary {
  // Stable semantic capability key.
  // 稳定语义能力键。
  kind: string;
  // User-facing capability label.
  // 面向用户显示的能力名称。
  label: string;
  // Whether the capability has at least one usable route.
  // 当前能力是否至少拥有一条可用路由。
  available: boolean;
  // Whether the capability is globally enabled.
  // 当前能力是否全局启用。
  enabled: boolean;
  // Whether the active provider/model handles this capability natively.
  // 当前 Provider/模型是否原生处理该能力。
  native: boolean;
  // Whether host-side AI tools bridge this capability.
  // 宿主侧 AI 工具是否桥接该能力。
  bridged: boolean;
  // Configured targets under this capability.
  // 当前能力下已配置的目标。
  targets: AiCapabilityTargetSummary[];
}

// AiCapabilityVendorSummary describes one shared AI capability vendor.
// AiCapabilityVendorSummary 描述单个共享 AI 能力供应商。
export interface AiCapabilityVendorSummary {
  // Stable vendor instance id.
  // 稳定供应商实例标识。
  id: string;
  // Stable catalog vendor id.
  // 稳定目录供应商标识。
  vendorId: string;
  // Vendor display name.
  // 供应商显示名称。
  name: string;
  // Whether the vendor is enabled.
  // 供应商是否启用。
  enabled: boolean;
  // Stored API key or environment placeholder.
  // 已保存 API Key 或环境变量占位符。
  apiKey: string;
  // Stored base URL override.
  // 已保存 Base URL 覆盖值。
  baseUrl: string;
  // Stored region override.
  // 已保存区域覆盖值。
  region: string;
  // Stored connection mode.
  // 已保存连接模式。
  connectionMode: string;
  // Configured target count.
  // 已配置目标数量。
  targetCount: number;
  // Catalog target choices exposed by this vendor.
  // 该供应商暴露的目录目标选项。
  targetChoices?: AiCapabilityVendorTargetChoiceSummary[];
}

// AiCapabilityVendorTargetChoiceSummary describes one selectable target below a configured vendor.
// AiCapabilityVendorTargetChoiceSummary 描述已配置供应商下单个可选择目标。
export interface AiCapabilityVendorTargetChoiceSummary {
  // Stable semantic capability kind owned by this target.
  // 当前目标所属的稳定语义能力类型。
  capabilityKind: string;
  // Stable target alias under the vendor.
  // 供应商下的稳定目标别名。
  targetAlias: string;
  // Default provider request model attached to the target.
  // 当前目标默认绑定的 Provider 请求模型。
  defaultModelId: string;
  // User-facing compact label.
  // 面向用户的紧凑标签。
  label: string;
  // Whether this target supports editing request model.
  // 当前目标是否支持编辑请求模型。
  supportsModelOverride: boolean;
  // Optional finite model choices.
  // 可选的有限模型选项。
  modelOptions: string[];
}

// AiCapabilityVendorCatalogSummary describes one supported vendor picker choice.
// AiCapabilityVendorCatalogSummary 描述单个受支持供应商选择项。
export interface AiCapabilityVendorCatalogSummary {
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string;
  // Vendor display name.
  // 供应商显示名称。
  name: string;
  // Supported capability labels.
  // 支持的能力标签。
  capabilities: string[];
}
