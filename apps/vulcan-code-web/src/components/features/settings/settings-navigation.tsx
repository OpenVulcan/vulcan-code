import React from "react";
import {
  Archive,
  Bell,
  Cpu,
  Database,
  GitCommitHorizontal,
  Info,
  Palette,
  SwatchBook,
  ScrollText,
  ServerCog,
  ShieldCheck,
  Settings,
  SlidersHorizontal,
  Stethoscope,
} from "lucide-react";
import { useI18n } from "@/i18n";
import type { SettingsSubAreaId } from "@/types";

// SettingsHeaderSummary describes the active settings page header copy.
// SettingsHeaderSummary 描述当前设置页标题文案。
export interface SettingsHeaderSummary {
  // Header title shown on the first line.
  // 第一行展示的标题。
  title: string;
  // Parent group label used by the title-bar breadcrumb.
  // 标题栏面包屑使用的父级分组名称。
  group: string;
  // Compact description shown on the second line.
  // 第二行展示的紧凑说明。
  detail: string;
}

// settingsSubAreaProviderToolCapability maps capability pages to the unified provider tool contract.
// settingsSubAreaProviderToolCapability 将能力页面映射到统一供应商工具契约。
export function settingsSubAreaProviderToolCapability(
  area: SettingsSubAreaId,
): import("@/types").PublicAiProviderToolCapabilityKind | null {
  const mapping: Partial<
    Record<SettingsSubAreaId, import("@/types").PublicAiProviderToolCapabilityKind>
  > = {
    "ai-web-search": "web_search",
    "ai-web-extract": "web_extract",
    "ai-understand-image": "image_understand",
    "ai-understand-video": "video_understand",
    "ai-understand-audio": "audio_understand",
    "ai-image-generation": "image_generate",
    "ai-image-edit": "image_edit",
    "ai-video-generation": "video_generate",
    "ai-video-edit": "video_edit",
    "ai-video-extend": "video_extend",
    "ai-speech-synthesis": "speech_synthesize",
    "ai-speech-transcribe": "speech_transcribe",
    "ai-music-generation": "music_generate",
    "ai-music-cover": "music_cover",
  };
  return mapping[area] ?? null;
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

// SettingsSubAreaGroup describes one parent group in the settings navigation.
// SettingsSubAreaGroup 描述设置导航中的单个父级分组。
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

// normalizeSettingsSubArea maps retired page ids onto the page that now owns their content.
// normalizeSettingsSubArea 将已退役的页面标识映射到现在承载其内容的页面。
// Parameters: area is any stored or deep-linked page id; returns the id shown as current in the navigation.
// 参数：area 为任意已保存或深链接的页面标识；返回在导航中显示为当前页的标识。
export function normalizeSettingsSubArea(area: SettingsSubAreaId): SettingsSubAreaId {
  if (area === "prompt-composer") return "context-basics";
  // Providers, accounts, models and capability routing share the merged model services page.
  // 供应商、账号、模型与能力分配共用合并后的模型服务页。
  if (area === "ai-vendors" || area === "credential-management" || area === "model-list" || area === "ai-capabilities") return "model-providers";
  if (settingsSubAreaProviderToolCapability(area)) return "model-providers";
  return area;
}

// activeSettingsHeaderSummary returns the title, description and group for the current settings page.
// activeSettingsHeaderSummary 返回当前设置页的标题、说明与所属分组。
export function activeSettingsHeaderSummary(activeArea: SettingsSubAreaId, t: ReturnType<typeof useI18n>["t"]): SettingsHeaderSummary {
  const normalized = normalizeSettingsSubArea(activeArea);
  const group = settingsSubAreaGroups(t).find((candidate) => candidate.items.some((item) => item.id === normalized));
  const entry = group?.items.find((item) => item.id === normalized);
  return {
    title: entry?.label ?? t("area.settings"),
    detail: entry?.description ?? t("area.settings.description"),
    group: group?.label ?? t("area.settings"),
  };
}

// settingsSubAreas returns localized settings leaf entries.
// settingsSubAreas 返回本地化设置叶子入口。
export function settingsSubAreas(t: ReturnType<typeof useI18n>["t"]): SettingsSubAreaEntry[] {
  return settingsSubAreaGroups(t).flatMap((group) => group.items);
}

// settingsSubAreaGroups returns the two-level settings tree: four groups, at most five pages each.
// settingsSubAreaGroups 返回两级设置树：4 个分组，每组最多 5 个页面。
export function settingsSubAreaGroups(t: ReturnType<typeof useI18n>["t"]): SettingsSubAreaGroup[] {
  return [
    {
      id: "general",
      label: t("settings.group.general"),
      icon: <Settings size={16} />,
      items: [
        { id: "context-basics", label: t("settings.modelBasics"), icon: <SlidersHorizontal size={16} />, description: t("settings.modelBasics.description") },
        { id: "appearance", label: t("settings.appearance"), icon: <SwatchBook size={16} />, description: t("settings.appearance.description") },
        { id: "notifications", label: t("settings.notifications"), icon: <Bell size={16} />, description: t("settings.notifications.description") },
        { id: "git-commit", label: t("settings.gitCommit"), icon: <GitCommitHorizontal size={16} />, description: t("settings.gitCommit.description") },
        { id: "permission-judge", label: t("settings.permissionJudge"), icon: <ShieldCheck size={16} />, description: t("settings.permissionJudge.description") },
      ],
    },
    {
      id: "models",
      label: t("settings.group.models"),
      icon: <Cpu size={16} />,
      items: [
        { id: "model-providers", label: t("settings.modelProviders"), icon: <ServerCog size={16} />, description: t("settings.modelProviders.description") },
      ],
    },
    {
      id: "data",
      label: t("settings.group.data"),
      icon: <Database size={16} />,
      items: [
        { id: "archived-restore", label: t("settings.archivedRestore"), icon: <Archive size={16} />, description: t("settings.archivedRestore.description") },
      ],
    },
    {
      id: "about",
      label: t("settings.group.about"),
      icon: <Info size={16} />,
      items: [
        { id: "runtime-diagnostics", label: t("settings.runtimeDiagnostics"), icon: <Stethoscope size={16} />, description: t("settings.runtimeDiagnostics.description") },
        { id: "about-licenses", label: t("settings.aboutLicenses"), icon: <ScrollText size={16} />, description: t("settings.aboutLicenses.description") },
        { id: "visual-lab", label: t("settings.visualLab"), icon: <Palette size={16} />, description: t("settings.visualLab.description") },
      ],
    },
  ];
}
