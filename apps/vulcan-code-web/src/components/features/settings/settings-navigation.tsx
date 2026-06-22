import React from "react";
import {
  Archive,
  Bot,
  BrainCircuit,
  FileText,
  FolderTree,
  Image,
  Music,
  RefreshCw,
  RotateCw,
  ScrollText,
  Search,
  Settings,
  Video,
  Volume2,
} from "lucide-react";
import { useI18n } from "@/i18n";
import type { SettingsSubAreaId } from "@/types";

// SettingsHeaderSummary describes the active settings page header copy.
// SettingsHeaderSummary 描述当前设置页标题文案。
export interface SettingsHeaderSummary {
  // Header title shown on the first line.
  // 第一行展示的标题。
  title: string;
  // Compact description shown on the second line.
  // 第二行展示的紧凑说明。
  detail: string;
}

// SettingsSubAreaEntry describes one leaf page in the settings navigation tree.
// SettingsSubAreaEntry 描述设置导航树中的单个叶子页面。
export interface SettingsSubAreaEntry {
  // Stable settings page identifier.
  // 稳定设置页面标识。
  id: SettingsSubAreaId;
  // Localized settings page label.
  // 本地化设置页面标签。
  label: string;
  // Icon shown before the page label.
  // 显示在页面标签前的图标。
  icon: React.ReactNode;
  // Localized settings page description.
  // 本地化设置页面说明。
  description: string;
}

// SettingsSubAreaGroup describes one parent group in the settings sidebar tree.
// SettingsSubAreaGroup 描述设置侧栏树中的单个父级分组。
export interface SettingsSubAreaGroup {
  // Stable group identifier.
  // 稳定分组标识。
  id: string;
  // Localized group label.
  // 本地化分组标签。
  label: string;
  // Group icon.
  // 分组图标。
  icon: React.ReactNode;
  // Leaf settings pages in the group.
  // 分组下的叶子设置页面。
  items: SettingsSubAreaEntry[];
}

// activeSettingsHeaderSummary returns the title and description for the current settings page.
// activeSettingsHeaderSummary 返回当前设置页的标题与说明。
export function activeSettingsHeaderSummary(activeArea: SettingsSubAreaId, t: ReturnType<typeof useI18n>["t"]): SettingsHeaderSummary {
  const area = settingsSubAreas(t);
  const entry = area.find((item) => item.id === activeArea);
  return {
    title: entry?.label ?? t("area.settings"),
    detail: entry?.description ?? t("area.settings.description"),
  };
}

// settingsSubAreaCapabilityKind maps a settings page to the backend AI capability kind.
// settingsSubAreaCapabilityKind 将设置页面映射到后端 AI 能力类型。
export function settingsSubAreaCapabilityKind(area: SettingsSubAreaId): string | null {
  const mapping: Partial<Record<SettingsSubAreaId, string>> = {
    "ai-web-search": "web_search",
    "ai-understand-image": "understand_image",
    "ai-understand-video": "understand_video",
    "ai-understand-audio": "understand_audio",
    "ai-image-generation": "image_generation",
    "ai-video-generation": "video_generation",
    "ai-speech-synthesis": "speech_synthesis",
    "ai-music-generation": "music_generation",
    "ai-music-cover": "music_cover",
  };
  return mapping[area] ?? null;
}

// settingsSubAreas returns localized settings sidebar leaf entries.
// settingsSubAreas 返回本地化设置侧栏叶子入口。
export function settingsSubAreas(t: ReturnType<typeof useI18n>["t"]): SettingsSubAreaEntry[] {
  return settingsSubAreaGroups(t).flatMap((group) => group.items);
}

// settingsSubAreaGroups returns the settings tree aligned with the WebUI settings navigation.
// settingsSubAreaGroups 返回与 WebUI 设置导航对齐的设置树。
export function settingsSubAreaGroups(t: ReturnType<typeof useI18n>["t"]): SettingsSubAreaGroup[] {
  return [
    {
      id: "models",
      label: t("settings.group.models"),
      icon: <Bot size={16} />,
      items: [
        { id: "model-providers", label: t("settings.modelProviders"), icon: <FileText size={16} />, description: t("settings.modelProviders.description") },
        { id: "model-list", label: t("settings.modelList"), icon: <FileText size={16} />, description: t("settings.modelList.description") },
      ],
    },
    {
      id: "ai-capabilities",
      label: t("settings.group.aiCapabilities"),
      icon: <BrainCircuit size={16} />,
      items: [
        { id: "ai-vendors", label: t("settings.aiVendors"), icon: <FileText size={16} />, description: t("settings.aiVendors.description") },
        { id: "ai-web-search", label: t("settings.aiWebSearch"), icon: <Search size={16} />, description: t("settings.aiWebSearch.description") },
        { id: "ai-understand-image", label: t("settings.aiUnderstandImage"), icon: <Image size={16} />, description: t("settings.aiUnderstandImage.description") },
        { id: "ai-understand-video", label: t("settings.aiUnderstandVideo"), icon: <Video size={16} />, description: t("settings.aiUnderstandVideo.description") },
        { id: "ai-understand-audio", label: t("settings.aiUnderstandAudio"), icon: <Volume2 size={16} />, description: t("settings.aiUnderstandAudio.description") },
        { id: "ai-image-generation", label: t("settings.aiImageGeneration"), icon: <Image size={16} />, description: t("settings.aiImageGeneration.description") },
        { id: "ai-video-generation", label: t("settings.aiVideoGeneration"), icon: <Video size={16} />, description: t("settings.aiVideoGeneration.description") },
        { id: "ai-speech-synthesis", label: t("settings.aiSpeechSynthesis"), icon: <Volume2 size={16} />, description: t("settings.aiSpeechSynthesis.description") },
        { id: "ai-music-generation", label: t("settings.aiMusicGeneration"), icon: <Music size={16} />, description: t("settings.aiMusicGeneration.description") },
        { id: "ai-music-cover", label: t("settings.aiMusicCover"), icon: <RotateCw size={16} />, description: t("settings.aiMusicCover.description") },
      ],
    },
    {
      id: "system",
      label: t("area.settings"),
      icon: <Settings size={16} />,
      items: [
        { id: "mcp-servers", label: t("settings.mcpServers"), icon: <FolderTree size={16} />, description: t("settings.mcpServers.description") },
        { id: "runtime-diagnostics", label: t("settings.runtimeDiagnostics"), icon: <RefreshCw size={16} />, description: t("settings.runtimeDiagnostics.description") },
        { id: "prompt-composer", label: t("settings.promptComposer"), icon: <FileText size={16} />, description: t("settings.promptComposer.description") },
        { id: "about-licenses", label: t("settings.aboutLicenses"), icon: <ScrollText size={16} />, description: t("settings.aboutLicenses.description") },
        { id: "archived-restore", label: t("settings.archivedRestore"), icon: <Archive size={16} />, description: t("settings.archivedRestore.description") },
      ],
    },
  ];
}
