import { CircleAlert } from "lucide-react";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";
import type { ProviderSummary } from "@/types";

// RequestConfig is the persisted route, not an account row or a mutable current selection.
// RequestConfig 是持久化路线，不是账号行或可变的当前选择。
type RequestConfig = Extract<ConversationEventSummary["event"], { kind: "request_config_changed" }>["current"];

// modelSwitchRouteLabel resolves a persisted provider/model against the current display catalog.
// modelSwitchRouteLabel 使用当前显示目录解析持久化的供应商和模型。
// Parameters: config is the recorded route; providers contains configured account-scoped catalog rows.
// 参数：config 是记录的路线；providers 包含已配置的账号作用域目录行。
// Returns: provider/model names; a deleted provider or missing model retains its exact recorded ID.
// 返回值：供应商和模型名称；已删除供应商或缺失模型保留精确记录的 ID。
export function modelSwitchRouteLabel(config: RequestConfig, providers: readonly ProviderSummary[]): string {
  // rows match provider identity, so credential changes do not hide an existing provider name.
  // rows 按供应商身份匹配，因此凭据变化不会隐藏仍存在的供应商名称。
  const rows = providers.filter((provider) => (provider.sourceProviderId ?? provider.id) === config.provider);
  // providerName excludes account suffixes; legacy provider-scoped rows carry the name directly.
  // providerName 不含账号后缀；旧的供应商作用域行直接携带名称。
  const providerName = rows.length > 0 ? (rows[0].providerDisplayName ?? rows[0].name) : (config.provider ?? "—");
  // model uses the full catalog, including models currently disabled in the picker.
  // model 使用完整目录，也包含当前在选择器中被禁用的模型。
  const model = rows.flatMap((provider) => provider.catalogModels ?? provider.models ?? [])
    .find((candidate) => candidate.id === config.model);
  return `${providerName} / ${model?.name ?? config.model ?? "—"}`;
}

// ModelSwitchNotice renders model and reasoning changes as a non-interactive separator.
// ModelSwitchNotice 将模型与思考强度变化渲染为不可交互的分隔提示。
// Parameters: summary is the localized before/after sentence; seq optionally identifies the canonical event.
// 参数：summary 是本地化前后切换句子；seq 可选地标识规范事件。
// Returns: a compact notice without account IDs, expandable cards, or detail dialogs.
// 返回值：不含账号 ID、展开卡片或详情弹窗的紧凑提示。
export function ModelSwitchNotice(props: { summary: string; seq?: number }) {
  return (
    <div className="conversation-model-switch" role="note" data-event-kind="request_config_changed" data-seq={props.seq}>
      <CircleAlert size={14} aria-hidden="true" />
      <span className="conversation-model-switch-label">{props.summary}</span>
    </div>
  );
}
