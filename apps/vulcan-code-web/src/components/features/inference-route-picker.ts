import type { ComboboxOption } from "@/ui-kit/primitives/combobox";
import type { PublicAiInferenceRoute, PublicAiInternalInferenceCandidate } from "@/types";

// Exact-identity helpers for every consumer-owned inference route picker.
// 供每个消费者自有推理路由选择器使用的精确身份助手。
//
// Selecting a route by model display name would pick a connection the user never
// reviewed, so these helpers derive a stable key from declared ownership and binding
// only. They live here rather than in one feature because the public-AI inference
// contract is shared by every consumer that owns a route.
// 按模型展示名选择路由会选中用户从未审查过的连接，因此这些助手只从已声明的归属与绑定推导
// 稳定键。它们放在此处而不是某个功能目录，因为公共 AI 推理契约由每个持有路由的消费者共享。

// Identify an exact route without interpreting display labels or parsing an encoded key.
// 标识精确路由，不解释展示标签或解析编码键。
// route contains declared ownership and binding; returns a stable picker value.
// route 包含已声明归属与绑定；返回稳定选择器值。
export function inferenceRouteKey(route: PublicAiInferenceRoute): string {
  // Binding variants are explicitly declared by the shared inference contract.
  // 绑定分支由共享推理契约明确声明。
  const binding = route.binding.kind === "protocol"
    ? `protocol:${route.binding.protocol_profile_id}` : `action:${route.binding.action_binding_id}`;
  return [route.provider_id, route.service_entry_id, route.connection_id, route.model_id,
    binding, route.operation, route.usage_scope].join("\u0000");
}

// Build common searchable options from backend-validated route candidates.
// 从后端已验证路由候选构建通用可搜索选项。
// candidates supply model/account/provider metadata; returns exact-identity picker options.
// candidates 提供模型、账号及供应商元数据；返回精确身份选择器选项。
export function inferenceRoutePickerOptions(
  candidates: PublicAiInternalInferenceCandidate[],
): ComboboxOption[] {
  return candidates.map((candidate) => {
    // Binding identity remains searchable without becoming a presentation-based routing shortcut.
    // 绑定身份保持可搜索，但不会成为按展示名称路由的捷径。
    const bindingId = candidate.route.binding.kind === "protocol"
      ? candidate.route.binding.protocol_profile_id : candidate.route.binding.action_binding_id;
    return { value: inferenceRouteKey(candidate.route),
      label: `${candidate.modelDisplayName} · ${candidate.connectionDisplayName}`,
      description: `${candidate.providerDisplayName} · ${candidate.serviceEntryDisplayName}`,
      keywords: [candidate.route.model_id, candidate.providerDisplayName, candidate.route.provider_id,
        candidate.connectionDisplayName, candidate.route.connection_id, candidate.serviceEntryDisplayName,
        candidate.route.service_entry_id, bindingId] };
  });
}
