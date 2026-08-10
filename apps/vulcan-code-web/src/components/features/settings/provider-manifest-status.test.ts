import { describe, expect, it } from "vitest";

import {
  activeLayerLabel,
  legacyBindingPendingLabel,
  manifestFieldSourceLabel,
  manifestModelBindingChangeLabel,
  manifestModelBindingIdentity,
  manifestModelSourceLabel,
  manifestProvenanceStatusLabel,
  ruleProvenanceSummary,
  shortHash,
  syncOutcomeLabel,
  syncStatusSummary,
  templateInstanceSummary,
} from "./provider-manifest-status";

describe("provider-manifest-status helpers", () => {
  // Maps every active-layer identity to its stable label.
  // 把每个 active 层身份映射到稳定标签。
  it("labels every active layer", () => {
    expect(activeLayerLabel("bundled")).toBe("Bundled");
    expect(activeLayerLabel("active")).toBe("Active");
    expect(activeLayerLabel("custom")).toBe("Custom Local");
    expect(activeLayerLabel("previous")).toBe("Previous");
  });

  // Maps every closed manifest provenance state without browser-side inference.
  // 映射每个封闭 Manifest 来源状态，不在浏览器侧推断。
  it("labels every manifest provenance state", () => {
    expect(manifestProvenanceStatusLabel("verified")).toBe("已验证");
    expect(manifestProvenanceStatusLabel("migrated")).toBe("已迁移");
    expect(manifestProvenanceStatusLabel("example_only")).toBe("仅示例");
  });

  // Truncates long hashes and falls back for absent values.
  // 截断长摘要并对缺失值回退。
  it("shortens hashes and handles absent values", () => {
    expect(shortHash("a".repeat(64))).toBe(`${"a".repeat(12)}…`);
    expect(shortHash("short")).toBe("short");
    expect(shortHash(undefined)).toBe("—");
  });

  // Explains the exact inheritance winner behind one effective protocol.
  // 解释一个有效协议背后的精确继承胜者。
  it("renders field-level provenance", () => {
    expect(manifestFieldSourceLabel("service_entry_override")).toBe("入口覆盖");
    expect(
      ruleProvenanceSummary({
        source: "model_rule_exact",
        rule_id: "model-a",
        manifest_revision: 7,
      }),
    ).toBe("精确规则 · model-a · r7");
    expect(ruleProvenanceSummary(undefined)).toBe("未声明有效协议");
  });

  // Maps every backend-owned model inventory source without browser-side inference.
  // 映射每个后端持有的模型目录来源，不在浏览器侧推导。
  it("labels every model inventory source", () => {
    expect(manifestModelSourceLabel("manifest")).toBe("Manifest");
    expect(manifestModelSourceLabel("discovery")).toBe("发现");
    expect(manifestModelSourceLabel("manual")).toBe("手工");
    expect(manifestModelSourceLabel("merged")).toBe("合并");
  });

  // Preserves every exact backend binding-identity dimension without semantic inference.
  // 保留后端绑定身份的每个精确维度，不进行语义推导。
  it("serializes exact model binding identities", () => {
    expect(manifestModelBindingIdentity({
      service_entry_id: "official-api",
      credential_profile_id: "api-key",
      connection_id: "account-a",
      model_id: "deepseek-chat",
    })).toBe('["official-api","api-key","account-a","deepseek-chat"]');
    expect(manifestModelBindingIdentity({
      service_entry_id: "official-api",
      credential_profile_id: undefined,
      connection_id: undefined,
      model_id: "deepseek-chat",
    })).toBe('["official-api",null,null,"deepseek-chat"]');
  });

  // Maps every backend-classified active model lifecycle change to its visible badge.
  // 把每种后端分类的 active 模型生命周期变化映射为可见徽标。
  it("labels active model binding changes", () => {
    expect(manifestModelBindingChangeLabel("added")).toBe("新增");
    expect(manifestModelBindingChangeLabel("changed")).toBe("变更");
    expect(manifestModelBindingChangeLabel("removed")).toBe("退役");
  });

  // Maps every sync outcome to a stable label (plan §R10 未配置/成功/拒绝).
  // 把每个同步结果映射到稳定标签（方案 §R10 未配置/成功/拒绝）。
  it("labels every sync outcome", () => {
    expect(syncOutcomeLabel("not_configured")).toBe("未配置");
    expect(syncOutcomeLabel("up_to_date")).toBe("已最新");
    expect(syncOutcomeLabel("activated")).toBe("已激活");
    expect(syncOutcomeLabel("rejected")).toBe("已拒绝");
    expect(syncOutcomeLabel("not_pinned")).toBe("未固定");
  });

  // Reports unconfigured before any sync state is read.
  // 在任何同步状态读取前报告未配置。
  it("reports unconfigured without sync state", () => {
    const summary = syncStatusSummary({ sync_configured: false, sync_state: undefined, quarantine: [] });
    expect(summary.kind).toBe("unconfigured");
    expect(summary.text).toBe("未配置");
  });

  // Surfaces a classified rejection with its quarantine count.
  // 暴露带隔离计数的分类拒绝。
  it("surfaces classified rejection", () => {
    const summary = syncStatusSummary({
      sync_configured: true,
      sync_state: { revision: 2, sha256: "x", last_error_kind: "signature_invalid", synced_at_unix_ms: 0 },
      quarantine: [{ revision: 2, failure_kind: "signature_invalid", quarantined_at_unix_ms: 0 }],
    });
    expect(summary.kind).toBe("rejected");
    expect(summary.text).toBe("signature_invalid");
    expect(summary.quarantineCount).toBe(1);
  });

  // Uses the newest quarantine classification before any successful state exists.
  // 在任何成功状态存在前使用最新隔离分类。
  it("surfaces first-sync rejection from quarantine", () => {
    const summary = syncStatusSummary({
      sync_configured: true,
      sync_state: undefined,
      quarantine: [
        { revision: 2, failure_kind: "schema_semantic_failed", quarantined_at_unix_ms: 20 },
        { revision: 1, failure_kind: "integrity_failed", quarantined_at_unix_ms: 10 },
      ],
    });
    expect(summary.kind).toBe("rejected");
    expect(summary.text).toBe("schema_semantic_failed");
    expect(summary.quarantineCount).toBe(2);
  });

  // Renders a last-sync time after a successful sync and never fakes success before one.
  // 在成功同步后渲染最近同步时间，且从未同步时绝不伪造成功。
  it("renders last sync time and never fakes success", () => {
    const never = syncStatusSummary({ sync_configured: true, sync_state: undefined, quarantine: [] });
    expect(never.kind).toBe("ok");
    expect(never.text).toBe("从未同步");
    const synced = syncStatusSummary({
      sync_configured: true,
      sync_state: { revision: 1, sha256: "y", synced_at_unix_ms: 1_700_000_000_000 },
      quarantine: [],
    });
    expect(synced.kind).toBe("ok");
    expect(synced.text).toContain("最近激活");
  });

  // Keeps a failed latest check distinct from the earlier successful activation timestamp.
  // 保持失败的最近检查与更早的成功激活时间戳相互独立。
  it("separates latest check from latest activation", () => {
    const summary = syncStatusSummary({
      sync_configured: true,
      sync_state: { revision: 2, sha256: "z", synced_at_unix_ms: 1_700_000_000_000 },
      sync_check_state: {
        checked_at_unix_ms: 1_700_000_100_000,
        last_successful_at_unix_ms: 1_700_000_000_000,
        outcome: "failed",
        last_error_kind: "index_check_failed",
      },
      quarantine: [],
    });
    expect(summary.kind).toBe("rejected");
    expect(summary.text).toBe("index_check_failed");
  });

  // Exposes the intentional APIKEY.FUN compatibility boundary without marking migrated providers.
  // 暴露有意保留的 APIKEY.FUN 兼容边界，且不会错误标记已迁移供应商。
  it("labels only a pending legacy binding", () => {
    expect(legacyBindingPendingLabel(true)).toBe("Legacy binding pending");
    expect(legacyBindingPendingLabel(false)).toBeNull();
  });

  // Renders every identity in one Alibaba template-generated tool target.
  // 渲染一条 Alibaba 模板生成工具目标中的全部身份信息。
  it("summarizes a concrete Alibaba template instance", () => {
    expect(templateInstanceSummary({
      target_id: "alibaba.model-studio-cn.qwen-image-generate.qwen-image-2.0-pro",
      template_id: "alibaba.model-studio-cn.qwen-image-generate.v1",
      service_entry_id: "model-studio-cn",
      model_id: "qwen-image-2.0-pro",
      model_display_name: "Qwen Image 2.0 Pro",
    })).toBe(
      "template alibaba.model-studio-cn.qwen-image-generate.v1 · model-studio-cn/qwen-image-2.0-pro · Qwen Image 2.0 Pro · target alibaba.model-studio-cn.qwen-image-generate.qwen-image-2.0-pro",
    );
  });
});
