import React from "react";
import { Monitor, PenLine, Settings, Settings2, Shapes, SlidersHorizontal, X } from "lucide-react";

import "@/ui-kit/styles/features/session-settings.css";
import { setConversationToolCollapseOverride, useToolCollapsePreference } from "@/components/conversation/tool-collapse-preferences";
import { setConversationThinkingPopupOverride, useThinkingDisplayPreference } from "@/components/conversation/thinking-display-preferences";
import { THINKING_DISPLAY_MODE_OPTIONS, type ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { MemoryBrainIcon } from "@/components/shell/memory-brain-icon";
import { formatMessage, pickBilingualText, useI18n, type TranslationKey } from "@/i18n";
import type { OutputStyleCatalogSummary, SessionOutputStyleSummary } from "@/types";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui-kit/primitives/popover";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";

import type { ComposerAttachmentMenuProps } from "./composer-attachment-controls";
import { ComposerKnowledgeControl } from "./composer-knowledge-control";
import { SessionSettingsGroup, SessionSettingsRow } from "./composer-session-settings-parts";
import { ComposerSubagentModelsBlock } from "./composer-subagent-models";
import type { ComposerSettingsSection } from "./composer-settings-request";
import { useDedicatedComposer } from "./dedicated/dedicated-session-controls";
import type { ComposerAnswerModeController } from "./composer-panel/use-composer-answer-mode";

// LazySubagentModelsSessionDialog loads the allowlist editor and its styles only when a session edits its list.
// LazySubagentModelsSessionDialog 只在会话编辑其清单时加载白名单编辑器及其样式。
const LazySubagentModelsSessionDialog = React.lazy(() => import("@/components/features/subagent-models/subagent-models-session-dialog"));

// ComposerSettingsMenu is the session settings panel opened from the status bar. Display settings at the top apply at once
// and stay on this machine; the settings below them belong to this session and are saved with it.
// ComposerSettingsMenu 是从状态栏打开的会话设置面板。上方的显示设置立即生效且只保存在本机；下方的设置属于本会话并随会话保存。
// Parameters: props carries the active conversation or draft options; returns the trigger and its panel.
// 参数：props 携带当前会话或草稿选项；返回入口按钮及其面板。
export function ComposerSettingsMenu(props: ComposerAttachmentMenuProps) {
  const { t } = useI18n();
  // The provider identifies the selected draft as well as persisted conversations.
  // Provider 同时标识当前草稿和持久化会话。
  const toolCollapse = useToolCollapsePreference();
  // Explicit selection stays distinct from the effective global value.
  // 明确选择始终与实际生效的全局值区分。
  const thinkingPreference = useThinkingDisplayPreference(toolCollapse.scope, props.thinkingDisplayMode);
  // Settings frozen with a dedicated session (style, answer mode, knowledge, memory) are hidden.
  // 随专职会话冻结的设置（风格、回答模式、知识库、记忆）不再显示。
  const dedicated = useDedicatedComposer();
  // Slash commands open this panel, optionally at one section; closing it clears the request.
  // slash 指令可打开本面板并定位到某个区块；关闭面板即清除该请求。
  const [open, setOpen] = React.useState(false);
  const [requestedSection, setRequestedSection] = React.useState<ComposerSettingsSection | null>(null);
  // The allowlist dialog outlives the panel, which closes when the dialog opens.
  // 白名单对话框的生命周期长于面板；对话框打开时面板关闭。
  const [subagentDialogOpen, setSubagentDialogOpen] = React.useState(false);
  const body = React.useRef<HTMLDivElement>(null);
  React.useEffect(() => {
    if (props.settingsRequest) {
      setRequestedSection(props.settingsRequest.section);
      setOpen(true);
    }
  }, [props.settingsRequest]);
  // A requested section is scrolled into view and its first control takes focus once the panel has rendered it;
  // the panel's own initial focus is skipped in that case. Focus is retried for up to a second because a closing
  // layer that opened the request (the command palette) can still hold the page inert.
  // 面板渲染出请求的区块后，将其滚动到可见处并聚焦其第一个控件；此时跳过面板自身的初始聚焦。由于发起请求的
  // 正在关闭的层（命令面板）可能仍使页面处于惰性状态，聚焦会在一秒内重试。
  React.useEffect(() => {
    if (!open || !requestedSection) return;
    let frame = 0;
    const started = performance.now();
    const focusSection = () => {
      const target = body.current?.querySelector<HTMLElement>(`[data-ss-section="${requestedSection}"]`);
      const control = target?.querySelector<HTMLElement>('[tabindex="0"], button:not(:disabled):not([tabindex="-1"])');
      if (target && control) {
        target.scrollIntoView({ block: "nearest" });
        control.focus();
      }
      if (document.activeElement !== control && performance.now() - started < 1000) frame = window.requestAnimationFrame(focusSection);
    };
    frame = window.requestAnimationFrame(focusSection);
    return () => window.cancelAnimationFrame(frame);
  }, [open, requestedSection]);
  const scope = toolCollapse.scope;
  const thinkingValue = thinkingPreference.override === undefined ? "inherit" : thinkingPreference.override ? "popup_only" : "inline_output";
  const collapseValue = toolCollapse.override === undefined ? "inherit" : toolCollapse.override ? "enabled" : "disabled";
  return (<>
    <Popover open={open} onOpenChange={(next) => {
      setOpen(next);
      if (!next) setRequestedSection(null);
    }}>
      <PopoverTrigger asChild>
        <button aria-label={t("composer.sessionSettings")} className="vk-status-item" data-action="session-settings" type="button">
          <Settings /><span>{t("composer.sessionSettings")}</span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" aria-label={t("composer.sessionSettings")} className="vk-ss" onOpenAutoFocus={(event) => {
        if (requestedSection) event.preventDefault();
      }} side="top" sideOffset={8} width="md">
        <div className="vk-ss-head">
          <h3>{t("composer.sessionSettings")}</h3>
          <IconButton aria-label={t("ui.close")} onClick={() => setOpen(false)} size="sm"><X size={15} /></IconButton>
        </div>
        <div className="vk-ss-body" ref={body}>
          <section className="vk-ss-sec">
            <div className="vk-ss-title"><Monitor size={14} />{t("sessionSettings.display")}<span>{t("sessionSettings.displayHint")}</span></div>
            <SessionSettingsRow description={t("sessionSettings.thinkingHint")} section="thinking" title={t("composer.thinkingProcess")}>
              <SegmentedControl aria-label={t("composer.thinkingProcess")} disabled={scope === null} onValueChange={(value) => {
                if (value !== "inherit") props.onThinkingDisplayModeChange(value as ThinkingDisplayMode);
                else if (scope !== null) setConversationThinkingPopupOverride(scope, null);
              }} value={thinkingValue}>
                <SegmentedControlItem value="inherit">{t("composer.inheritSystem")}</SegmentedControlItem>
                {THINKING_DISPLAY_MODE_OPTIONS.map((option) => (
                  <SegmentedControlItem key={option.value} value={option.value}>{t(option.labelKey)}</SegmentedControlItem>
                ))}
              </SegmentedControl>
            </SessionSettingsRow>
            <SessionSettingsRow description={t("sessionSettings.toolsHint")} section="tools" title={t("composer.collapseTools")}>
              <SegmentedControl aria-label={t("composer.collapseTools")} disabled={scope === null} onValueChange={(value) => {
                if (scope !== null) setConversationToolCollapseOverride(scope, value === "inherit" ? null : value === "enabled");
              }} value={collapseValue}>
                <SegmentedControlItem value="inherit">{t("composer.inheritSystem")}</SegmentedControlItem>
                <SegmentedControlItem value="enabled">{t("sessionSettings.collapseOn")}</SegmentedControlItem>
                <SegmentedControlItem value="disabled">{t("sessionSettings.collapseOff")}</SegmentedControlItem>
              </SegmentedControl>
            </SessionSettingsRow>
          </section>
          {dedicated ? null : (
            <section className="vk-ss-sec">
              <div className="vk-ss-title"><SlidersHorizontal size={14} />{t("sessionSettings.session")}<span>{t("sessionSettings.sessionHint")}</span></div>
              <OutputStyleRow
                catalog={props.outputStyleCatalog}
                conversationId={props.conversationId}
                draftStyleId={props.draftOutputStyleId}
                onChange={props.onOutputStyleChange}
                sessionOutputStyle={props.sessionOutputStyle}
              />
              <AnswerModeRow controller={props.answerMode} />
              <ComposerKnowledgeControl
                conversationId={props.conversationId}
                draftSelection={props.draftKnowledgeSelection}
                onDraftSelectionChange={props.onDraftKnowledgeSelectionChange}
                onOpenKnowledge={props.onOpenKnowledge}
                projectId={props.projectId}
              />
              {props.vmmAvailable || props.onOpenMemorySettings ? (
                <SessionSettingsGroup
                  action={props.onOpenMemorySettings ? (
                    <Button className="vk-ss-sub-action" onClick={props.onOpenMemorySettings} size="xs" type="button" variant="ghost">
                      <Settings2 size={13} />{t("memory.workspace.settings")}
                    </Button>
                  ) : undefined}
                  icon={<MemoryBrainIcon size={14} />}
                  section="memory"
                  title={t("composer.vmmMemory")}
                >
                  {props.vmmStatus ? <p className="vk-ss-note">{t(`memory.workspace.${props.vmmStatus}`)}</p> : null}
                  {props.vmmAvailable ? <>
                    <VmmPolicyRow label={t("composer.vmm.precheckInjection")} onChange={props.onVmmPolicyChange} policy="precheck"
                      selectedMode={props.sessionVmmPolicy?.precheckMode ?? "inherit"} />
                    <VmmPolicyRow label={t("composer.vmm.memoryWriteback")} onChange={props.onVmmPolicyChange} policy="writeback"
                      selectedMode={props.sessionVmmPolicy?.memoryWritebackMode ?? "inherit"} />
                  </> : null}
                </SessionSettingsGroup>
              ) : null}
              <ComposerSubagentModelsBlock conversationId={props.conversationId} onEdit={() => {
                setOpen(false);
                setSubagentDialogOpen(true);
              }} />
            </section>
          )}
        </div>
      </PopoverContent>
    </Popover>
    {subagentDialogOpen && props.conversationId ? (
      <React.Suspense fallback={null}>
        <LazySubagentModelsSessionDialog conversationId={props.conversationId} onClose={() => setSubagentDialogOpen(false)}
          providers={props.providers} sessionModelLabel={props.sessionModelLabel} />
      </React.Suspense>
    ) : null}
  </>);
}

// OutputStyleRow chooses the prompt style for this session, or follows the global choice.
// OutputStyleRow 为本会话选择提示词风格，或跟随全局选择。
function OutputStyleRow(props: {
  catalog: OutputStyleCatalogSummary;
  conversationId: string | null;
  draftStyleId?: string | null;
  sessionOutputStyle?: SessionOutputStyleSummary | null;
  onChange: (styleId: string | null) => void;
}) {
  const { locale, t } = useI18n();
  const selectedId = props.conversationId
    ? props.sessionOutputStyle?.overrideStyleId ?? null
    : props.draftStyleId ?? null;
  const globalEntry = props.catalog.entries.find((entry) => entry.id === props.catalog.selectedId);
  const globalLabel = globalEntry
    ? pickBilingualText(locale, globalEntry.labelZh, globalEntry.labelEn)
    : props.catalog.selectedId;
  const selectedEntry = props.catalog.entries.find((entry) => entry.id === selectedId);
  const pending = Boolean(props.conversationId && props.sessionOutputStyle?.pendingActivation);
  return (
    <SessionSettingsGroup icon={<PenLine size={14} />} section="style" title={t("composer.outputStyle.menuTitle")}>
    <SessionSettingsRow
      description={pending
        ? t("composer.outputStyle.savedPending")
        : selectedEntry
          ? formatMessage(t("composer.outputStyle.promptCharacters"), { count: selectedEntry.promptCharCount.toLocaleString(locale) })
          : formatMessage(t("composer.outputStyle.useSystemSetting"), { label: globalLabel })}
    >
      <Select onValueChange={(value) => props.onChange(value === "inherit" ? null : value.slice("style:".length))}
        value={selectedId === null ? "inherit" : `style:${selectedId}`}>
        <SelectTrigger aria-label={t("composer.outputStyle.menuTitle")} size="sm"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="inherit">{t("composer.outputStyle.inheritGlobal")}</SelectItem>
          {props.catalog.entries.map((entry) => (
            <SelectItem key={entry.id} value={`style:${entry.id}`}>{pickBilingualText(locale, entry.labelZh, entry.labelEn)}</SelectItem>
          ))}
        </SelectContent>
      </Select>
    </SessionSettingsRow>
    </SessionSettingsGroup>
  );
}

// AnswerModeRow chooses whether answers may use native answer components.
// AnswerModeRow 选择回答是否可使用原生回答组件。
function AnswerModeRow(props: { controller: ComposerAnswerModeController }) {
  const { t } = useI18n();
  const { mode, saving, available, awaitingRequest, select } = props.controller;
  return (
    <SessionSettingsGroup icon={<Shapes size={14} />} section="answer" title={t("answerMode.label")}>
    <SessionSettingsRow
      description={awaitingRequest ? t("answerMode.pending") : t(mode === "interactive" ? "answerMode.interactiveHint" : "answerMode.standardHint")}
    >
      <SegmentedControl aria-busy={saving} aria-label={t("answerMode.label")} disabled={!available || saving}
        onValueChange={(value) => select(value as "standard" | "interactive")} value={mode ?? ""}>
        <SegmentedControlItem value="standard">{t("answerMode.standard")}</SegmentedControlItem>
        <SegmentedControlItem value="interactive">{t("answerMode.interactive")}</SegmentedControlItem>
      </SegmentedControl>
    </SessionSettingsRow>
    </SessionSettingsGroup>
  );
}

// VMM_POLICY_MODE_OPTIONS lists the session-level VMM policy modes.
// VMM_POLICY_MODE_OPTIONS 列出会话级 VMM 策略模式。
const VMM_POLICY_MODE_OPTIONS: Array<{ mode: string; labelKey: TranslationKey }> = [
  { mode: "inherit", labelKey: "composer.vmm.mode.inheritProject" },
  { mode: "enabled", labelKey: "composer.vmm.mode.enabled" },
  { mode: "disabled", labelKey: "composer.vmm.mode.disabled" },
];

// VmmPolicyRow renders one VMM policy's three-way choice.
// VmmPolicyRow 渲染单个 VMM 策略的三态选择。
function VmmPolicyRow(props: {
  // User-facing policy label and backend policy key.
  // 面向用户的策略标签与后端策略键。
  label: string;
  policy: string;
  // Currently selected mode; applies one new mode.
  // 当前选中的模式；应用新的模式。
  selectedMode: string;
  onChange: (policy: string, mode: string) => void;
}) {
  const { t } = useI18n();
  return (
    <SessionSettingsRow title={props.label}>
      <SegmentedControl aria-label={props.label} onValueChange={(mode) => props.onChange(props.policy, mode)} value={props.selectedMode}>
        {VMM_POLICY_MODE_OPTIONS.map((option) => (
          <SegmentedControlItem key={option.mode} value={option.mode}>{t(option.labelKey)}</SegmentedControlItem>
        ))}
      </SegmentedControl>
    </SessionSettingsRow>
  );
}
