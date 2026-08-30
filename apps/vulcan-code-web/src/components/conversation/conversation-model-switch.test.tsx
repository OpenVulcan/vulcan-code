import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import { ModelSwitchNotice, modelSwitchRouteLabel } from "./conversation-model-switch";
import { requestConfigChangeKind } from "@/service/conversation-event-display-projection";
import { ConversationV1DisplayRenderer } from "./conversation-v1-display-renderer";
import type { ProviderSummary } from "@/types";

// providers mirrors the account-scoped workbench contract, including a disabled historical model.
// providers 对应账号作用域的工作台契约，包含一个已禁用的历史模型。
const providers: ProviderSummary[] = [{
  id: "connection-internal-id",
  sourceProviderId: "custom-provider-id",
  connectionId: "connection-internal-id",
  name: "我的供应商 · 私有账号",
  providerDisplayName: "我的供应商",
  status: "ready",
  models: [],
  catalogModels: [{ id: "model-internal-id", name: "显示模型", enabled: false }],
}];

// The route intentionally references an old credential; provider identity still resolves its visible name.
// 路线刻意引用旧凭据；供应商身份仍能解析可见名称。
const route = { provider: "custom-provider-id", model: "model-internal-id", connection_id: "deleted-connection" };

describe("model switch presentation", () => {
  // Parameters: none. Returns: none; verifies configured names exclude every credential identity.
  // 参数：无。返回值：无；验证配置名称不包含任何凭据身份。
  it("uses configured provider/model names even after credential rotation", () => {
    expect(modelSwitchRouteLabel(route, providers)).toBe("我的供应商 / 显示模型");
  });

  // Parameters: none. Returns: none; verifies exact IDs survive missing configuration.
  // 参数：无。返回值：无；验证配置丢失时保留精确 ID。
  it("uses the recorded provider ID only when no configured provider row remains", () => {
    expect(modelSwitchRouteLabel(route, [])).toBe("custom-provider-id / model-internal-id");
  });

  // Parameters: none. Returns: none; verifies first initialization and credential changes are not model switches.
  // 参数：无。返回值：无；验证首次初始化与凭据变化不属于模型切换。
  it("hides initial selection and credential-only changes", () => {
    expect(requestConfigChangeKind({ kind: "request_config_changed", previous: {}, current: route })).toBe("none");
    expect(requestConfigChangeKind({ kind: "request_config_changed", previous: route, current: { ...route, connection_id: "new-connection" } })).toBe("none");
    expect(requestConfigChangeKind({ kind: "request_config_changed", previous: route, current: { ...route, thinking_level: "high" } })).toBe("thinking");
  });

  // Parameters: none. Returns: none; verifies no empty notice survives in the event-native renderer.
  // 参数：无。返回值：无；验证事件原生渲染器不会残留首次配置的空提示。
  it("does not render the initial route from older persisted history", () => {
    // markup renders the old initialization fact without creating a misleading switch row.
    // markup 渲染旧初始化事实，但不产生误导的切换行。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationV1DisplayRenderer events={[{
      seq: 1, turn_id: null, created_at_ms: 1, updated_at_ms: 1, event_revision: 1, visibility: "ui_only",
      event: { kind: "request_config_changed", previous: {}, current: route },
    }]} providers={providers} /></I18nProvider>);
    expect(markup).not.toContain("conversation-model-switch");
    expect(markup).not.toContain("request_config_changed");
  });

  // Parameters: none. Returns: none; verifies the shared row has only one sentence and an accessible help trigger.
  // 参数：无。返回值：无；验证共享行仅含一句说明及可访问的帮助触发器。
  it("renders a compact separator rather than an expandable detail card", () => {
    // markup uses the same component as both conversation renderers.
    // markup 使用两个会话渲染器共用的同一个组件。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ModelSwitchNotice summary="模型已从 供应商甲 / 甲模型 切换为 供应商乙 / 乙模型。" /></I18nProvider>);
    expect(markup).toContain("conversation-model-switch-label");
    expect(markup).toContain('aria-label="模型切换说明"');
    expect(markup).not.toContain("special-block");
    expect(markup).not.toContain("aria-expanded");
  });
});
