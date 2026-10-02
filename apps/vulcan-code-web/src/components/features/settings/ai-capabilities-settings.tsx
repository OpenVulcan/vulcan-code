import * as React from "react";
import { Settings2 } from "lucide-react";

import { ProviderToolCapabilityIcon } from "@/components/shared/provider-tool-capability-icon";
import { Button } from "@/components/ui/button";
import { InlineStreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { useI18n } from "@/i18n";
import { isModelExtensionCapability } from "@/lib/provider-tool-capabilities";
import { cn } from "@/lib/utils";
import { loadPublicAiSettings } from "@/service/settings-actions-service";
import type {
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTargetSelection,
  PublicAiSettingsSnapshot,
} from "@/types";

import { PageTabs } from "@/components/workspace/workspace-directory-layout";

import { ProviderToolSelectionSettingsArea } from "./provider-tool-selection-settings";

// AiCapabilityTabId groups the fourteen capabilities into four tabs by what the user is trying to do.
// AiCapabilityTabId 按用户意图把 14 项能力归入 4 个标签页。
type AiCapabilityTabId = "search" | "understand" | "generate" | "audio";

// AiCapabilityDefinition binds one capability to its existing localized label and description keys.
// AiCapabilityDefinition 将单项能力绑定到既有的本地化名称与说明键。
interface AiCapabilityDefinition {
  // Semantic capability configured by the row.
  // 该行配置的语义能力。
  capability: PublicAiProviderToolCapabilityKind;
  // Translation key of the capability name.
  // 能力名称的翻译键。
  labelKey: Parameters<ReturnType<typeof useI18n>["t"]>[0];
  // Translation key of the one-line capability description.
  // 能力一句话说明的翻译键。
  descriptionKey: Parameters<ReturnType<typeof useI18n>["t"]>[0];
}

// AI_CAPABILITY_TABS keeps the retired per-capability pages' labels, now ordered inside four tabs.
// AI_CAPABILITY_TABS 沿用原单项能力页的文案，现按 4 个标签页排列。
const AI_CAPABILITY_TABS: Array<{ id: AiCapabilityTabId; labelKey: AiCapabilityDefinition["labelKey"]; items: AiCapabilityDefinition[] }> = [
  {
    id: "search",
    labelKey: "settings.aiCapabilities.tab.search",
    items: [
      { capability: "web_search", labelKey: "settings.aiWebSearch", descriptionKey: "settings.aiWebSearch.description" },
      { capability: "web_extract", labelKey: "settings.aiWebExtract", descriptionKey: "settings.aiWebExtract.description" },
    ],
  },
  {
    id: "understand",
    labelKey: "settings.aiCapabilities.tab.understand",
    items: [
      { capability: "image_understand", labelKey: "settings.aiUnderstandImage", descriptionKey: "settings.aiUnderstandImage.description" },
      { capability: "video_understand", labelKey: "settings.aiUnderstandVideo", descriptionKey: "settings.aiUnderstandVideo.description" },
      { capability: "audio_understand", labelKey: "settings.aiUnderstandAudio", descriptionKey: "settings.aiUnderstandAudio.description" },
    ],
  },
  {
    id: "generate",
    labelKey: "settings.aiCapabilities.tab.generate",
    items: [
      { capability: "image_generate", labelKey: "settings.aiImageGeneration", descriptionKey: "settings.aiImageGeneration.description" },
      { capability: "image_edit", labelKey: "settings.providerTools.imageEdit", descriptionKey: "settings.aiImageEdit.description" },
      { capability: "video_generate", labelKey: "settings.aiVideoGeneration", descriptionKey: "settings.aiVideoGeneration.description" },
      { capability: "video_edit", labelKey: "settings.providerTools.videoEdit", descriptionKey: "settings.aiVideoEdit.description" },
      { capability: "video_extend", labelKey: "settings.providerTools.videoExtend", descriptionKey: "settings.aiVideoExtend.description" },
    ],
  },
  {
    id: "audio",
    labelKey: "settings.aiCapabilities.tab.audio",
    items: [
      { capability: "speech_synthesize", labelKey: "settings.aiSpeechSynthesis", descriptionKey: "settings.aiSpeechSynthesis.description" },
      { capability: "speech_transcribe", labelKey: "settings.providerTools.speechTranscribe", descriptionKey: "settings.aiSpeechTranscribe.description" },
      { capability: "music_generate", labelKey: "settings.aiMusicGeneration", descriptionKey: "settings.aiMusicGeneration.description" },
      { capability: "music_cover", labelKey: "settings.aiMusicCover", descriptionKey: "settings.aiMusicCover.description" },
    ],
  },
];

// AiCapabilitySummary is the read-only state one overview row shows.
// AiCapabilitySummary 是单个概览行展示的只读状态。
interface AiCapabilitySummary {
  // Whether the capability is effective for new sessions.
  // 该能力是否对新会话生效。
  enabled: boolean;
  // Current model/account description, or null when nothing is selected.
  // 当前模型与账号描述；未选择时为 null。
  target: string | null;
}

// AiCapabilitiesSettingsArea merges every capability page into one overview with tabs and an editing drawer.
// AiCapabilitiesSettingsArea 将所有能力页合并为一个带标签页与编辑抽屉的概览页。
// Parameters: initialCapability opens the tab that owns a retired per-capability page id.
// 参数：initialCapability 打开承载已退役单项能力页的标签页。
export function AiCapabilitiesSettingsArea(props: {
  // Capability from a retired deep link; selects its tab on first render.
  // 来自已退役深链接的能力；首次渲染时选中其所在标签页。
  initialCapability?: PublicAiProviderToolCapabilityKind | null;
}) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  // reloadVersion refreshes the overview after the drawer, which owns its own snapshot, closes.
  // reloadVersion 在拥有独立快照的抽屉关闭后刷新概览。
  const [reloadVersion, setReloadVersion] = React.useState(0);
  const [activeTab, setActiveTab] = React.useState<AiCapabilityTabId>(() => (
    AI_CAPABILITY_TABS.find((tab) => tab.items.some((item) => item.capability === props.initialCapability))?.id ?? "search"
  ));
  // editing is the capability whose full editor is open in the drawer.
  // editing 是正在抽屉中编辑的能力。
  const [editing, setEditing] = React.useState<AiCapabilityDefinition | null>(null);

  React.useEffect(() => {
    let active = true;
    setError(null);
    void loadPublicAiSettings()
      .then((next) => {
        if (active) setSnapshot(next);
      })
      .catch((reason) => {
        if (active) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      active = false;
    };
  }, [reloadVersion]);

  // currentTab is the definition behind the selected page tab; only its rows are rendered.
  // currentTab 是当前页面标签对应的定义；只渲染该标签下的能力行。
  const currentTab = AI_CAPABILITY_TABS.find((tab) => tab.id === activeTab) ?? AI_CAPABILITY_TABS[0];

  return (
    <div className="ai-capabilities-page">
      {/* The shared page tab strip keeps this page consistent with provider management and the other tabbed pages. */}
      {/* 共享页面标签条让本页与供应商管理等带标签页面保持一致。 */}
      <PageTabs
        ariaLabel={t("settings.aiCapabilities")}
        onValueChange={setActiveTab}
        options={AI_CAPABILITY_TABS.map((tab) => ({
          value: tab.id,
          label: t(tab.labelKey),
          count: `${snapshot ? tab.items.filter((item) => summarizeCapability(snapshot, item.capability, t).enabled).length : "–"}/${tab.items.length}`,
        }))}
        value={activeTab}
      />
      <div className="ai-capabilities-panel" key={currentTab.id}>
        {error && !snapshot ? (
          <div className="ai-capabilities-message" role="alert">
            <p className="settings-error-text">{error}</p>
            <Button onClick={() => setReloadVersion((value) => value + 1)} size="sm" type="button" variant="outline">
              {t("settings.modelExperience.retryLoad")}
            </Button>
          </div>
        ) : (
          <ul className="ai-capability-list">
            {currentTab.items.map((item) => {
              const summary = snapshot ? summarizeCapability(snapshot, item.capability, t) : null;
              return (
                <li className="ai-capability-row" key={item.capability}>
                  <span className="ai-capability-icon" aria-hidden="true">
                    <ProviderToolCapabilityIcon capability={item.capability} size={16} />
                  </span>
                  <span className="ai-capability-copy">
                    <strong>{t(item.labelKey)}</strong>
                    <small>{t(item.descriptionKey)}</small>
                  </span>
                  <span className={cn("ai-capability-target", !summary?.target && "empty")}>
                    {summary ? summary.target ?? t("settings.aiCapabilities.notConfigured") : <InlineStreamPlaceholder label={t("settings.providerTools.loading")} />}
                  </span>
                  <span className="ai-capability-status" data-enabled={summary?.enabled ? "true" : "false"}>
                    {summary?.enabled ? t("settings.aiCapabilities.enabled") : t("settings.aiCapabilities.disabled")}
                  </span>
                  <Button
                    aria-label={`${t("settings.aiCapabilities.configure")} · ${t(item.labelKey)}`}
                    onClick={() => setEditing(item)}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    <Settings2 className="size-3.5" />
                    {t("settings.aiCapabilities.configure")}
                  </Button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
      <Sheet
        onOpenChange={(open) => {
          if (!open) {
            setEditing(null);
            setReloadVersion((value) => value + 1);
          }
        }}
        open={editing !== null}
      >
        <SheetContent className="ai-capability-sheet" side="right" size="lg">
          {editing ? (
            <>
              <SheetHeader>
                <SheetTitle className="flex items-center gap-2">
                  <ProviderToolCapabilityIcon capability={editing.capability} size={16} />
                  {t(editing.labelKey)}
                </SheetTitle>
                <SheetDescription>{t(editing.descriptionKey)} {t("settings.aiCapabilities.drawerHint")}</SheetDescription>
              </SheetHeader>
              <div className="ai-capability-sheet-body">
                <ProviderToolSelectionSettingsArea capability={editing.capability} />
              </div>
            </>
          ) : null}
        </SheetContent>
      </Sheet>
    </div>
  );
}

// summarizeCapability projects the persisted snapshot into the overview row without inventing new rules.
// summarizeCapability 将持久化快照投影为概览行状态，不引入新的判定规则。
// Parameters: snapshot is authoritative settings, capability selects the row, t localizes counts.
// 参数：snapshot 为权威设置，capability 选择行，t 用于本地化计数文案。
function summarizeCapability(
  snapshot: PublicAiSettingsSnapshot,
  capability: PublicAiProviderToolCapabilityKind,
  t: ReturnType<typeof useI18n>["t"],
): AiCapabilitySummary {
  if (isModelExtensionCapability(capability)) {
    // Generative capabilities are effective exactly when at least one native model is selected.
    // 生成类能力在至少选中一个原生模型时生效。
    const selections = snapshot.providerExtensionTargetSets[capability] ?? [];
    return {
      enabled: selections.length > 0,
      target: selections.length === 0 ? null
        : selections.length === 1 ? describeTarget(snapshot, selections[0])
          : t("settings.aiCapabilities.modelCount").replace("{count}", String(selections.length)),
    };
  }
  const enabled = snapshot.providerToolEnabledCapabilities.includes(capability);
  const mediaSelection = snapshot.mediaUnderstandingSelections?.[capability];
  if (mediaSelection?.mode === "model_replacement") {
    const candidate = snapshot.mediaUnderstandingModelCandidates?.find((item) => (
      item.selection.provider_id === mediaSelection.selection.provider_id
      && item.selection.model_id === mediaSelection.selection.model_id
      && item.selection.connection_id === mediaSelection.selection.connection_id
    ));
    const model = candidate ? `${candidate.modelDisplayName} · ${candidate.connectionDisplayName}` : mediaSelection.selection.model_id;
    return { enabled, target: t("settings.aiCapabilities.modelReplacement").replace("{model}", model) };
  }
  const selection = mediaSelection?.mode === "provider_tool" ? mediaSelection.selection : snapshot.providerToolSelections[capability];
  return { enabled, target: selection ? describeTarget(snapshot, selection) : null };
}

// describeTarget renders one exact target selection as "model · account".
// describeTarget 将一个精确目标选择渲染为「模型 · 账号」。
function describeTarget(snapshot: PublicAiSettingsSnapshot, selection: PublicAiProviderToolTargetSelection): string {
  const target = snapshot.providerToolTargets.find((candidate) => (
    candidate.connectionId === selection.connection_id && candidate.target.target_id === selection.target_id
  ));
  return target ? `${target.target.display_name} · ${target.connectionDisplayName}` : selection.target_id;
}
