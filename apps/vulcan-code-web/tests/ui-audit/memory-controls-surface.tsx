import React from "react";
import { I18nProvider, useI18n } from "../../src/i18n";
import { Button } from "../../src/components/ui/button";
import { VmmNativeDurationInput } from "../../src/components/features/memory/vmm-native-duration-input";
import { VmmNativeLexicalIndex } from "../../src/components/features/memory/vmm-native-lexical-index";
import { VmmNativeCleanupPreview } from "../../src/components/features/memory/vmm-native-cleanup-preview";
import { VmmSelectField, vmmPolicyModeOptions } from "../../src/components/features/memory/vmm-project-settings-fields";
import { VmmSearchRetrieval } from "../../src/components/features/memory/vmm-search-retrieval";
import type { VmmNativeRetentionPreview } from "../../src/types";

/** Exercises five memory controls using local state and synthetic outcomes only.
 * 仅以本地状态及合成结果验证五个记忆控件。
 * No parameters; returns the localized fixture without making service requests.
 * 无参数；返回本地化夹具，不发送服务请求。
 */
export function AuditMemoryControlsSurface() {
  return <I18nProvider initialLocale="zh-CN"><MemoryControls /></I18nProvider>;
}

/** Owns local control state; no callbacks mutate a real memory project.
 * 持有本地控件状态；全部回调均不修改真实记忆项目。
 * No parameters; returns the selected synthetic control surface and readable callback log.
 * 无参数；返回选中的合成控件界面及可读回调日志。
 */
function MemoryControls() {
  // t supplies the same policy option labels used by production consumers.
  // t 提供生产消费方使用的相同策略选项文案。
  const { t } = useI18n();
  // query chooses long content and confirmation-only surfaces explicitly.
  // query 明确选择长内容及独立确认界面。
  const query = new URLSearchParams(window.location.search);
  // longName deliberately contains an uninterrupted synthetic identity.
  // longName 刻意包含连续的合成身份文本。
  const longName = query.get("content") === "long" ? "SyntheticMemoryProject".repeat(18) : "合成记忆项目";
  // duration is the sole integer-millisecond draft, never persisted by this fixture.
  // duration 是唯一整数毫秒草稿，本夹具绝不持久化。
  const [duration, setDuration] = React.useState("86400000");
  // policy uses the declared three-state option values.
  // policy 使用已声明的三态选项值。
  const [policy, setPolicy] = React.useState("inherit");
  // disabled exercises the shared busy state without holding any real action channel.
  // disabled 验证共用忙碌状态，不持有真实动作通道。
  const [disabled, setDisabled] = React.useState(false);
  // available distinguishes a missing index status from real zero counts.
  // available 区分缺失索引状态与真实零计数。
  const [available, setAvailable] = React.useState(true);
  // rejected selects a declared null result from the parent action contract.
  // rejected 选择父级动作契约中已声明的空结果。
  const [rejected, setRejected] = React.useState(false);
  // observed exercises the actual provenance visibility guard.
  // observed 验证实际来源信息的观察状态守卫。
  const [observed, setObserved] = React.useState(true);
  // log exposes exact local callback activity without accessing application state.
  // log 展示准确本地回调活动，不访问应用状态。
  const [log, setLog] = React.useState("无操作");
  // returnControl models the exact initiating control used by the production maintenance parent.
  // returnControl 模拟生产维护父级使用的准确发起控件。
  const returnControl = React.useRef<HTMLButtonElement>(null);
  // preview is an explicit synthetic server-shaped cleanup plan.
  // preview 是明确合成的服务端形状清洗计划。
  const [preview, setPreview] = React.useState<VmmNativeRetentionPreview | null>(() => query.get("entry") === "cleanup" ? {
    previewId: "audit-preview", storageVersion: 7, evaluatedAtUnixMs: 1000, expiresAtUnixMs: 301000,
    projects: Array.from({ length: 12 }, (_, index) => ({ projectId: index + 1, projectName: `${longName} ${index + 1}`,
      recycledMemoryIds: [`memory-${index}-` + "x".repeat(120)], recycledTurnIds: [], expiredProfileNodeIds: [], purgedBatchIds: [] })),
  } : null);
  return <main className="h-dvh overflow-auto bg-background p-4 text-foreground">
    <div className="mx-auto max-w-2xl space-y-5">
      <h1>记忆控件隔离验收</h1>
      <div className="flex flex-wrap gap-2">
        <Button ref={returnControl} onClick={() => setDisabled(!disabled)}>切换禁用</Button>
        <Button onClick={() => setAvailable(!available)}>切换状态缺失</Button>
        <Button onClick={() => setRejected(!rejected)}>切换拒绝结果</Button>
        <Button onClick={() => setObserved(!observed)}>切换情境观察</Button>
        <Button onClick={() => setDuration("60000")}>重置为一分钟</Button>
      </div>
      <label htmlFor="audit-duration">保留时长</label>
      <VmmNativeDurationInput id="audit-duration" value={duration} disabled={disabled} onChange={setDuration} />
      <output aria-label="毫秒草稿">{duration}</output>
      <VmmSelectField label="项目策略" value={policy} disabled={disabled} options={vmmPolicyModeOptions(t)} onChange={setPolicy} />
      <output aria-label="策略草稿">{policy}</output>
      <VmmSearchRetrieval retrieval={{ source_kind: longName, origin: "lexical_rerank", origin_label: longName,
        origin_explanation: "合成来源说明", final_score: 0.875, context_observed: observed, support_count: 9,
        rebuttal_count: 5, matched_context_support_count: 2, matched_context_rebuttal_count: 1,
        matched_context_score_delta: -0.010000000000005, matched_context_values: [longName] }} />
      <VmmNativeLexicalIndex busy={disabled} status={available ? { storageVersion: 7, activeMemoryCount: 0, documentCount: 0, indexedRowCount: 0 } : null}
        onRunAction={async (action, payload) => {
          setLog(`${action} ${JSON.stringify(payload)}`);
          if (rejected) return null;
          return { action, summary: "", kind: "native_lexical_rebuild", request: payload, response: {}, data: { kind: "native_lexical_rebuild", value: { storageVersion: 7, reindexedMemoryCount: 0 } } };
        }} onRefresh={async () => true} />
      <output aria-label="回调日志" className="block break-all">{log}</output>
      <VmmNativeCleanupPreview preview={preview} busy={disabled} onRestoreFocus={() => returnControl.current?.focus()} onClose={() => { setPreview(null); setLog("仅取消合成预览"); }} onConfirm={() => { setPreview(null); setLog("仅确认合成预览"); }} />
    </div>
  </main>;
}
