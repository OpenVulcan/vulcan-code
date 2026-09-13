import React from "react";
import { I18nProvider } from "../../src/i18n";
import { Button } from "../../src/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "../../src/components/ui/dropdown-menu";
import { ComposerKnowledgeControl } from "../../src/components/composer/composer-knowledge-control";
import { ModelSecondaryRouteControl } from "../../src/components/composer/composer-panel/model-secondary-route-control";
import { ComposerAiCapabilityControl } from "../../src/components/composer/composer-capability-controls";
import { ComposerPanelToolbarRight } from "../../src/components/composer/composer-panel/toolbar-right";
import { ComposerPanelAttachmentPreviewStrip } from "../../src/components/composer/composer-panel/attachment-preview-strip";
import { ComposerAttachmentMenu } from "../../src/components/composer/composer-attachment-controls";
import type { ThinkingDisplayMode } from "../../src/components/conversation/thinking-display";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import type { ComposerProviderToolSettingsSnapshot } from "../../src/components/composer/composer-model";
import { publicAi, workbench } from "./fixtures";
import type { ComposerAttachmentSummary, ContextCompactionWindowNodeId, ProviderModelSecondaryRouteSummary } from "../../src/types";

// previewAttachments contain synthetic local identities; only the thumbnail resolver supplies inline fixture pixels.
// previewAttachments 包含合成本地身份；仅缩略图解析器提供内联夹具图像。
const previewAttachments: ComposerAttachmentSummary[] = [
  { assetId: "audit-thumbnail", kind: "image", fileName: "原始图片.png", localPath: "synthetic/original.png", previewLocalPath: "synthetic/preview.png", mimeType: "image/png" },
  { assetId: "audit-upload", kind: "video", fileName: "LongUpload文件名".repeat(7) + ".mp4", localPath: "synthetic/video.mp4", mimeType: "video/mp4" },
  { assetId: "audit-file", kind: "file", fileName: "notes.txt", localPath: "synthetic/notes.txt", mimeType: "text/plain" },
];
// thumbnailFixture is a small code-authored preview, with no host asset or network dependency.
// thumbnailFixture 是代码绘制的小预览图，不依赖宿主资产或网络。
const thumbnailFixture = "data:image/svg+xml," + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="120" height="90"><rect width="120" height="90" fill="#327e66"/><circle cx="60" cy="45" r="24" fill="#edf8ed"/><text x="60" y="52" text-anchor="middle" font-size="22">P</text></svg>');

// routes contain only synthetic provider/model identities; callbacks never contact any provider.
// routes 仅包含合成供应商和模型身份；回调从不联系任何供应商。
const routes: ProviderModelSecondaryRouteSummary[] = Array.from({ length: 40 }, (_, index) => ({
  providerId: "audit-provider", modelId: "audit-model", routeId: `route-${index}`, tag: `T${index}`,
  providerName: "Synthetic provider", endpointName: `endpoint-${index}`, label: index === 1 ? "LongUnbroken名称".repeat(12) : `线路 ${index + 1}`,
  subtitle: `Endpoint ${index} · Synthetic only`,
}));

/** Mounts exact production selectors with explicit local callbacks and isolated read fixtures.
 * 挂载精确生产选择器，使用显式本地回调及隔离读取夹具。
 * No parameters; returns the independently usable composer-control acceptance surface.
 * 无参数；返回可独立使用的输入区控件验收界面。
 */
export function AuditComposerSurface() {
  // query declares the fixture language without changing a production preference.
  // query 声明夹具语言，不修改生产偏好。
  const query = new URLSearchParams(window.location.search);
  // draft stores only this mounted page's explicit knowledge selection.
  // draft 仅保存此挂载页面的显式知识库选择。
  const [draft, setDraft] = React.useState<{ mode: "inherit" | "explicit"; libraryIds: string[] }>({ mode: "inherit", libraryIds: [] });
  // selectedRoute mirrors one local success, while readState and rejectWrite exercise declared failures.
  // selectedRoute 对齐一次本地成功，readState 与 rejectWrite 验证已声明失败。
  const [selectedRoute, setSelectedRoute] = React.useState<ProviderModelSecondaryRouteSummary | null>(null);
  const [readState, setReadState] = React.useState<"ready" | "empty" | "error">("ready");
  const [rejectWrite, setRejectWrite] = React.useState(false);
  // managed records only activation of the management navigation callback.
  // managed 仅记录管理导航回调被激活。
  const [managed, setManaged] = React.useState(false);
  // failKnowledge selects one explicitly rejected read identity within the isolated transport.
  // failKnowledge 在隔离传输中选择一个明确拒绝读取的身份。
  const [failKnowledge, setFailKnowledge] = React.useState(false);
  // failCapabilities selects an explicitly rejected synthetic session read, never a real session.
  // failCapabilities 选择明确拒绝的合成会话读取，绝不使用真实会话。
  const [failCapabilities, setFailCapabilities] = React.useState(false);
  // capacityNode and reasoning are local acceptance choices, never persisted to the host.
  // capacityNode 与 reasoning 是本地验收选择，绝不写入宿主。
  const [capacityNode, setCapacityNode] = React.useState<ContextCompactionWindowNodeId | null>(null);
  const [reasoning, setReasoning] = React.useState("high");
  // reasoningAvailable and reasoningSignal exercise actual selector availability and slash-command opens.
  // reasoningAvailable 与 reasoningSignal 验证真实选择器可用性及斜杠指令打开。
  const [reasoningAvailable, setReasoningAvailable] = React.useState(true);
  const [reasoningSignal, setReasoningSignal] = React.useState(0);
  // stopState supplies only controlled visual states; stop request timing is verified with the complete production composer test.
  // stopState 仅提供受控视觉状态；停止请求时序由完整生产输入区测试验证。
  const [stopState, setStopState] = React.useState<"idle" | "running" | "stopping">("idle");
  // removedFiles, openedFile and retriedFile record exact visual acceptance actions without uploading or deleting host files.
  // removedFiles、openedFile 与 retriedFile 记录精确视觉验收操作，不上传或删除宿主文件。
  const [removedFiles, setRemovedFiles] = React.useState<string[]>([]);
  const [openedFile, setOpenedFile] = React.useState<string | null>(null);
  const [retriedFile, setRetriedFile] = React.useState<string | null>(null);
  // menuState records only controlled add-menu choices; no callback opens a host picker or saves policy.
  // menuState 仅记录受控加号菜单选择；所有回调都不打开宿主选择器或保存策略。
  const [menuState, setMenuState] = React.useState<{
    thinking: ThinkingDisplayMode; plan: boolean; style: string | null; precheck: string; writeback: string;
    picked: string | null; goalOpened: boolean; memoryOpened: boolean;
  }>({ thinking: "popup_only", plan: false, style: "structured", precheck: "enabled", writeback: "disabled", picked: null, goalOpened: false, memoryOpened: false });
  // memoryAvailable and stylePending independently expose the explicit configuration and activation gates.
  // memoryAvailable 与 stylePending 独立暴露明确的配置及生效门控。
  const [memoryAvailable, setMemoryAvailable] = React.useState(true);
  const [stylePending, setStylePending] = React.useState(false);
  // visibleAttachments is the exact ordered list consumed by the production index-based removal callback.
  // visibleAttachments 是生产索引移除回调使用的精确有序列表。
  const visibleAttachments = previewAttachments.filter((attachment) => !removedFiles.includes(attachment.localPath));
  // capabilityDraft contains only synthetic stale choices; saving updates this visible local output.
  // capabilityDraft 仅包含合成失效选择；保存只更新此可见本地输出。
  const [capabilityDraft, setCapabilityDraft] = React.useState<ComposerProviderToolSettingsSnapshot>({
    enabledCapabilities: publicAi.providerToolEnabledCapabilities, selections: publicAi.providerToolSelections,
    mediaUnderstandingSelections: publicAi.mediaUnderstandingSelections ?? {},
    extensionTargetSets: { image_generate: [
      { connection_id: "removed-account-1", target_id: "removed-model" },
      { connection_id: "removed-account-2", target_id: "LongUnavailableModel名称".repeat(12) },
    ] },
  });
  return <I18nProvider initialLocale={query.get("locale") === "en-US" ? "en-US" : "zh-CN"}>
    <main style={{ padding: 16, maxWidth: 800 }}>
      <p>输入区合成验收 · 无模型调用、无宿主配置写入</p>
      <section aria-label="附件验收"><TooltipProvider delayDuration={0}>
        <ComposerPanelAttachmentPreviewStrip attachments={visibleAttachments} preparations={{ "audit-upload": {
          attachmentId: "audit-upload", blocksSubmission: true, deliveryPlan: "upload_reference", generation: 1,
          phase: "failed", reason: "upload", error: "SyntheticUploadFailure".repeat(12), transferId: "audit-transfer",
        } }} progresses={{}}
          resolveMediaSourceUrl={(path) => path === "synthetic/preview.png" ? thumbnailFixture : "data:image/png;base64,"}
          onOpenMedia={(attachment) => setOpenedFile(attachment.localPath)}
          onRemove={(index) => setRemovedFiles((current) => [...current, visibleAttachments[index].localPath])}
          onRetry={setRetriedFile} />
      </TooltipProvider></section>
      <TooltipProvider><ComposerPanelToolbarRight draftSlotKey="audit-capacity" attachmentCount={0} draftInput="" isConversationRunning={stopState !== "idle"} stopPending={stopState === "stopping"}
        sendBlocked contextUsage={{ baselineMatchesActiveRuntime: true, breakdown: [], cacheHitBasisPoints: null, maxTokens: 320_000,
          percent: 42, severity: "normal", tooltipRows: [], tooltipTitle: "容量验收", usageBasisPoints: 4200, usedTokens: 134_400 }}
        sessionProviderUsage={{ providerCallCount: 4, inFlightCallCount: 0, unknownUsageCallCount: 2, cumulativeUsage: null, lastUsage: { totalTokens: 900 }, usageRevision: 1 }}
        contextCompactionWindowNodes={publicAi.contextCompactionWindowNodes.nodes} sessionContextCompactionWindowNodeId={capacityNode} modelContextLimit={320_000}
        onSetSessionContextCompactionWindow={async (node) => { if (rejectWrite) throw new Error("Synthetic capacity write failure · " + "long details ".repeat(70)); setCapacityNode(node); return true; }}
        reasoningAriaLabel="推理验收" reasoningEffort={reasoning} reasoningEffortOptions={reasoningAvailable ? [{ value: "low", label: "低 · Low" }, { value: "high", label: "高 · High" }] : [{ value: reasoning, label: "固定档位" }]}
        reasoningPickerOpenSignal={reasoningSignal} onReasoningEffortChange={setReasoning} onSend={() => setStopState("stopping")}
        selectFallbackLabel="选择" sendAriaLabel="发送（夹具禁用）" stopAriaLabel="停止（仅视觉验收）" /></TooltipProvider>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBlock: 16 }}>
        <TooltipProvider><ComposerAttachmentMenu
          capabilities={["image", "video", "audio", "pdf", "file"].map((kind) => ({ kind, label: kind, enabled: true, native: true, bridged: false, accept: "" }))}
          thinkingDisplayMode={menuState.thinking} planModeEnabled={menuState.plan} sessionVmmPolicy={{ precheckMode: menuState.precheck, memoryWritebackMode: menuState.writeback }}
          vmmAvailable={memoryAvailable} vmmStatus={memoryAvailable ? "not_ready" : "disabled"}
          onOpenMemorySettings={() => setMenuState((current) => ({ ...current, memoryOpened: true }))}
          onPick={(kind) => setMenuState((current) => ({ ...current, picked: kind }))}
          onThinkingDisplayModeChange={(thinking) => setMenuState((current) => ({ ...current, thinking }))}
          onPlanModeChange={(plan) => setMenuState((current) => ({ ...current, plan }))}
          onVmmPolicyChange={(policy, mode) => setMenuState((current) => ({ ...current, [policy]: mode }))}
          onOpenGoal={() => setMenuState((current) => ({ ...current, goalOpened: true }))}
          projectId={null} conversationId={stylePending ? "audit-style" : null} onOpenKnowledge={() => setManaged(true)}
          outputStyleCatalog={workbench.outputStyleCatalog} draftOutputStyleId={menuState.style}
          sessionOutputStyle={stylePending ? { overrideStyleId: menuState.style, desiredStyleId: menuState.style ?? workbench.outputStyleCatalog.selectedId, pendingActivation: true } : null}
          onOutputStyleChange={(style) => setMenuState((current) => ({ ...current, style }))} /></TooltipProvider>
        <TooltipProvider><ComposerAiCapabilityControl ariaLabel="扩展能力验收" conversationId={failCapabilities ? "audit-capabilities-read-failure" : null}
          draftSettings={capabilityDraft} onDraftSettingsChange={setCapabilityDraft} /></TooltipProvider>
        <DropdownMenu><DropdownMenuTrigger asChild><Button variant="outline">加号菜单</Button></DropdownMenuTrigger>
          <DropdownMenuContent><ComposerKnowledgeControl projectId={failKnowledge ? "audit-knowledge-read-failure" : "audit"} conversationId={null} draftSelection={draft}
            onDraftSelectionChange={setDraft} onOpenKnowledge={() => setManaged(true)} /></DropdownMenuContent>
        </DropdownMenu>
        <ModelSecondaryRouteControl providerId="audit-provider" modelId="audit-model" selectedRoute={selectedRoute}
          onListRoutes={async () => { if (readState === "error") throw new Error("Synthetic discovery failure · " + "long details ".repeat(12)); return readState === "empty" ? [] : routes; }}
          onSelectRoute={async (_provider, _model, selection) => {
            if (rejectWrite) throw new Error("Synthetic route write rejected");
            setSelectedRoute(selection ? routes.find((entry) => entry.routeId === selection.routeId)! : null); return true;
          }} />
      </div>
      <fieldset><legend>夹具读取状态</legend><div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
        {(["ready", "empty", "error"] as const).map((state) => <Button key={state} variant="outline" aria-pressed={readState === state} onClick={() => setReadState(state)}>{state}</Button>)}
        <Button variant="outline" aria-pressed={rejectWrite} onClick={() => setRejectWrite(!rejectWrite)}>切换写入拒绝</Button>
        <Button variant="outline" aria-pressed={failKnowledge} onClick={() => { setFailKnowledge(!failKnowledge); setDraft({ mode: "inherit", libraryIds: [] }); }}>切换知识库读取失败</Button>
        <Button variant="outline" aria-pressed={failCapabilities} onClick={() => setFailCapabilities(!failCapabilities)}>切换能力读取失败</Button>
        <Button variant="outline" onClick={() => setReasoningAvailable(!reasoningAvailable)}>切换推理可用性</Button>
        <Button variant="outline" onClick={() => setReasoningSignal(reasoningSignal + 1)}>触发推理指令</Button>
        <Button variant="outline" onClick={() => setCapacityNode("compression-window-5")}>设为失效容量节点</Button>
        <Button variant="outline" onClick={() => setStopState("running")}>显示可停止状态</Button>
        <Button variant="outline" onClick={() => setStopState("idle")}>复位停止状态</Button>
        <Button variant="outline" onClick={() => { setRemovedFiles([]); setOpenedFile(null); setRetriedFile(null); }}>复位附件状态</Button>
        <Button variant="outline" aria-pressed={memoryAvailable} onClick={() => setMemoryAvailable(!memoryAvailable)}>切换记忆可用性</Button>
        <Button variant="outline" aria-pressed={stylePending} onClick={() => setStylePending(!stylePending)}>切换输出风格等待生效</Button>
      </div></fieldset>
      <pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{JSON.stringify({ draft, route: selectedRoute?.routeId ?? null, managed, capacityNode, reasoning, stopState, openedFile, retriedFile, removedFiles, menuState, capabilityDraft }, null, 2)}</pre>
    </main>
  </I18nProvider>;
}
