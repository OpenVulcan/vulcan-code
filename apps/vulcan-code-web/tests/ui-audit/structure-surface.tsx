import React from "react";
import { I18nProvider } from "../../src/i18n";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "../../src/components/ui/tooltip";
import { Button } from "../../src/components/ui/button";
import { ComposerGoalDetails } from "../../src/components/composer/composer-panel/goal-details";
import { ComposerSessionProgressCapsule } from "../../src/components/composer/composer-panel/progress-capsule";
import { ProviderModelModalitySummary } from "../../src/components/features/settings/provider-model-modality-summary";
import { UsagePanel, type UsagePanelViewModel } from "../../src/components/product/usage/usage-panel";
import { AdaptiveToolPanel } from "../../src/components/workspace/adaptive-tool-panel";
import { ConversationMeasuredCollapse } from "../../src/components/conversation/conversation-measured-collapse";
import { ChangeSetDiffPreview } from "../../src/components/conversation/conversation-change-set-diff-preview";
import { parseUnifiedDiff } from "../../src/diff/parseDiff";
import type { PublicAiModel, SessionGoalSummary } from "../../src/types";
import "../../src/styles/settings-pages.css";

// goal is synthetic and never persisted or submitted to a model.
// goal 为合成数据，绝不持久化或提交模型。
const goal: SessionGoalSummary = {
  sessionId: "audit", goalId: "audit", objective: "完整目标\n" + "LongObjective无空格".repeat(16), status: "active",
  tokenBudget: 20_000, tokensUsed: 100, usageTokens: 100, apiCallCount: 2, toolCallCount: 1,
  inputTokens: 50, cachedInputTokens: 450, outputTokens: 50, reasoningTokens: 10, totalTokens: 550,
  timeUsedSeconds: 2, createdAtMs: 1, updatedAtMs: 2,
};
// model declares exact capability types to exercise the production icon projection.
// model 声明精确能力类型，用于验证生产图标投影。
const model: PublicAiModel = { id: "audit", display_name: "Audit", capabilities: {
  evidence_revision: "audit", reasoning: "unsupported", model_parameters: [],
  prompt_cache: { provider_default_behavior: "provider_default", supports_managed_explicit: false, supported_ttl_options: [] },
  operations: ["conversation_respond"], input_resources: ["image", "audio", "video", "file"],
} };
// usage exercises all declared read states with exact preformatted synthetic values.
// usage 使用精确预格式化合成值覆盖全部声明读取状态。
const usage: UsagePanelViewModel[] = [
  { state: "ready", title: "已知用量", metrics: [{ id: "tokens", label: "Tokens", value: "9,223,372,036,854,775,807" }],
    cost: { label: "费用", amount: "-" }, modelsTitle: "模型用量",
    models: [{ id: "long", label: "VeryLongModelName".repeat(8), tokens: "9,223,372,036,854,775,807 Tokens", requests: "100 requests" }] },
  { state: "empty", title: "空状态", stateMessage: "暂无用量" },
  { state: "loading", title: "加载状态", stateMessage: "正在读取用量" },
  { state: "partial", title: "部分可用", stateLabel: "部分", metrics: [{ id: "zero", label: "Tokens", value: "0" }] },
  { state: "error", title: "读取失败", stateMessage: "合成读取失败" },
];
// diff is parsed locally and contains markup-like text to verify escaped rendering.
// diff 在本地解析，并包含类似标记的文本以验证转义渲染。
const diff = parseUnifiedDiff("diff --git a/audit.txt b/audit.txt\n--- a/audit.txt\n+++ b/audit.txt\n@@ -1 +1 @@\n-<img src=x onerror=alert(1)>\n+safe <script>alert(1)</script>\n").files[0];
if (!diff) throw new Error("Audit diff fixture is empty");

/** Renders production structural components with local controls and no host mutations.
 * 渲染生产结构组件及本地控件，不修改宿主状态。
 * No parameters; returns the exact requested gallery, collapse, or adaptive layout.
 * 无参数；返回精确请求的图集、折叠区或自适应布局。
 */
export function AuditStructureSurface() {
  // query selects only declared visual scenarios on the isolated origin.
  // query 仅选择隔离源中声明的视觉场景。
  const query = new URLSearchParams(window.location.search);
  // kind chooses the inspected component family, not a production route.
  // kind 选择被检查的组件族，不表示生产路由。
  const kind = query.get("structure") ?? "gallery";
  if (!["gallery", "collapse", "adaptive", "progress"].includes(kind)) throw new Error(`Unknown structure audit: ${kind}`);
  // expanded controls only the local fold or side panel.
  // expanded 仅控制本地折叠或侧栏。
  const [expanded, setExpanded] = React.useState(false);
  // extended changes child height while the production collapse remains open.
  // extended 在生产折叠区保持展开时改变子项高度。
  const [extended, setExtended] = React.useState(false);
  return <I18nProvider initialLocale={query.get("locale") === "en-US" ? "en-US" : "zh-CN"}><TooltipProvider>
    <main style={{ padding: 16, height: "100vh", overflow: "auto" }}>
      <p>结构组件合成验收 · 无模型调用与配置写入</p>
      {kind === "progress" ? <>
        <Button onClick={() => setExtended(!extended)} variant="outline">切换任务数量</Button>
        <div className="workspace-statusbar-todolist-status" style={{ position: "fixed", bottom: 16, left: 16, maxWidth: "calc(100vw - 32px)" }}>
          <ComposerSessionProgressCapsule reviewSummary={null} todolist={{
            totalCount: extended ? 40 : 3, completedCount: 1,
            items: Array.from({ length: extended ? 40 : 3 }, (_, index) => ({ step: `步骤 ${index + 1} ${"LongUnbroken说明".repeat(5)}`, status: index === 0 ? "completed" : index === 1 ? "in_progress" : "pending" })),
          }} />
        </div>
      </> : kind === "gallery" ? <div style={{ display: "grid", gap: 16, gridTemplateColumns: "repeat(auto-fit, minmax(min(320px, 100%), 1fr))" }}>
        <div style={{ minWidth: 0 }}>
          <Tooltip><TooltipTrigger asChild><Button variant="outline">目标详情</Button></TooltipTrigger>
            <TooltipContent className="context-usage-tooltip composer-goal-details-tooltip" side="top" sideOffset={6} showArrow={false}><ComposerGoalDetails goal={goal} /></TooltipContent>
          </Tooltip>
          <ProviderModelModalitySummary model={model} /><ChangeSetDiffPreview file={diff} />
        </div>
        {usage.map((entry) => <UsagePanel key={entry.state} model={entry} />)}
      </div> : kind === "collapse" ? <section data-conversation-surface-id="audit-collapse">
        <Button onClick={() => setExpanded(!expanded)} aria-expanded={expanded}>切换折叠</Button>
        <Button onClick={() => setExtended(!extended)} variant="outline">切换正文高度</Button>
        <ConversationMeasuredCollapse collapseId="audit-fold" expanded={expanded}>
          <p style={{ padding: 12, margin: 0 }}>{("测试正文 long content\n").repeat(extended ? 100 : 4)}</p>
        </ConversationMeasuredCollapse>
        <p>折叠区后方内容</p>
      </section> : <div style={{ height: "calc(100% - 40px)" }}>
        <AdaptiveToolPanel side="right" expanded={expanded} panel={<div style={{ width: expanded ? 176 : 80, background: "var(--card)", padding: 8 }}>
          <Button onClick={() => setExpanded(!expanded)} aria-expanded={expanded} variant="outline">侧栏</Button>
          {expanded ? <p>面板内容</p> : null}
        </div>}><p style={{ maxWidth: 800 }}>居中正文。宽视口预留侧栏空间；通用面板在窄视口按原有规则纵向排列。</p></AdaptiveToolPanel>
      </div>}
    </main>
  </TooltipProvider></I18nProvider>;
}
