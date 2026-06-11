import React from "react";
import ReactDOM from "react-dom/client";
import ReactMarkdown from "react-markdown";
import {
  closestCenter,
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Icon as IconifyIcon, addCollection } from "@iconify/react";
import { icons as materialIconThemeIconSet } from "@iconify-json/material-icon-theme";
import { icons as skillIconSet } from "@iconify-json/skill-icons";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { Button } from "@/components/ui/button";
import { StatusIcon, StatusPill } from "@/components/shared/status";
import {
  FeatureShell as ExtractedFeatureShell,
  areaIconMap,
  primaryFeatureAreas as buildPrimaryFeatureAreas,
} from "@/components/features/feature-shell";
import {
  ModelPickerControl,
  modelPickerChoiceKey,
  modelPickerChoicesFromProviders,
  type ModelPickerChoice,
} from "@/components/composer/model-picker-control";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  Dialog,
  DialogDescription,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar";
import { Spinner } from "@/components/ui/spinner";
import { Toaster } from "@/components/ui/sonner";
import { ShinyText } from "@/components/ui/shiny-text";
import { Textarea } from "@/components/ui/textarea";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { I18nProvider, useI18n, type LocaleCode } from "@/i18n";
import type {
  AiCapabilitySummary,
  AiCapabilityTargetSummary,
  AgentSkillsCatalogSummary,
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ContextInjectionCardSummary,
  ContextInjectionSnapshot,
  ContextUsageSnapshot,
  ConversationMessage,
  ConversationSummary,
  FeatureAreaSummary,
  MainAreaId,
  PendingQuestionOptionSummary,
  PendingQuestionPromptSummary,
  PendingQuestionSnapshot,
  PendingPromptSummary,
  ProjectSummary,
  ProviderSummary,
  ReasoningSelectionSummary,
  SettingsSubAreaId,
  StartupStepSnapshot,
  StartupSnapshot,
  VmmConfigDraftSummary,
  VmmConfigSaveRequest,
  VmmMemoryActionRequest,
  VmmMemoryActionResult,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";
import { DiffReviewPanel } from "@/diff/DiffReviewPanel";
import { hasRenderableDiff, parseUnifiedDiff } from "@/diff/parseDiff";
import type { DiffPanelSnapshot, DiffReviewSelection, WorkspaceDiffSnapshotResponse } from "@/diff/types";
import {
  BROWSER_SERVICE_CONNECTION_EVENT,
  createBrowserServiceTransport,
  createServiceClient,
  isBrowserServiceRuntime,
  type BrowserServiceConnectionDetail,
} from "@/service";
import { createShellClient } from "@/shell";
import { localMediaSourceUrl } from "@/resources/localMedia";
import { toast } from "sonner";
import * as LucideIcons from "lucide-react";
import {
  Activity,
  Bot,
  Boxes,
  BrainCircuit,
  Archive,
  ArrowDown,
  ArrowUp,
  ChevronLeft,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Check,
  CheckCircle2,
  CircleAlert,
  Calendar,
  Clock3,
  Code2,
  Copy,
  CornerDownRight,
  Database,
  FileCode2,
  FileJson2,
  ExternalLink,
  FileText,
  Folder,
  FolderCog,
  FolderPlus,
  FolderOpen,
  GitBranch,
  Globe,
  GripVertical,
  Hammer,
  History,
  Image as ImageIcon,
  ImagePlus,
  Link,
  Minus,
  Plus,
  LayoutDashboard,
  MailOpen,
  Maximize2,
  Minimize2,
  MessageSquareText,
  MicVocal,
  Music2,
  Scan,
  Save,
  PanelRightClose,
  PanelRightOpen,
  PanelTopOpen,
  Palette,
  Pencil,
  Pin,
  Play,
  PlugZap,
  PinOff,
  ScrollText,
  Send,
  Settings,
  Shield,
  Video,
  Volume2,
  Square,
  SquarePen,
  SlidersHorizontal,
  Moon,
  MinusIcon,
  Sun,
  SquareIcon,
  TerminalSquare,
  Trash2,
  Wrench,
  X,
} from "lucide-react";
import "./styles.css";

// GITHUB_REPOSITORY_URL points to the public project repository.
// GITHUB_REPOSITORY_URL 指向公开项目仓库。
const GITHUB_REPOSITORY_URL = "https://github.com/OpenVulcan/vulcan-code";

// THINKING_OVERLAY_STORAGE_KEY persists the WebUI-only thinking display preference.
// THINKING_OVERLAY_STORAGE_KEY 持久化仅 WebUI 使用的思考展示偏好。
const THINKING_OVERLAY_STORAGE_KEY = "vulcan-webui-thinking-overlay-enabled";

// ThemeMode describes the two shadcn default theme modes.
// ThemeMode 描述 shadcn 默认的两种主题模式。
type ThemeMode = "light" | "dark";

// ProjectIconBadgeShape names the badge silhouette used by generated project icons.
// ProjectIconBadgeShape 命名生成式项目图标使用的徽标轮廓。
type ProjectIconBadgeShape = "circle" | "square";

// ProjectIconPack names every project icon source supported by the picker.
// ProjectIconPack 命名项目图标选择器支持的每一种图标来源。
type ProjectIconPack = "skill-icons" | "material-icon-theme" | "lucide";

// ProjectIconTab names the selectable tabs inside the project icon dialog.
// ProjectIconTab 命名项目图标弹窗中的可选择标签页。
type ProjectIconTab = ProjectIconPack | "badge";

// ProjectIconSelection describes one icon override persisted for a project.
// ProjectIconSelection 描述为项目持久化的单个图标覆盖配置。
interface ProjectIconSelection {
  // Icon pack identifier stored in the project row.
  // 存储在项目行中的图标包标识。
  pack: string;
  // Icon key or encoded badge configuration stored in the project row.
  // 存储在项目行中的图标键或编码后的徽标配置。
  key: string;
}

// ProjectIconBadgeConfig stores the generated badge icon appearance.
// ProjectIconBadgeConfig 保存生成式徽标图标的外观。
interface ProjectIconBadgeConfig {
  // Badge shape used by the generated SVG.
  // 生成 SVG 使用的徽标形状。
  shape: ProjectIconBadgeShape;
  // One Chinese character or up to two Latin letters.
  // 一个中文字符或最多两个英文字母。
  text: string;
  // Badge background color.
  // 徽标背景色。
  background: string;
  // Badge border color.
  // 徽标边框色。
  border: string;
  // Badge text color.
  // 徽标文字色。
  foreground: string;
}

// ProjectIconOption describes one selectable lucide project icon.
// ProjectIconOption 描述一个可选择的 lucide 项目图标。
interface ProjectIconOption {
  // Icon pack that owns this option.
  // 拥有该选项的图标包。
  pack: ProjectIconPack;
  // Stable icon key persisted in the project row.
  // 持久化到项目行中的稳定图标键。
  key: string;
  // User-facing icon label.
  // 面向用户显示的图标名称。
  label: string;
  // Lucide icon component rendered by outline icon options.
  // 线性图标选项渲染使用的 Lucide 图标组件。
  Icon?: React.ComponentType<{ size?: number; className?: string }>;
  // Iconify full icon name for colored icon options.
  // 彩色图标选项使用的 Iconify 完整图标名。
  iconName?: string;
}

// ComposerSubmission contains the user prompt and runtime quick-control values.
// ComposerSubmission 包含用户提示词与运行时快捷控制取值。
interface ComposerSubmission {
  // User-entered prompt text to append into the active conversation.
  // 需要追加到当前会话中的用户输入提示词。
  prompt: string;
  // Staged attachments submitted before the prompt text.
  // 在提示词文本之前提交的已暂存附件。
  attachments: ComposerAttachmentSummary[];
  // Composer execution mode selected by the user.
  // 用户选择的输入区执行模式。
  executionMode: string;
  // Reasoning effort selected by the user.
  // 用户选择的思考档位。
  reasoningEffort: string;
  // Runtime-disabled AI capability keys for the current prompt.
  // 当前提示词运行时禁用的 AI 能力键。
  disabledAiCapabilityKinds: AiCapabilityKind[];
  // Runtime-disabled AI capability route selectors for the current prompt.
  // 当前提示词运行时禁用的 AI 能力路由选择器。
  disabledAiCapabilityTargetSelectors: string[];
  // Runtime target order overrides grouped by AI capability key.
  // 按 AI 能力键分组的运行时目标顺序覆盖。
  aiCapabilityTargetOrderOverrides: Record<string, string[]>;
}

// ComposerSlashCommandOpenTarget names existing composer popups opened by slash commands.
// ComposerSlashCommandOpenTarget 命名 slash 指令可打开的现有输入区弹窗。
type ComposerSlashCommandOpenTarget = "model" | "reasoning";

// ComposerSlashCommandResult describes the routed effect of one slash command activation.
// ComposerSlashCommandResult 描述单个 slash 指令激活后的路由效果。
interface ComposerSlashCommandResult {
  // Whether the command was handled by the caller.
  // 调用方是否已处理该指令。
  handled: boolean;
  // Optional text inserted into the composer after command activation.
  // 指令激活后可选插入到输入区的文本。
  replacementText?: string;
  // Optional existing popup opened by command activation.
  // 指令激活后打开的可选现有弹窗。
  openTarget?: ComposerSlashCommandOpenTarget;
}

// SLASH_SKILL_PROMPT matches the native workbench prompt inserted by /skill.
// SLASH_SKILL_PROMPT 对齐原生工作台中 /skill 插入的提示词。
const SLASH_SKILL_PROMPT = "请根据当前任务判断是否需要调用 `skill` 工具加载已启用的提示词技能详情。";

// DEFAULT_PROJECT_BADGE_CONFIG provides the first generated badge preview.
// DEFAULT_PROJECT_BADGE_CONFIG 提供生成式徽标的初始预览配置。
const DEFAULT_PROJECT_BADGE_CONFIG: ProjectIconBadgeConfig = {
  shape: "square",
  text: "VC",
  background: "#111827",
  border: "#111827",
  foreground: "#ffffff",
};

// PROJECT_BADGE_COLORS lists quick colors for generated project badges.
// PROJECT_BADGE_COLORS 列出生成式项目徽标的快捷颜色。
const PROJECT_BADGE_COLORS = ["#111827", "#2563eb", "#0891b2", "#16a34a", "#ca8a04", "#dc2626", "#7c3aed", "#db2777"];

// PROJECT_ICON_STREAM_BATCH_SIZE controls how many icon cells are mounted per incremental pass.
// PROJECT_ICON_STREAM_BATCH_SIZE 控制每次增量挂载多少个图标单元。
const PROJECT_ICON_STREAM_BATCH_SIZE = 120;

addCollection(skillIconSet);
addCollection(materialIconThemeIconSet);

// SKILL_PROJECT_ICON_OPTIONS lists colored Skill Icons for the project picker.
// SKILL_PROJECT_ICON_OPTIONS 列出项目选择器使用的彩色 Skill Icons 图标。
const SKILL_PROJECT_ICON_OPTIONS: ProjectIconOption[] = buildIconifyProjectIconOptions("skill-icons", Object.keys(skillIconSet.icons));

// MATERIAL_PROJECT_ICON_OPTIONS lists colored Material Icon Theme icons for the project picker.
// MATERIAL_PROJECT_ICON_OPTIONS 列出项目选择器使用的彩色 Material Icon Theme 图标。
const MATERIAL_PROJECT_ICON_OPTIONS: ProjectIconOption[] = buildIconifyProjectIconOptions("material-icon-theme", Object.keys(materialIconThemeIconSet.icons));

// LUCIDE_PROJECT_ICON_OPTIONS lists every usable exported Lucide icon for the project picker.
// LUCIDE_PROJECT_ICON_OPTIONS 列出项目选择器可使用的全部 Lucide 导出图标。
const LUCIDE_PROJECT_ICON_OPTIONS: ProjectIconOption[] = buildLucideProjectIconOptions();

// PROJECT_ICON_MAP resolves persisted lucide icon keys to components.
// PROJECT_ICON_MAP 将持久化的 lucide 图标键解析为组件。
const PROJECT_ICON_MAP = new Map<string, ProjectIconOption>(
  [
    ...LUCIDE_PROJECT_ICON_OPTIONS.map((icon) => [icon.key, icon] as const),
    ["folder", { pack: "lucide", key: "Folder", label: "Folder", Icon: Folder }] as const,
    ["folder-open", { pack: "lucide", key: "FolderOpen", label: "FolderOpen", Icon: FolderOpen }] as const,
    ["folder-plus", { pack: "lucide", key: "FolderPlus", label: "FolderPlus", Icon: FolderPlus }] as const,
    ["layout-dashboard", { pack: "lucide", key: "LayoutDashboard", label: "LayoutDashboard", Icon: LayoutDashboard }] as const,
    ["scroll-text", { pack: "lucide", key: "ScrollText", label: "ScrollText", Icon: ScrollText }] as const,
    ["message-square-text", { pack: "lucide", key: "MessageSquareText", label: "MessageSquareText", Icon: MessageSquareText }] as const,
  ],
);

// buildIconifyProjectIconOptions converts one Iconify icon set into selectable colored project icons.
// buildIconifyProjectIconOptions 将一个 Iconify 图标集转换为可选择的彩色项目图标。
function buildIconifyProjectIconOptions(pack: Extract<ProjectIconPack, "skill-icons" | "material-icon-theme">, keys: string[]): ProjectIconOption[] {
  return keys
    .map((key) => ({
      pack,
      key,
      label: key,
      iconName: `${pack}:${key}`,
    }))
    .sort((left, right) => left.label.localeCompare(right.label));
}

// buildLucideProjectIconOptions extracts usable icon components from lucide-react exports.
// buildLucideProjectIconOptions 从 lucide-react 导出中提取可用图标组件。
function buildLucideProjectIconOptions(): ProjectIconOption[] {
  return Object.entries(LucideIcons)
    .filter(([name, value]) => /^[A-Z]/.test(name) && !name.endsWith("Icon") && name !== "Icon" && isLucideIconComponent(value))
    .map(([name, Icon]) => ({
      pack: "lucide" as const,
      key: name,
      label: name,
      Icon: Icon as React.ComponentType<{ size?: number; className?: string }>,
    }))
    .sort((left, right) => left.label.localeCompare(right.label));
}

// isLucideIconComponent accepts both plain components and React.forwardRef icon objects.
// isLucideIconComponent 同时接受普通组件与 React.forwardRef 图标对象。
function isLucideIconComponent(value: unknown): boolean {
  if (typeof value === "function") {
    return true;
  }
  return Boolean(value && typeof value === "object" && "render" in value && typeof (value as { render?: unknown }).render === "function");
}

// ComposerSelectOption describes one local-only composer dropdown choice.
// ComposerSelectOption 描述一个仅用于输入区本地预览的下拉选项。
interface ComposerSelectOption {
  // Stable option value used by the shadcn Select state.
  // shadcn Select 状态使用的稳定选项值。
  value: string;
  // Primary label rendered in the compact trigger.
  // 紧凑触发器中渲染的主标签。
  label: string;
  // Optional secondary line matching the iced dual-line trigger subtitle.
  // 可选副标题，对齐 iced 双行触发器的第二行。
  subtitle?: string;
}

// AiCapabilityKind mirrors provider-config AiToolKind snake_case keys supported by iced quick toggles.
// AiCapabilityKind 对齐 iced 快捷开关支持的 provider-config AiToolKind snake_case 键。
type AiCapabilityKind =
  | "web_search"
  | "understand_image"
  | "image_generation"
  | "video_generation"
  | "speech_synthesis"
  | "music_generation"
  | "music_cover";

// ComposerAiCapabilityOption describes one quick-toggle row in the composer capability menu.
// ComposerAiCapabilityOption 描述输入区能力菜单中的单个快捷开关行。
interface ComposerAiCapabilityOption {
  // Stable provider-config capability key sent to the desktop service when disabled.
  // 禁用时发送给桌面服务的稳定 provider-config 能力键。
  value: AiCapabilityKind;
  // Icon shown in the compact menu row.
  // 显示在紧凑菜单行中的图标。
  Icon: React.ComponentType<{ size?: number; className?: string }>;
  // Chinese user-facing label.
  // 中文用户可见标签。
  labelZh: string;
  // English user-facing label.
  // 英文用户可见标签。
  labelEn: string;
  // Chinese short route hint.
  // 中文短路由提示。
  subtitleZh: string;
  // English short route hint.
  // 英文短路由提示。
  subtitleEn: string;
  // Accent color used by the compact trigger and dialog icons.
  // 紧凑触发器与弹窗图标使用的强调色。
  color: string;
}

// COMPOSER_AI_CAPABILITY_OPTIONS keeps the WebUI quick menu in the same semantic order as the iced workbench.
// COMPOSER_AI_CAPABILITY_OPTIONS 保持 WebUI 快捷菜单与 iced 工作台相同的语义顺序。
const COMPOSER_AI_CAPABILITY_OPTIONS: ComposerAiCapabilityOption[] = [
  {
    value: "web_search",
    Icon: Globe,
    labelZh: "联网搜索",
    labelEn: "Web search",
    subtitleZh: "网页搜索与检索增强",
    subtitleEn: "Browsing-grounded retrieval",
    color: "#0ea5e9",
  },
  {
    value: "understand_image",
    Icon: ImageIcon,
    labelZh: "图像理解",
    labelEn: "Image understanding",
    subtitleZh: "外部视觉能力路由",
    subtitleEn: "Vision capability routing",
    color: "#6366f1",
  },
  {
    value: "image_generation",
    Icon: ImagePlus,
    labelZh: "图像生成",
    labelEn: "Image generation",
    subtitleZh: "图片生成或编辑",
    subtitleEn: "Image generation or editing",
    color: "#ec4899",
  },
  {
    value: "video_generation",
    Icon: Video,
    labelZh: "视频生成",
    labelEn: "Video generation",
    subtitleZh: "视频生成能力",
    subtitleEn: "Video generation capability",
    color: "#f97316",
  },
  {
    value: "speech_synthesis",
    Icon: Volume2,
    labelZh: "语音合成",
    labelEn: "Speech synthesis",
    subtitleZh: "文本转语音生成",
    subtitleEn: "Text-to-speech generation",
    color: "#14b8a6",
  },
  {
    value: "music_generation",
    Icon: Music2,
    labelZh: "音乐生成",
    labelEn: "Music generation",
    subtitleZh: "音乐生成能力",
    subtitleEn: "Music generation capability",
    color: "#a855f7",
  },
  {
    value: "music_cover",
    Icon: MicVocal,
    labelZh: "音乐翻唱",
    labelEn: "Music cover",
    subtitleZh: "基于参考音频翻唱",
    subtitleEn: "Reference-audio cover generation",
    color: "#84cc16",
  },
];

// defaultDisabledAiCapabilityKinds derives prompt-level capability disables from the service snapshot.
// defaultDisabledAiCapabilityKinds 从服务快照推导提示词级能力禁用集合。
function defaultDisabledAiCapabilityKinds(capabilities: AiCapabilitySummary[]): Set<AiCapabilityKind> {
  const disabled = new Set<AiCapabilityKind>();
  for (const capability of capabilities) {
    if (isComposerAiCapabilityKind(capability.kind) && (!capability.available || !capability.enabled)) {
      disabled.add(capability.kind);
    }
  }
  return disabled;
}

// defaultDisabledAiCapabilityTargetSelectors derives prompt-level target disables from the service snapshot.
// defaultDisabledAiCapabilityTargetSelectors 从服务快照推导提示词级目标禁用集合。
function defaultDisabledAiCapabilityTargetSelectors(capabilities: AiCapabilitySummary[]): Set<string> {
  const disabled = new Set<string>();
  for (const capability of capabilities) {
    for (const target of capability.targets) {
      if (!target.enabled) {
        disabled.add(target.targetSelector);
      }
    }
  }
  return disabled;
}

// defaultAiCapabilityTargetOrder derives prompt-level target order from the service snapshot.
// defaultAiCapabilityTargetOrder 从服务快照推导提示词级目标顺序。
function defaultAiCapabilityTargetOrder(capabilities: AiCapabilitySummary[]): Map<AiCapabilityKind, string[]> {
  const order = new Map<AiCapabilityKind, string[]>();
  for (const capability of capabilities) {
    if (isComposerAiCapabilityKind(capability.kind)) {
      order.set(capability.kind, capability.targets.map((target) => target.targetSelector));
    }
  }
  return order;
}

// serializeAiCapabilityTargetOrder converts local target order into the service payload shape.
// serializeAiCapabilityTargetOrder 将本地目标顺序转换为服务载荷形态。
function serializeAiCapabilityTargetOrder(order: Map<AiCapabilityKind, string[]>): Record<string, string[]> {
  return Object.fromEntries(Array.from(order.entries()).map(([kind, selectors]) => [kind, selectors]));
}

// orderedAiCapabilityTargets returns targets in the current prompt-level display order.
// orderedAiCapabilityTargets 按当前提示词级显示顺序返回目标列表。
function orderedAiCapabilityTargets(
  capability: AiCapabilitySummary | undefined,
  order: Map<AiCapabilityKind, string[]>,
): AiCapabilityTargetSummary[] {
  if (!capability || !isComposerAiCapabilityKind(capability.kind)) {
    return [];
  }
  const currentOrder = order.get(capability.kind) ?? [];
  const bySelector = new Map(capability.targets.map((target) => [target.targetSelector, target]));
  const ordered = currentOrder
    .map((selector) => bySelector.get(selector))
    .filter((target): target is AiCapabilityTargetSummary => Boolean(target));
  for (const target of capability.targets) {
    if (!ordered.some((existing) => existing.targetSelector === target.targetSelector)) {
      ordered.push(target);
    }
  }
  return ordered;
}

// aiCapabilitySignature returns a stable signature for composer-relevant capability route state.
// aiCapabilitySignature 返回输入区相关能力路由状态的稳定签名。
function aiCapabilitySignature(capabilities: AiCapabilitySummary[]): string {
  return capabilities
    .map((capability) => [
      capability.kind,
      capability.available ? "1" : "0",
      capability.enabled ? "1" : "0",
      ...capability.targets.map((target) => [
        target.targetSelector,
        target.enabled ? "1" : "0",
        target.modelId,
      ].join(":")),
    ].join("|"))
    .join("||");
}

// isComposerAiCapabilityKind narrows arbitrary service keys to composer-supported quick-toggle keys.
// isComposerAiCapabilityKind 将任意服务键收窄为输入区支持的快捷能力键。
function isComposerAiCapabilityKind(value: string): value is AiCapabilityKind {
  return COMPOSER_AI_CAPABILITY_OPTIONS.some((option) => option.value === value);
}

// DraftConversationSelection describes a not-yet-persisted conversation opened from a project.
// DraftConversationSelection 描述从项目打开但尚未持久化的草稿会话。
interface DraftConversationSelection {
  // Stable local-only draft identifier.
  // 稳定的本地草稿标识。
  id: string;
  // Owning project identifier.
  // 所属项目标识。
  projectId: string;
  // Owning project display name.
  // 所属项目展示名称。
  projectName: string;
}

// ConversationBlockKind describes one WebUI-renderable conversation block family.
// ConversationBlockKind 描述 WebUI 可渲染的单个会话块族。
type ConversationBlockKind = "markdown" | "tool-call" | "tool-result" | "command" | "diff" | "thinking" | "notice" | "turn-stats";

// GeneratedMediaKind describes host-generated media families that can be rendered inline.
// GeneratedMediaKind 描述可内联渲染的宿主生成媒体族。
type GeneratedMediaKind = "image" | "audio" | "video" | "document";

// GeneratedMediaAttachment describes one generated media output carried by tool metadata.
// GeneratedMediaAttachment 描述工具元数据携带的一条生成媒体输出。
interface GeneratedMediaAttachment {
  // Stable media family used to select the preview control.
  // 用于选择预览控件的稳定媒体族。
  kind: GeneratedMediaKind;
  // Human-readable file name displayed in the media card.
  // 媒体卡片中显示的人类可读文件名。
  fileName: string;
  // Preferred local path for preview and opening.
  // 用于预览和打开的首选本地路径。
  localPath: string;
  // Optional preview path, usually a generated video thumbnail.
  // 可选预览路径，通常是生成视频缩略图。
  previewLocalPath?: string;
  // Optional MIME type persisted by the host.
  // 宿主持久化的可选 MIME 类型。
  mimeType?: string;
}

// GENERATED_MEDIA_MIN_ZOOM is the smallest image zoom ratio allowed in media viewers.
// GENERATED_MEDIA_MIN_ZOOM 是媒体查看器允许的最小图片缩放比例。
const GENERATED_MEDIA_MIN_ZOOM = 0.1;

// GENERATED_MEDIA_MAX_ZOOM is the largest image zoom ratio allowed in media viewers.
// GENERATED_MEDIA_MAX_ZOOM 是媒体查看器允许的最大图片缩放比例。
const GENERATED_MEDIA_MAX_ZOOM = 10;

// GENERATED_MEDIA_ZOOM_STEP is the fixed zoom delta used by media viewer controls.
// GENERATED_MEDIA_ZOOM_STEP 是媒体查看器控制按钮使用的固定缩放步长。
const GENERATED_MEDIA_ZOOM_STEP = 0.1;

// GeneratedMediaImageSize stores measured image or viewport dimensions used for true-size zoom math.
// GeneratedMediaImageSize 保存用于真实尺寸缩放计算的图片或视口尺寸。
interface GeneratedMediaImageSize {
  // Width in CSS pixels or natural image pixels depending on the measurement source.
  // 宽度，单位根据测量来源为 CSS 像素或图片自然像素。
  width: number;
  // Height in CSS pixels or natural image pixels depending on the measurement source.
  // 高度，单位根据测量来源为 CSS 像素或图片自然像素。
  height: number;
}

// GeneratedMediaImageOffset stores the pan offset from the image viewport center.
// GeneratedMediaImageOffset 保存相对图片视口中心的拖拽偏移。
interface GeneratedMediaImageOffset {
  // Horizontal offset in CSS pixels.
  // 水平方向 CSS 像素偏移。
  x: number;
  // Vertical offset in CSS pixels.
  // 垂直方向 CSS 像素偏移。
  y: number;
}

// ConversationRenderItemKind describes the projected transcript item family.
// ConversationRenderItemKind 描述投影后的转录展示项族。
type ConversationRenderItemKind = "user" | "assistant" | "system";

// ConversationBlock describes one parsed message block shown inside the transcript.
// ConversationBlock 描述转录区中展示的单个解析后消息块。
interface ConversationBlock {
  // Stable block identifier within the owning message.
  // 所属消息内的稳定块标识。
  id: string;
  // Semantic block kind used for component selection.
  // 用于选择组件的语义块类型。
  kind: ConversationBlockKind;
  // Short block title shown in the compact row or modal.
  // 展示在紧凑行或弹窗中的简短块标题。
  title: string;
  // Full block body used by markdown renderers and detail views.
  // 供 Markdown 渲染器与详情视图使用的完整块正文。
  body: string;
  // Compact summary shown before opening the detail view.
  // 打开详情视图前展示的紧凑摘要。
  summary: string;
  // Optional status text detected from legacy transcript formats.
  // 从旧转录格式中识别出的可选状态文本。
  status?: string;
  // Optional tool name detected from tool-call or tool-result formats.
  // 从工具调用或工具返回格式中识别出的可选工具名称。
  toolName?: string;
  // Whether this block carries explicit tool execution metadata or a legacy tool marker.
  // 该块是否携带明确工具执行元数据或旧版工具标记。
  toolEvidence?: boolean;
  // Optional provider or host-visible tool call identifier.
  // 可选 Provider 或宿主可见工具调用标识。
  toolCallId?: string;
  // Optional host-visible tool result identifier.
  // 可选宿主可见工具返回标识。
  toolResultId?: string;
  // Optional command-session identifier for command-like tool rows.
  // 命令类工具行使用的可选命令会话标识。
  commandSessionId?: string;
  // Optional child task-session identifier for routed subagent review.
  // 用于路由子代理复看的可选子任务会话标识。
  taskSessionId?: string;
  // Optional raw metadata used for generated media and future plugin-specific renderers.
  // 可选原始元数据，用于生成媒体与未来插件专属渲染器。
  rawMetadata?: unknown;
  // Generated media outputs resolved from host tool metadata.
  // 从宿主工具元数据解析出的生成媒体输出。
  mediaAttachments?: GeneratedMediaAttachment[];
  // Optional context-compaction section id used to merge status rows.
  // 用于合并状态行的可选上下文压缩分段标识。
  compactionSectionId?: string;
  // Optional context-compaction role inside a merged section.
  // 合并分段内的可选上下文压缩角色。
  compactionRole?: string;
  // Optional context-compaction state label.
  // 可选上下文压缩状态标签。
  compactionState?: string;
  // Optional context-compaction trigger label.
  // 可选上下文压缩触发来源标签。
  compactionTrigger?: string;
  // Optional context-compaction budget note.
  // 可选上下文压缩预算摘要。
  compactionBudgetNote?: string;
}

// RuntimeCue describes the compact running-state badge shown above the composer.
// RuntimeCue 描述输入区上方展示的紧凑运行状态徽标。
interface RuntimeCue {
  // Stable cue family used to pick the icon and visual treatment.
  // 用于选择图标与视觉表现的稳定提示类型。
  kind: "default";
  // Localized text shown in the badge.
  // 徽标中展示的本地化文本。
  text: string;
}

// StatusbarRuntimeActivity describes one centered footer runtime activity badge.
// StatusbarRuntimeActivity 描述底部栏居中的单个运行活动徽标。
interface StatusbarRuntimeActivity {
  // Stable activity family used to pick the footer icon.
  // 用于选择底部栏图标的稳定活动类型。
  kind: "tool" | "subagent";
  // Localized status text.
  // 本地化状态文本。
  text: string;
}

// ConversationRenderItem describes one row rendered after transcript projection.
// ConversationRenderItem 描述转录投影后的单条渲染行。
interface ConversationRenderItem {
  // Stable projected row identifier.
  // 稳定投影行标识。
  id: string;
  // Projected row role.
  // 投影行角色。
  role: ConversationRenderItemKind;
  // Blocks rendered inside this projected row.
  // 在该投影行中渲染的块集合。
  blocks: ConversationBlock[];
  // Original transcript message used for timing and fallback projection.
  // 用于耗时与兜底投影的原始转录消息。
  sourceMessage?: ConversationMessage;
  // Original user message when this row is a user bubble.
  // 当该行为用户气泡时对应的原始用户消息。
  userMessage?: ConversationMessage;
}

// ConversationProcessRenderItem describes one collapsible per-turn processing group.
// ConversationProcessRenderItem 描述单个可折叠的轮次处理过程分组。
interface ConversationProcessRenderItem {
  // Stable process row identifier.
  // 稳定过程行标识。
  id: string;
  // Process rows that belong to the current turn before final assistant output.
  // 属于当前轮次最终助手输出前的过程行。
  items: ConversationRenderItem[];
  // User guidance rows shown under the process header but outside the folded process body.
  // 显示在过程标题下且不进入折叠正文的用户引导行。
  guidanceItems: ConversationRenderItem[];
  // Whether the process body may be manually collapsed.
  // 过程正文是否允许手动折叠。
  collapsible: boolean;
  // Whether the process should start collapsed.
  // 过程是否应默认折叠。
  defaultCollapsed: boolean;
  // Whether the turn is still running.
  // 当前轮次是否仍在运行。
  running: boolean;
  // Whether the user stopped this turn.
  // 当前轮次是否由用户停止。
  stopped: boolean;
  // Turn start timestamp in Unix milliseconds.
  // 轮次开始的 Unix 毫秒时间戳。
  startedAtMs: number | null;
  // Final fixed duration in milliseconds when available.
  // 可用时的固定最终耗时毫秒数。
  durationMs: number | null;
}

// ConversationTimelineVirtualWindow describes the currently mounted timeline slice.
// ConversationTimelineVirtualWindow 描述当前挂载的时间线切片。
interface ConversationTimelineVirtualWindow {
  // Items that should be mounted for the current viewport.
  // 当前视口需要挂载的条目。
  items: Array<{ index: number; item: ConversationTimelineItem }>;
  // Spacer height before the mounted slice.
  // 挂载切片前方的占位高度。
  paddingTop: number;
  // Spacer height after the mounted slice.
  // 挂载切片后方的占位高度。
  paddingBottom: number;
  // Whether virtualization is active for this timeline.
  // 当前时间线是否启用虚拟化。
  virtualized: boolean;
  // Whether mounted rows should be measured even before virtualization starts.
  // 挂载行是否需要在虚拟化开始前先测量。
  measuring: boolean;
}

// ConversationTimelineVirtualRestoreState stores reusable layout measurements for one transcript surface.
// ConversationTimelineVirtualRestoreState 保存单个转录界面可复用的布局测量信息。
interface ConversationTimelineVirtualRestoreState {
  // Measured row heights keyed by stable timeline item id.
  // 按稳定时间线条目标识记录的已测量行高。
  rowHeightsByKey: Record<string, number>;
}

// ConversationTimelineItem describes either one normal row or one grouped process row.
// ConversationTimelineItem 描述单条普通行或单条分组过程行。
type ConversationTimelineItem =
  | { kind: "item"; item: ConversationRenderItem }
  | { kind: "process"; process: ConversationProcessRenderItem };

// ConversationDetail describes the modal payload opened from one special block.
// ConversationDetail 描述从特殊块打开的弹窗载荷。
interface ConversationDetail {
  // Detail modal title.
  // 详情弹窗标题。
  title: string;
  // Detail modal kind used for visual treatment.
  // 用于视觉处理的详情弹窗类型。
  kind: ConversationBlockKind;
  // Detail body source.
  // 详情正文源码。
  body: string;
  // Optional detail metadata rows.
  // 可选详情元数据行。
  metadata?: Array<[string, string]>;
  // Optional generated media outputs rendered by the detail modal.
  // 可选生成媒体输出，由详情弹窗渲染。
  mediaAttachments?: GeneratedMediaAttachment[];
  // Optional hidden detail record reference loaded on demand.
  // 可选按需加载的隐藏详情记录引用。
  detailRef?: ConversationDetailRef;
}

// ConversationDetailRef identifies one hidden detail record stored outside the visible timeline.
// ConversationDetailRef 标识一条存放在可见时间线之外的隐藏详情记录。
interface ConversationDetailRef {
  // Session id that owns the hidden detail record.
  // 拥有隐藏详情记录的会话标识。
  sessionId: string;
  // Stable hidden detail record id.
  // 稳定隐藏详情记录标识。
  recordId?: string;
  // Tool-call id used when the visible call row has not received a detail record id.
  // 当可见调用行尚未收到详情记录标识时使用的工具调用标识。
  toolCallId?: string;
  // Optional JSONL event sequence for direct detail lookup.
  // 用于直接定位详情的可选 JSONL 事件序号。
  seq?: number;
  // Optional conversation message index used when seq is unavailable.
  // seq 不可用时使用的可选会话消息索引。
  messageIndex?: number;
}

// ToolDetailPayload carries one hidden structured tool-detail record.
// ToolDetailPayload 承载单条隐藏结构化工具详情记录。
interface ToolDetailPayload {
  // Model-facing assembled tool result body.
  // 面向模型的组合工具返回正文。
  body: string;
  // Structured metadata used by display renderers.
  // 展示渲染器使用的结构化元数据。
  metadata?: unknown;
}

// conversationToolDetailPayloadCache keeps hidden tool details stable across transcript remounts.
// conversationToolDetailPayloadCache 在转录重新挂载之间稳定保存隐藏工具详情。
const conversationToolDetailPayloadCache = new Map<string, ToolDetailPayload | null>();

// conversationToolDetailPromiseCache deduplicates in-flight hidden tool detail requests.
// conversationToolDetailPromiseCache 对进行中的隐藏工具详情请求去重。
const conversationToolDetailPromiseCache = new Map<string, Promise<ToolDetailPayload | null>>();

// conversationToolDetailMediaCache keeps generated media cards stable across transcript remounts.
// conversationToolDetailMediaCache 在转录重新挂载之间稳定保存生成媒体卡片。
const conversationToolDetailMediaCache = new Map<string, GeneratedMediaAttachment[]>();

// serviceClient is the single business service facade used by the WebUI.
// serviceClient 是 WebUI 使用的唯一业务服务门面。
const serviceClient = createServiceClient(
  createBrowserServiceTransport(),
);

// BROWSER_SERVICE_CONNECTION_TOAST_ID keeps reconnect notifications collapsed into one toast.
// BROWSER_SERVICE_CONNECTION_TOAST_ID 将重连通知折叠到同一个 toast。
const BROWSER_SERVICE_CONNECTION_TOAST_ID = "browser-service-connection";

// shellClient is the single client-host facade used by the WebUI.
// shellClient 是 WebUI 使用的唯一客户端宿主门面。
const shellClient = createShellClient();

// hasServiceRuntime returns whether the page can use a backend service transport.
// hasServiceRuntime 返回页面是否可以使用后端服务传输。
function hasServiceRuntime(): boolean {
  return isBrowserServiceRuntime();
}

// serviceBootstrapErrorIsRetryable returns whether the initial service load should be retried.
// serviceBootstrapErrorIsRetryable 判断初始服务加载失败是否应该自动重试。
function serviceBootstrapErrorIsRetryable(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("Browser service is unavailable")
    || message.includes("Failed to fetch")
    || message.includes("NetworkError")
    || message.includes("Load failed");
}

// WorkbenchDeltaSignal wraps the newest partial update with a React-visible sequence.
// WorkbenchDeltaSignal 使用 React 可见序号包装最新局部更新。
interface WorkbenchDeltaSignal {
  // Monotonic frontend sequence that makes repeated delta objects observable.
  // 前端单调序号，用于让重复增量对象也能被观察到。
  seq: number;
  // Backend partial workbench update envelopes batched for one render frame.
  // 单个渲染帧内批处理的后端局部工作台更新信封。
  envelopes: WorkbenchDeltaEnvelope[];
}

// BrowserFallbackStartup is used when the WebUI runs outside Tauri during frontend development.
// BrowserFallbackStartup 用于 WebUI 在 Tauri 外部进行前端开发时展示。
const browserFallbackStartup: StartupSnapshot = {
  ready: true,
  phase: "browser-preview",
  steps: [
    { id: "service-process", label: "Core Service", status: "warning", message: "浏览器预览模式：未连接本地 Core Service" },
    { id: "luaskills", label: "LuaSkills", status: "pending", message: "等待桌面壳提供真实状态" },
    { id: "vldb", label: "VLDB", status: "pending", message: "等待桌面壳提供真实状态" },
    { id: "mcp", label: "MCP Runtime", status: "pending", message: "等待桌面壳提供真实状态" },
    { id: "workspace", label: "Workspace", status: "ready", message: "前端布局预览已就绪" },
  ],
  recentLogs: ["[webui] browser preview mode"],
};

// browserFallbackSlashCommands mirrors the service-projected iced command catalog for preview mode.
// browserFallbackSlashCommands 在预览模式中镜像 service 投影出的 iced 指令目录。
const browserFallbackSlashCommands: ComposerSlashCommandSummary[] = [
  {
    id: "model",
    command: "/model",
    descriptionZh: "选择当前模型",
    descriptionEn: "Select the current model",
  },
  {
    id: "compact",
    command: "/compact",
    descriptionZh: "压缩当前会话上下文",
    descriptionEn: "Compact the current conversation context",
  },
  {
    id: "reasoning",
    command: "/reasoning",
    descriptionZh: "选择当前思考级别",
    descriptionEn: "Select the current reasoning level",
  },
  {
    id: "skill",
    command: "/skill",
    descriptionZh: "插入提示词技能加载指令",
    descriptionEn: "Insert the prompt-skill loading instruction",
  },
  {
    id: "skills",
    command: "/skills",
    descriptionZh: "打开提示词技能面板",
    descriptionEn: "Open the prompt skills panel",
  },
];

// BrowserFallbackWorkbench is used when the WebUI runs outside Tauri during frontend development.
// BrowserFallbackWorkbench 用于 WebUI 在 Tauri 外部进行前端开发时展示。
const browserFallbackWorkbench: WorkbenchSnapshot = {
  appVersion: "0.1.0",
  modules: [
    { id: "core", label: "Core Service", status: "warning", message: "浏览器预览模式" },
    { id: "luaskills", label: "LuaSkills", status: "pending", message: "等待真实状态" },
    { id: "vldb", label: "VLDB", status: "pending", message: "等待真实状态" },
    { id: "mcp", label: "MCP Runtime", status: "pending", message: "等待真实状态" },
    { id: "workspace", label: "Workspace", status: "ready", message: "前端布局预览" },
  ],
  projects: [
    { id: "preview", name: "vulcan-code", path: "D:/openvulcan/vulcan-code", pinned: true },
    { id: "calc", name: "遗弃之地鬼仆计算器", path: "D:/openvulcan/abandoned-calculator", pinned: true },
    { id: "test-project", name: "测试项目", path: "D:/openvulcan/test-project", pinned: false },
    { id: "luaskills-packages", name: "luaskills-packages", path: "D:/openvulcan/luaskills-packages", pinned: false },
    { id: "vulcan-workmem", name: "vulcan-workmem", path: "D:/openvulcan/vulcan-workmem", pinned: false },
  ],
  expandedProjectIds: ["preview", "calc", "test-project", "luaskills-packages", "vulcan-workmem"],
  conversations: [
    { id: "preview", title: "Tauri WebUI 迁移预览", status: "Completed", lastUpdatedLabel: "刚刚", projectId: "preview" },
    { id: "preview-intro", title: "介绍你自己", status: "Completed", lastUpdatedLabel: "刚刚", projectId: "preview" },
    { id: "calc-curl", title: "排查 lcurl.safe 模块缺失", status: "Completed", lastUpdatedLabel: "3 天", projectId: "calc" },
    { id: "calc-iconv", title: "检查是否缺少 iconv", status: "Completed", lastUpdatedLabel: "1 周", projectId: "calc" },
    { id: "test-tools", title: "你有哪些工具", status: "Completed", lastUpdatedLabel: "刚刚", projectId: "test-project", unread: true },
    { id: "test-playwright", title: "添加 Playwright 浏览器支持", status: "Running", lastUpdatedLabel: "1 分", projectId: "test-project" },
    { id: "luaskills-rust", title: "分析 Rust 插件机制", status: "Completed", lastUpdatedLabel: "2 天", projectId: "luaskills-packages" },
    { id: "workmem-plan", title: "VMM 置顶项目对接方案", status: "Completed", lastUpdatedLabel: "5 小时", projectId: "vulcan-workmem" },
  ],
  selectedConversationId: "preview",
  messages: [
    { id: "preview-user", conversationId: "preview", role: "user", body: "新 WebUI 需要完整承接当前界面显示。" },
    {
      id: "preview-assistant",
      conversationId: "preview",
      role: "assistant",
      body: "当前页面展示工作台、设置、插件、VMM、Provider、工具和诊断区域的第一版布局。\n\n:::tool-call tool_name=service_startup_snapshot tool_call_id=preview-service-startup status=preview\n## Tool Call\n\n`service_startup_snapshot`\n\n## Arguments\n\n```json\n{}\n```\n:::\n\n```diff\ndiff --git a/apps/vulcan-code-web/src/main.tsx b/apps/vulcan-code-web/src/main.tsx\n+ 接入 Markdown 会话渲染\n+ 特殊工具块支持详情窗口\n```\n\n后续真实会话会把工具调用、命令输出和 diff 分块展示。",
    },
    {
      id: "preview-turn-stats",
      conversationId: "preview",
      role: "assistant",
      title: "Turn Stats · completed",
      recordKind: "turn-stats",
      body: "状态：已完成\nProvider 调用：2\n工具调用：3\n输入 Tokens：45310\n输出 Tokens：1943\n思考 Tokens：1182\n总 Tokens：47253",
      metadata: {
        turn_stats: {
          status: "completed",
          provider_call_count: 2,
          tool_call_count: 3,
          input_tokens: 45310,
          output_tokens: 1943,
          reasoning_tokens: 1182,
          total_tokens: 47253,
          cached_input_tokens: 34944,
          duration_ms: 51275,
          output_tokens_per_second: 37.894,
        },
      },
    },
    {
      id: "preview-tool-result",
      conversationId: "preview",
      role: "assistant",
      title: "工具返回 · service_startup_snapshot",
      recordKind: "tool-result-output",
      body: "已运行 1 条命令\n\nCore Service 当前处于 browser-preview 模式。",
      metadata: {
        tool_call_id: "preview-service-startup",
        tool_name: "service_startup_snapshot",
        tool_result_id: "tool-result-preview-service-startup",
      },
    },
  ],
  toolCalls: [{ id: "preview-tool", name: "service_startup_snapshot", status: "preview", detail: "浏览器模式使用 fallback 数据。" }],
  featureAreas: [
    { id: "chat", label: "工作台", description: "项目会话、消息流与模型输入区", status: "可用" },
    { id: "skills", label: "技能", description: "LuaSkills、工具能力与技能包", status: "预览" },
    { id: "automation", label: "自动", description: "自动化任务、工作流与后台执行", status: "规划中" },
    { id: "plugins", label: "插件", description: "插件中心与 WebUI 控制面板", status: "预览" },
    { id: "memory", label: "记忆", description: "VMM 用户、项目与会话记忆", status: "预览" },
    { id: "settings", label: "设置", description: "项目、模型、工具、诊断与系统设置", status: "预览" },
  ],
  composerSlashCommands: browserFallbackSlashCommands,
  pendingPrompts: [],
  pendingQuestion: null,
  plugins: [
    { id: "vulcan-codekit", name: "Vulcan CodeKit", kind: "LuaSkill", status: "预览", capability: "代码搜索、AST 定位、结构化 patch" },
    { id: "vulcan-curl", name: "Vulcan Curl", kind: "LuaSkill", status: "预览", capability: "HTTP GET/POST 与受控请求" },
    { id: "vulcan-testkit", name: "Vulcan TestKit", kind: "LuaSkill", status: "预览", capability: "构建、测试、日志压缩与诊断" },
    { id: "ai-short-drama", name: "AI 短剧工作流", kind: "Workflow WebUI", status: "规划中", capability: "脚本、素材、播放器与工作流控制面板" },
  ],
  luaSkills: {
    runtimeRoot: "D:/openvulcan/vulcan-code/output/lua_runtime",
    systemLuaLib: "D:/openvulcan/vulcan-code/output/lua_runtime/system_lua_lib",
    entryCount: 3,
    notice: "浏览器预览模式展示 Lua Skills 样例目录。",
    skills: [
      {
        skillId: "vulcan-codekit",
        displayName: "Vulcan CodeKit",
        version: "0.1.0",
        rootName: "ROOT",
        skillDir: "D:/openvulcan/vulcan-code/output/lua_runtime/skills/vulcan-codekit",
        enabled: true,
        managed: true,
        sourceType: "github",
        sourceLocator: "LuaSkills/vulcan-codekit",
        entryCount: 2,
        systemOwned: true,
        tools: [
          { canonicalName: "vulcan_codekit_rg", localName: "rg", description: "搜索代码并返回结构化上下文。", registrationState: "explicit", registrationLabel: "显式启用", registrationHelp: "工具会作为独立函数 Schema 暴露给模型。" },
          { canonicalName: "vulcan_codekit_patch", localName: "patch", description: "按结构选择器替换函数。", registrationState: "implicit", registrationLabel: "隐式启用", registrationHelp: "工具保留宿主调用能力，但不暴露独立 Schema。" },
        ],
      },
    ],
  },
  agentSkills: {
    globalEnabled: true,
    systemRoot: "C:/Users/20000/.vulcan/skills/.system",
    userRoot: "C:/Users/20000/.vulcan/skills",
    notice: "浏览器预览模式展示 Agent Skills 样例目录。",
    errors: [],
    skills: [
      {
        name: "security-review",
        displayName: "Security Review",
        description: "按安全最佳实践检查 JavaScript、Python 和 Go 代码。",
        scope: "系统",
        enabled: true,
        skillFile: "C:/Users/20000/.vulcan/skills/.system/security-review/SKILL.md",
        baseDir: "C:/Users/20000/.vulcan/skills/.system/security-review",
        iconPath: "C:/Users/20000/.vulcan/skills/.system/skill-creator/assets/skill-creator-small.svg",
        body: "Use this skill when the user explicitly requests a security review.",
      },
    ],
  },
  settings: [
    { id: "runtime", label: "运行时", description: "Core Service、LuaSkills、VLDB、MCP", items: ["启动模式", "运行时目录", "MCP 工具策略"] },
    { id: "providers", label: "模型与 Provider", description: "模型供应商、默认模型、工具能力与推理配置", items: ["默认 Provider", "默认模型", "工具调用", "推理强度"] },
    { id: "plugins", label: "插件", description: "LuaSkills、Rust lib 插件、WebUI 控制面板", items: ["插件目录", "权限声明", "WebUI 面板"] },
    { id: "remote", label: "远程访问", description: "未来浏览器、手机、远程客户端访问 Core Service 的设置", items: ["监听地址", "访问令牌", "远程开关", "审计日志"] },
  ],
  providers: [
    {
      id: "deepseek",
      name: "DeepSeek",
      status: "预览",
      enabledModels: "2 个已启用模型",
      active: true,
      models: [
        { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro", active: true },
        { id: "deepseek-v4", name: "DeepSeek V4" },
      ],
    },
    {
      id: "alibaba-cn",
      name: "Alibaba CN",
      status: "预览",
      enabledModels: "1 个已启用模型",
      models: [{ id: "qwen3-coder", name: "Qwen3 Coder" }],
    },
    { id: "local", name: "本地模型", status: "待接入", enabledModels: "0", models: [] },
  ],
  reasoningSelection: {
    selectedEffort: "x-high",
    options: [
      { value: "off", label: "关闭" },
      { value: "high", label: "高" },
      { value: "x-high", label: "Max" },
    ],
  },
  aiCapabilities: [
    {
      kind: "web_search",
      label: "网页搜索",
      available: true,
      enabled: true,
      targets: [
        {
          targetSelector: "builtin/web-search",
          vendorId: "builtin",
          vendorName: "Builtin",
          targetAlias: "web-search",
          targetName: "Web Search",
          modelId: "native",
          enabled: true,
        },
      ],
    },
    {
      kind: "understand_image",
      label: "图片理解",
      available: false,
      enabled: false,
      targets: [],
    },
    {
      kind: "image_generation",
      label: "图片生成",
      available: false,
      enabled: false,
      targets: [],
    },
  ],
  memories: [
    { id: "user-profile", title: "用户画像", category: "Profile", state: "等待真实 VMM 节点接入" },
    { id: "project-memory", title: "项目记忆", category: "Project", state: "等待真实 VMM 节点接入" },
    { id: "conversation-memory", title: "会话记忆", category: "Conversation", state: "等待真实 VMM 节点接入" },
    { id: "conflict-review", title: "冲突审计", category: "Review", state: "等待真实 VMM 冲突视图接入" },
  ],
  vmmMemory: {
    effective: {
      enabled: true,
      grpcTarget: "127.0.0.1:50051",
      grpcConfigured: true,
      userId: 20000,
      projectId: 1001,
      diagnostic: "ready",
      precheckDiagnostic: "ready",
      aiToolsEnabled: true,
      precheckEnabled: true,
      implicitMemoryTurns: 5,
      profileRefreshTurns: 3,
      profileNodeLimit: 24,
      injectionTag: "vmm_inject",
      injectionDescription: "VMM recalled memory",
    },
    globalDraft: {
      hasOverride: true,
      enabled: true,
      grpcTarget: "127.0.0.1:50051",
      grpcApiKey: "",
      userId: "20000",
      projectId: "1001",
      receiveTimeoutMs: "30000",
      handshakeTimeoutMs: "8000",
      maxReceiveMessageBytes: "",
      grpcKeepaliveTimeMs: "30000",
      grpcKeepaliveTimeoutMs: "10000",
      grpcKeepalivePermitWithoutCalls: true,
      implicitMemoryTurns: "5",
      profileRefreshTurns: "3",
      profileNodeLimit: "24",
      aiToolsEnabled: true,
      precheckEnabled: true,
      injectionTag: "vmm_inject",
      injectionDescription: "VMM recalled memory",
    },
    projectDraft: {
      hasOverride: false,
      enabled: true,
      grpcTarget: "127.0.0.1:50051",
      grpcApiKey: "",
      userId: "20000",
      projectId: "1001",
      receiveTimeoutMs: "30000",
      handshakeTimeoutMs: "8000",
      maxReceiveMessageBytes: "",
      grpcKeepaliveTimeMs: "30000",
      grpcKeepaliveTimeoutMs: "10000",
      grpcKeepalivePermitWithoutCalls: true,
      implicitMemoryTurns: "5",
      profileRefreshTurns: "3",
      profileNodeLimit: "24",
      aiToolsEnabled: true,
      precheckEnabled: true,
      injectionTag: "vmm_inject",
      injectionDescription: "VMM recalled memory",
    },
    projectScopes: [
      { projectId: "vulcan-workmem", name: "Vulcan Code", path: "D:/openvulcan/vulcan-code", hasOverride: false },
    ],
    selectedProjectId: "vulcan-workmem",
    selectedConversationId: "preview-session",
    runtime: {
      outboxCount: 0,
      profileSyncCount: 3,
      completedTurns: 1,
      updatedAt: "1780908377000",
      profilePreview: "用户偏好：偏向工程化闭环。\n项目画像：WebUI 正在迁移 iced 功能。",
    },
    sections: [
      { id: "user_manager", key: "USER MANAGER", label: "用户管理", description: "解析、创建预检和删除长期用户。" },
      { id: "project_manager", key: "PROJECT MANAGER", label: "项目管理", description: "解析、创建预检、删除和迁移项目路径。" },
      { id: "profile_center", key: "PROFILE CENTER", label: "画像中心", description: "查看 active profile nodes 并应用手工画像指令。" },
      { id: "profile_bundle", key: "PROFILE BUNDLE TEST", label: "画像 Bundle 测试", description: "读取当前 user/project 的完整画像 Bundle。" },
      { id: "tools_debug", key: "TOOLS DEBUG", label: "工具调试", description: "模拟 AI 专属工具检索、详情展开和主动写入。" },
      { id: "memory_settings", key: "MEMORY SETTINGS", label: "记忆设置", description: "配置记忆开关、TTL、画像刷新和注入策略。" },
      { id: "grpc_transport", key: "GRPC TRANSPORT", label: "gRPC 传输", description: "配置全局 VMM 连接和传输参数。" },
    ],
    notice: "当前项目：Vulcan Code；诊断：ready；用户/项目：20000/1001。",
  },
  commandSessions: [
    { id: "preview-command", command: "pnpm --dir apps\\vulcan-code-web run build", status: "preview", preview: "前端预览命令会话" },
    { id: "preview-cargo", command: "cargo check -p vulcan-code-tauri", status: "preview", preview: "Tauri 桌面壳检查入口" },
  ],
  logs: [
    { id: "preview-log", level: "info", source: "webui", message: "浏览器预览模式已加载" },
    { id: "preview-legacy", level: "info", source: "iced", message: "旧 iced GUI 保留为 reference 入口" },
  ],
};

// browserFallbackWorkbenchState stores mutable preview data for non-Tauri browser development.
// browserFallbackWorkbenchState 保存非 Tauri 浏览器开发模式下的可变预览数据。
let browserFallbackWorkbenchState: WorkbenchSnapshot = cloneWorkbenchSnapshot(browserFallbackWorkbench);

// cloneWorkbenchSnapshot returns a detached workbench snapshot for React state updates.
// cloneWorkbenchSnapshot 返回用于 React 状态更新的脱离引用工作台快照。
function cloneWorkbenchSnapshot(snapshot: WorkbenchSnapshot): WorkbenchSnapshot {
  return structuredClone(snapshot) as WorkbenchSnapshot;
}

// browserFallbackSnapshot returns the current browser-preview snapshot.
// browserFallbackSnapshot 返回当前浏览器预览快照。
function browserFallbackSnapshot(): WorkbenchSnapshot {
  return cloneWorkbenchSnapshot(browserFallbackWorkbenchState);
}

// updateBrowserFallbackWorkbench mutates preview state and returns a fresh snapshot.
// updateBrowserFallbackWorkbench 修改预览状态并返回新的快照。
function updateBrowserFallbackWorkbench(mutator: (snapshot: WorkbenchSnapshot) => void): WorkbenchSnapshot {
  mutator(browserFallbackWorkbenchState);
  browserFallbackWorkbenchState.luaSkills.entryCount = browserFallbackWorkbenchState.luaSkills.skills
    .reduce((total, skill) => total + (skill.enabled ? skill.entryCount : 0), 0);
  return browserFallbackSnapshot();
}

// luaSkillRegistrationLabel returns the preview label for one LuaSkill tool registration state.
// luaSkillRegistrationLabel 返回单个 LuaSkill 工具注册状态的预览标签。
function luaSkillRegistrationLabel(registrationState: string): string {
  if (registrationState === "explicit") {
    return "显式启用";
  }
  if (registrationState === "implicit") {
    return "隐式启用";
  }
  return "禁用";
}

// loadStartupSnapshot loads startup state from the local service or fallback data.
// loadStartupSnapshot 从本地服务或回退数据加载启动状态。
async function loadStartupSnapshot(): Promise<StartupSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackStartup;
  }
  return serviceClient.request<StartupSnapshot>("startup.snapshot");
}

// loadWorkbenchSnapshot loads workbench state from the local service or fallback data.
// loadWorkbenchSnapshot 从本地服务或回退数据加载工作台状态。
async function loadWorkbenchSnapshot(): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot();
  }
  return serviceClient.request<WorkbenchSnapshot>("workbench.snapshot");
}

// loadWorkspaceDiffSnapshot loads a fresh Git diff snapshot for one project path.
// loadWorkspaceDiffSnapshot 为单个项目路径加载最新 Git diff 快照。
async function loadWorkspaceDiffSnapshot(
  workspacePath: string,
  projectId: string | null,
  conversationId: string | null,
  request: DiffReviewSelection,
): Promise<WorkspaceDiffSnapshotResponse> {
  if (!hasServiceRuntime()) {
    return {
      repositoryRoot: workspacePath,
      unifiedDiff: "",
      changedFiles: 0,
      additions: 0,
      deletions: 0,
      notice: "浏览器预览模式没有连接本地 Git diff 服务。",
    };
  }
  return serviceClient.request<WorkspaceDiffSnapshotResponse>("workspace.diff.snapshot", {
    workspacePath,
    projectId,
    conversationId,
    request,
  });
}

// refreshLuaSkillsCatalog refreshes the Lua Skills catalog through the local service.
// refreshLuaSkillsCatalog 通过本地服务刷新 Lua Skills 目录。
async function refreshLuaSkillsCatalog(): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot();
  }
  return serviceClient.request<WorkbenchSnapshot>("service_refresh_luaskills_catalog");
}

// installLuaSkill installs one Lua Skill package through the local service.
// installLuaSkill 通过本地服务安装单个 Lua Skill 包。
async function installLuaSkill(skillId: string, source: string, sourceType: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    const normalizedSkillId = skillId.trim() || source.split("/").filter(Boolean).pop() || "preview-skill";
    return updateBrowserFallbackWorkbench((snapshot) => {
      if (snapshot.luaSkills.skills.some((skill) => skill.skillId === normalizedSkillId)) {
        return;
      }
      snapshot.luaSkills.skills.push({
        skillId: normalizedSkillId,
        displayName: normalizedSkillId,
        version: "0.1.0",
        rootName: "USER",
        skillDir: `C:/Users/20000/.vulcan/code/luaskills-user/skills/${normalizedSkillId}`,
        enabled: true,
        managed: true,
        sourceType,
        sourceLocator: source,
        entryCount: 0,
        systemOwned: false,
        tools: [],
      });
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_install_luaskill", { skillId: skillId.trim() || null, source, sourceType });
}

// updateLuaSkill updates one managed Lua Skill package.
// updateLuaSkill 更新单个受管 Lua Skill 包。
async function updateLuaSkill(skillId: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      if (skill) {
        skill.version = skill.version || "0.1.0";
      }
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_update_luaskill", { skillId });
}

// setLuaSkillEnabled toggles one Lua Skill package lifecycle state.
// setLuaSkillEnabled 切换单个 Lua Skill 包生命周期状态。
async function setLuaSkillEnabled(skillId: string, enabled: boolean): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      if (skill) {
        skill.enabled = enabled;
        skill.disabledReason = enabled ? null : "disabled from browser preview";
      }
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_luaskill_enabled", { skillId, enabled });
}

// uninstallLuaSkill removes one ordinary Lua Skill package.
// uninstallLuaSkill 移除单个普通 Lua Skill 包。
async function uninstallLuaSkill(skillId: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.luaSkills.skills = snapshot.luaSkills.skills.filter((skill) => skill.skillId !== skillId || skill.systemOwned);
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_uninstall_luaskill", { skillId });
}

// setLuaSkillToolRegistrationState persists one Lua Skill tool visibility state.
// setLuaSkillToolRegistrationState 持久化单个 Lua Skill 工具可见性状态。
async function setLuaSkillToolRegistrationState(skillId: string, localName: string, registrationState: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      const tool = skill?.tools.find((item) => item.localName === localName);
      if (tool) {
        tool.registrationState = registrationState as "explicit" | "implicit" | "disabled";
        tool.registrationLabel = luaSkillRegistrationLabel(registrationState);
      }
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_luaskill_tool_registration_state", { skillId, localName, registrationState });
}

// refreshAgentSkillsCatalog refreshes prompt-based Agent Skills through the local service.
// refreshAgentSkillsCatalog 通过本地服务刷新基于提示词的 Agent Skills。
async function refreshAgentSkillsCatalog(): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot();
  }
  return serviceClient.request<WorkbenchSnapshot>("service_refresh_agent_skills_catalog");
}

// installSystemAgentSkills syncs bundled system Agent Skills.
// installSystemAgentSkills 同步内置系统 Agent Skills。
async function installSystemAgentSkills(): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot();
  }
  return serviceClient.request<WorkbenchSnapshot>("service_install_system_agent_skills");
}

// setAgentSkillsEnabled persists the global Agent Skills switch.
// setAgentSkillsEnabled 持久化 Agent Skills 全局开关。
async function setAgentSkillsEnabled(enabled: boolean): Promise<AgentSkillsCatalogSummary> {
  if (!hasServiceRuntime()) {
    const snapshot = updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.agentSkills.globalEnabled = enabled;
      snapshot.agentSkills.skills = snapshot.agentSkills.skills.map((skill) => ({ ...skill, enabled }));
    });
    return snapshot.agentSkills;
  }
  return serviceClient.request<AgentSkillsCatalogSummary>("service_set_agent_skills_enabled", { enabled });
}

// setAgentSkillEnabled persists one global Agent Skill state.
// setAgentSkillEnabled 持久化单个全局 Agent Skill 状态。
async function setAgentSkillEnabled(name: string, enabled: boolean): Promise<AgentSkillsCatalogSummary> {
  if (!hasServiceRuntime()) {
    const snapshot = updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.agentSkills.skills.find((item) => item.name === name);
      if (skill) {
        skill.enabled = enabled && snapshot.agentSkills.globalEnabled;
      }
    });
    return snapshot.agentSkills;
  }
  return serviceClient.request<AgentSkillsCatalogSummary>("service_set_agent_skill_enabled", { name, enabled });
}

// loadProjectAgentSkillOverrides loads project-local Agent Skill overrides.
// loadProjectAgentSkillOverrides 加载项目本地 Agent Skill 覆盖。
async function loadProjectAgentSkillOverrides(projectId: string): Promise<Record<string, string>> {
  if (!hasServiceRuntime()) {
    return {};
  }
  return serviceClient.request<Record<string, string>>("service_load_project_agent_skill_overrides", { projectId });
}

// setAgentSkillProjectState persists one project-level Agent Skill tri-state override.
// setAgentSkillProjectState 持久化单个项目级 Agent Skill 三态覆盖。
async function setAgentSkillProjectState(projectId: string, name: string, stateKey: ProjectAgentSkillState): Promise<void> {
  if (!hasServiceRuntime()) {
    return;
  }
  return serviceClient.request<void>("service_set_agent_skill_project_state", { projectId, name, stateKey });
}

// runVmmMemoryAction executes one VMM memory workbench action.
// runVmmMemoryAction 执行单个 VMM 记忆工作台动作。
async function runVmmMemoryAction(request: VmmMemoryActionRequest): Promise<VmmMemoryActionResult> {
  if (!hasServiceRuntime()) {
    return {
      action: request.action,
      summary: `预览模式已执行 ${request.action}`,
      kind: request.action,
      request,
      response: previewVmmActionResponse(request.action),
    };
  }
  return serviceClient.request<VmmMemoryActionResult>("service_run_vmm_memory_action", { request });
}

// loadProjectVmmConfig loads the editable project-scoped VMM draft.
// loadProjectVmmConfig 加载可编辑的项目级 VMM 草稿。
async function loadProjectVmmConfig(projectId: string): Promise<VmmConfigDraftSummary> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot().vmmMemory.projectDraft;
  }
  return serviceClient.request<VmmConfigDraftSummary>("service_load_project_vmm_config", { projectId });
}

// previewProjectAttachmentCleanup builds a project-scoped managed attachment cleanup preview.
// previewProjectAttachmentCleanup 构建项目级托管附件清理预览。
async function previewProjectAttachmentCleanup(request: ProjectAttachmentCleanupRequest): Promise<ProjectAttachmentCleanupPreviewSummary> {
  if (!hasServiceRuntime()) {
    return projectAttachmentPreviewFallback(request);
  }
  return serviceClient.request<ProjectAttachmentCleanupPreviewSummary>("service_preview_project_attachment_cleanup", { request });
}

// executeProjectAttachmentCleanup runs one project-scoped managed attachment cleanup.
// executeProjectAttachmentCleanup 执行单次项目级托管附件清理。
async function executeProjectAttachmentCleanup(request: ProjectAttachmentCleanupRequest): Promise<ProjectAttachmentCleanupReportSummary> {
  if (!hasServiceRuntime()) {
    return {
      cutoffDate: request.cutoffDate,
      purgedAssetCount: 0,
      deletedFileCount: 0,
      alreadyMissingFileCount: 0,
      failedDeleteCount: 0,
      purgedByteLenLabel: "0 B",
      impactedSessionCount: 0,
      summary: "预览模式不会删除托管附件。",
    };
  }
  return serviceClient.request<ProjectAttachmentCleanupReportSummary>("service_execute_project_attachment_cleanup", { request });
}

// saveVmmMemoryConfig persists one VMM memory configuration draft.
// saveVmmMemoryConfig 持久化单个 VMM 记忆配置草稿。
async function saveVmmMemoryConfig(request: VmmConfigSaveRequest): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      if (request.scope === "project") {
        snapshot.vmmMemory.projectDraft = { ...request.draft, hasOverride: request.draft.hasOverride };
      } else {
        snapshot.vmmMemory.globalDraft = { ...request.draft, hasOverride: true };
      }
      snapshot.vmmMemory.effective = {
        ...snapshot.vmmMemory.effective,
        enabled: request.draft.enabled,
        grpcTarget: request.draft.grpcTarget,
        grpcConfigured: request.draft.grpcTarget.trim().length > 0,
        userId: Number(request.draft.userId) || null,
        projectId: Number(request.draft.projectId) || null,
        aiToolsEnabled: request.draft.aiToolsEnabled,
        precheckEnabled: request.draft.precheckEnabled,
        implicitMemoryTurns: Number(request.draft.implicitMemoryTurns) || snapshot.vmmMemory.effective.implicitMemoryTurns,
        profileRefreshTurns: Number(request.draft.profileRefreshTurns) || snapshot.vmmMemory.effective.profileRefreshTurns,
        profileNodeLimit: Number(request.draft.profileNodeLimit) || snapshot.vmmMemory.effective.profileNodeLimit,
        injectionTag: request.draft.injectionTag,
        injectionDescription: request.draft.injectionDescription,
      };
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_vmm_memory_config", { request });
}

// previewVmmActionResponse returns a compact browser-preview result for one VMM action.
// previewVmmActionResponse 返回单个 VMM 动作的紧凑浏览器预览结果。
function previewVmmActionResponse(action: string): unknown {
  if (action === "load_users_projects") {
    return {
      users: [{ userId: 20000, userName: "preview-user" }],
      projects: [{ projectId: 1001, displayPath: "OpenVulcan/Vulcan/Code" }],
    };
  }
  if (action === "load_profile_nodes") {
    return {
      nodes: [
        { profileNodeId: 1, content: "用户偏好工程化闭环。", priority: "high", level: "user", bindId: 20000 },
        { profileNodeId: 2, content: "当前项目正在迁移 WebUI。", priority: "normal", level: "project", bindId: 1001 },
      ],
    };
  }
  if (action === "search_memory_events") {
    return {
      results: [{ query: "webui", hits: [{ memoryId: 1, sourceTurnId: 42, abstractText: "技能页迁移到 WebUI。" }] }],
      traceId: "preview-trace",
    };
  }
  return { ok: true, preview: true };
}

// loadConversationMessages loads one conversation body from the local service.
// loadConversationMessages 从本地服务加载单个会话正文。
async function loadConversationMessages(sessionId: string): Promise<ConversationMessage[]> {
  if (!hasServiceRuntime()) {
    return browserFallbackWorkbenchState.messages.filter((message) => message.conversationId === sessionId);
  }
  return serviceClient.request<ConversationMessage[]>("service_session_messages", { sessionId });
}

// loadConversationToolDetail loads one hidden structured tool result detail on demand.
// loadConversationToolDetail 按需加载单条隐藏结构化工具结果详情。
async function loadConversationToolDetail(detailRef: ConversationDetailRef): Promise<ToolDetailPayload | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<ToolDetailPayload>("service_session_tool_detail", {
    sessionId: detailRef.sessionId,
    recordId: detailRef.recordId ?? null,
    toolCallId: detailRef.toolCallId ?? null,
    seq: detailRef.seq ?? null,
    messageIndex: detailRef.messageIndex ?? null,
  });
}

// conversationDetailRefCacheKey builds a stable cache key for one hidden tool detail reference.
// conversationDetailRefCacheKey 为单条隐藏工具详情引用构建稳定缓存键。
function conversationDetailRefCacheKey(detailRef: ConversationDetailRef): string {
  const locator = detailRef.recordId
    ? `record:${detailRef.recordId}`
    : detailRef.seq != null
      ? `seq:${detailRef.seq}`
      : detailRef.toolCallId
        ? `tool:${detailRef.toolCallId}`
        : detailRef.messageIndex != null
          ? `message:${detailRef.messageIndex}`
          : "unknown";
  return `${detailRef.sessionId}:${locator}`;
}

// loadCachedConversationToolDetail loads hidden tool detail once per stable reference.
// loadCachedConversationToolDetail 按稳定引用仅加载一次隐藏工具详情。
async function loadCachedConversationToolDetail(detailRef: ConversationDetailRef): Promise<ToolDetailPayload | null> {
  const cacheKey = conversationDetailRefCacheKey(detailRef);
  if (conversationToolDetailPayloadCache.has(cacheKey)) {
    return conversationToolDetailPayloadCache.get(cacheKey) ?? null;
  }
  const existingPromise = conversationToolDetailPromiseCache.get(cacheKey);
  if (existingPromise) {
    return existingPromise;
  }
  const promise = loadConversationToolDetail(detailRef)
    .then((payload) => {
      conversationToolDetailPayloadCache.set(cacheKey, payload);
      conversationToolDetailPromiseCache.delete(cacheKey);
      return payload;
    })
    .catch((error) => {
      conversationToolDetailPromiseCache.delete(cacheKey);
      throw error;
    });
  conversationToolDetailPromiseCache.set(cacheKey, promise);
  return promise;
}

// submitConversationPrompt sends one composer prompt to the desktop conversation runtime.
// submitConversationPrompt 将单条输入区提示词发送到桌面会话运行时。
async function submitConversationPrompt(
  projectId: string | null,
  conversationId: string | null,
  prompt: string,
  attachments: ComposerAttachmentSummary[],
  executionMode: string,
  reasoningEffort: string,
  disabledAiCapabilityKinds: AiCapabilityKind[],
  disabledAiCapabilityTargetSelectors: string[],
  aiCapabilityTargetOrderOverrides: Record<string, string[]>,
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_submit_conversation_prompt", {
    projectId,
    conversationId,
    prompt,
    attachments,
    executionMode,
    reasoningEffort,
    disabledAiCapabilityKinds,
    disabledAiCapabilityTargetSelectors,
    aiCapabilityTargetOrderOverrides,
  });
}

// stopConversationRun stops the selected backend conversation run.
// stopConversationRun 停止当前选中的后端会话运行。
async function stopConversationRun(): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_stop_conversation_run");
}

// compactSelectedConversationContext starts backend context compaction for the selected conversation.
// compactSelectedConversationContext 为当前选中会话启动后端上下文压缩。
async function compactSelectedConversationContext(): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_compact_selected_conversation_context");
}

// replyPendingQuestion submits ask-tool answers to the blocked backend runtime.
// replyPendingQuestion 将 ask 工具答案提交给正在阻塞的后端运行时。
async function replyPendingQuestion(requestId: string, answers: string[][]): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_reply_pending_question", { requestId, answers });
}

// rejectPendingQuestion cancels one pending ask-tool request after explicit confirmation.
// rejectPendingQuestion 在明确确认后取消一个待处理 ask 工具请求。
async function rejectPendingQuestion(requestId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_reject_pending_question", { requestId });
}

// openSubagentPanel asks the backend to switch the active transcript surface to a child task.
// openSubagentPanel 请求后端把活动转录界面切换到子任务。
async function openSubagentPanel(parentConversationId: string, taskId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_open_subagent_panel", { parentConversationId, taskId });
}

// closeSubagentPanel asks the backend to return the active transcript surface to the parent conversation.
// closeSubagentPanel 请求后端将活动转录界面返回父会话。
async function closeSubagentPanel(): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_close_subagent_panel");
}

// stopSubagentTask asks the backend to stop one child task through the shared task runtime.
// stopSubagentTask 请求后端通过共享任务运行时停止单个子任务。
async function stopSubagentTask(parentConversationId: string, taskId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_stop_subagent_task", { parentConversationId, taskId });
}

// editPendingPrompt restores one queued prompt from the selected conversation queue.
// editPendingPrompt 从当前选中会话队列恢复单条排队提示词。
async function editPendingPrompt(entryId: string): Promise<PendingPromptSummary | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<PendingPromptSummary>("service_edit_pending_prompt", { entryId });
}

// insertPendingPrompt routes one visible queue item through the shared pending-prompt router.
// insertPendingPrompt 通过共享待发送提示词路由器插入单条可见队列项。
async function insertPendingPrompt(entryId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_insert_pending_prompt", { entryId });
}

// removePendingPrompt removes one queued prompt from the selected conversation queue.
// removePendingPrompt 从当前选中会话队列移除单条排队提示词。
async function removePendingPrompt(entryId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_pending_prompt", { entryId });
}

// reorderPendingPrompts persists one iced-compatible pending prompt drop target.
// reorderPendingPrompts 持久化一次与 iced 兼容的待发送提示词拖放落点。
async function reorderPendingPrompts(
  conversationId: string | null,
  draggedEntryId: string,
  targetEntryId: string | null,
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_reorder_pending_prompts", { conversationId, draggedEntryId, targetEntryId });
}

// stageComposerAttachmentBytes stages one browser/drop/paste provided file through the service.
// stageComposerAttachmentBytes 通过服务暂存一个由浏览器、拖拽或粘贴提供的文件。
async function stageComposerAttachmentBytes(
  projectId: string | null,
  conversationId: string | null,
  file: File,
): Promise<ComposerAttachmentSummary | null> {
  if (!hasServiceRuntime()) {
    throw new Error("浏览器服务不可用，无法暂存附件。");
  }
  const dataBase64 = await fileToBase64(file);
  return serviceClient.request<ComposerAttachmentSummary>("service_stage_composer_attachment_bytes", {
    projectId,
    conversationId,
    fileName: file.name || "clipboard-attachment",
    mimeType: file.type || "",
    dataBase64,
  });
}

// MAX_COMPOSER_ATTACHMENTS matches the iced composer attachment limit.
// MAX_COMPOSER_ATTACHMENTS 对齐 iced 输入区附件数量上限。
const MAX_COMPOSER_ATTACHMENTS = 10;

// normalizedComposerSlashCommandId returns the stable action id for a projected command.
// normalizedComposerSlashCommandId 返回投影指令对应的稳定动作标识。
function normalizedComposerSlashCommandId(command: ComposerSlashCommandSummary): string {
  const id = command.id.trim().toLowerCase();
  if (id) {
    return id;
  }
  return command.command.replace(/^\/+/, "").trim().toLowerCase();
}

// composerSlashCommandDescription returns the locale-specific description for one command row.
// composerSlashCommandDescription 返回单个指令行的当前语言说明。
function composerSlashCommandDescription(command: ComposerSlashCommandSummary, locale: LocaleCode): string {
  return locale === "en-US" ? command.descriptionEn : command.descriptionZh;
}

// composerSlashCommandMatchesInput mirrors the native slash-command prefix matching rule.
// composerSlashCommandMatchesInput 对齐原生 slash 指令前缀匹配规则。
function composerSlashCommandMatchesInput(command: ComposerSlashCommandSummary, input: string): boolean {
  if (!input.startsWith("/")) {
    return false;
  }
  const filterText = input.slice(1).trim().toLowerCase();
  const trigger = command.command.replace(/^\/+/, "").toLowerCase();
  return trigger.startsWith(filterText);
}

// exactComposerSlashCommand finds a command whose trigger exactly equals the typed input.
// exactComposerSlashCommand 查找触发词与当前输入完全相等的指令。
function exactComposerSlashCommand(
  commands: ComposerSlashCommandSummary[],
  input: string,
): ComposerSlashCommandSummary | null {
  const normalizedInput = input.trim();
  return commands.find((command) => command.command.localeCompare(normalizedInput, undefined, { sensitivity: "accent" }) === 0) ?? null;
}

// isComposerTextEntryElement detects native or rich text-entry targets that should keep focus.
// isComposerTextEntryElement 检测应保留焦点的原生或富文本输入目标。
function isComposerTextEntryElement(element: Element | null): boolean {
  if (!element) {
    return false;
  }
  if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) {
    return true;
  }
  if (element instanceof HTMLElement && element.isContentEditable) {
    return true;
  }
  return Boolean(element.closest('[contenteditable="true"], [role="textbox"]'));
}

// hasBlockingComposerPopup detects modal, menu, or popover surfaces that own keyboard input.
// hasBlockingComposerPopup 检测拥有键盘输入权的模态框、菜单或弹出层。
function hasBlockingComposerPopup(): boolean {
  if (typeof document === "undefined") {
    return false;
  }
  return Boolean(document.querySelector([
    '[role="dialog"]',
    '[role="alertdialog"]',
    '[role="menu"]',
    '[role="listbox"]',
  ].join(",")));
}

// isComposerPrintableKey returns whether a global key event should become prompt text.
// isComposerPrintableKey 返回全局键盘事件是否应转换为提示词文本。
function isComposerPrintableKey(event: KeyboardEvent): boolean {
  return event.key.length === 1
    && !event.ctrlKey
    && !event.metaKey
    && !event.altKey
    && !event.isComposing;
}

// fileToBase64 converts a browser File into a standard base64 payload.
// fileToBase64 将浏览器 File 转换为标准 base64 载荷。
async function fileToBase64(file: File): Promise<string> {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunkSize));
  }
  return btoa(binary);
}

// inferComposerAttachmentKindFromFile maps MIME type and extension to a composer kind.
// inferComposerAttachmentKindFromFile 将 MIME 类型与扩展名映射到输入区附件类型。
function inferComposerAttachmentKindFromFile(file: File): string | null {
  const mimeType = file.type.toLowerCase();
  const name = file.name.toLowerCase();
  if (mimeType.startsWith("image/") || /\.(png|jpe?g|webp|gif|bmp)$/.test(name)) return "image";
  if (mimeType.startsWith("audio/") || /\.(mp3|wav|m4a|aac|flac|ogg)$/.test(name)) return "audio";
  if (mimeType.startsWith("video/") || /\.(mp4|mov|mkv|webm|avi)$/.test(name)) return "video";
  if (mimeType === "application/pdf" || /\.pdf$/.test(name)) return "pdf";
  return null;
}

// composerAttachmentMedia converts a staged attachment into a reusable media preview descriptor.
// composerAttachmentMedia 将已暂存附件转换为可复用的媒体预览描述。
function composerAttachmentMedia(attachment: ComposerAttachmentSummary): GeneratedMediaAttachment | null {
  const kind = attachment.kind === "pdf" ? "document" : attachment.kind;
  if (kind !== "image" && kind !== "audio" && kind !== "video" && kind !== "document") {
    return null;
  }
  return {
    kind,
    fileName: attachment.fileName,
    localPath: attachment.localPath,
    previewLocalPath: attachment.previewLocalPath ?? undefined,
    mimeType: attachment.mimeType,
  };
}

// composerAttachmentAccept joins browser accept strings for enabled attachment capabilities.
// composerAttachmentAccept 拼接已启用附件能力的浏览器 accept 字符串。
function composerAttachmentAccept(capabilities: ComposerAttachmentCapabilitySummary[]): string {
  return capabilities
    .filter((capability) => capability.enabled)
    .map((capability) => capability.accept)
    .filter(Boolean)
    .join(",");
}

// selectWorkbenchModel persists the active provider/model pair through the desktop service.
// selectWorkbenchModel 通过桌面服务持久化当前激活的 Provider/模型组合。
async function selectWorkbenchModel(choice: ModelPickerChoice): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_workbench_model", {
    providerId: choice.providerId,
    modelId: choice.modelId,
  });
}

// selectWorkbenchReasoningEffort persists one reasoning selection through the desktop service router.
// selectWorkbenchReasoningEffort 通过桌面服务路由持久化单个思考档位选择。
async function selectWorkbenchReasoningEffort(effort: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_workbench_reasoning_effort", { effort });
}

// saveProviderConfiguration persists one provider form through the desktop service.
// saveProviderConfiguration 通过桌面服务持久化单个 Provider 表单。
async function saveProviderConfiguration(request: { providerId: string; fields: Record<string, string> }): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_provider_configuration", { request });
}

// removeProviderConfiguration removes one provider through the desktop service.
// removeProviderConfiguration 通过桌面服务移除单个 Provider。
async function removeProviderConfiguration(providerId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_provider_configuration", { providerId });
}

// saveProviderEnabledModels persists one provider enabled-model list.
// saveProviderEnabledModels 持久化单个 Provider 的启用模型列表。
async function saveProviderEnabledModels(request: { providerId: string; enabledModelIds: string[] }): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_provider_enabled_models", { request });
}

// saveProviderModelOverride persists one provider model capability override.
// saveProviderModelOverride 持久化单个 Provider 模型能力覆盖。
async function saveProviderModelOverride(request: {
  providerId: string;
  modelId: string;
  reasoning: boolean | null;
  attachment: boolean | null;
  contextLimit: number | null;
  inputLimit: number | null;
  outputLimit: number | null;
  inputModalities: string[] | null;
  outputModalities: string[] | null;
  reasoningProfileJson: string | null;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_provider_model_override", { request });
}

// saveAiCapabilityVendor persists one shared AI capability vendor.
// saveAiCapabilityVendor 持久化单个共享 AI 能力供应商。
async function saveAiCapabilityVendor(request: {
  providerId?: never;
  vendorId: string;
  enabled: boolean;
  apiKey: string;
  baseUrl: string;
  region: string;
  connectionMode?: string | null;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_ai_capability_vendor", { request });
}

// removeAiCapabilityVendor removes one shared AI capability vendor.
// removeAiCapabilityVendor 移除单个共享 AI 能力供应商。
async function removeAiCapabilityVendor(vendorId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_ai_capability_vendor", { vendorId });
}

// setAiCapabilityTargetEnabled persists one AI capability target enabled state.
// setAiCapabilityTargetEnabled 持久化单个 AI 能力目标启用状态。
async function setAiCapabilityTargetEnabled(request: { capabilityKind: string; vendorId: string; targetAlias: string; enabled: boolean }): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_ai_capability_target_enabled", { request });
}

// saveAiCapabilityTargets persists one ordered AI capability target list.
// saveAiCapabilityTargets 持久化单个有序 AI 能力目标列表。
async function saveAiCapabilityTargets(request: {
  capabilityKind: string;
  targets: Array<{ vendorId: string; targetAlias: string; modelId: string; enabled: boolean }>;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_ai_capability_targets", { request });
}

// saveMcpServerConfiguration persists one MCP server through the desktop service.
// saveMcpServerConfiguration 通过桌面服务持久化单个 MCP 服务器。
async function saveMcpServerConfiguration(request: Record<string, unknown>): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_mcp_server_configuration", { request });
}

// removeMcpServerConfiguration removes one MCP server through the desktop service.
// removeMcpServerConfiguration 通过桌面服务移除单个 MCP 服务器。
async function removeMcpServerConfiguration(serverId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_mcp_server_configuration", { serverId });
}

// openProjectDirectoryPath asks the service to reveal one filesystem folder.
// openProjectDirectoryPath 请求服务打开单个文件系统文件夹。
async function openProjectDirectoryPath(path: string): Promise<string | null> {
  return serviceClient.request<string>("service_open_project_directory_path", { path });
}

// openProjectDirectory asks the service to reveal one project folder.
// openProjectDirectory 请求服务打开单个项目文件夹。
async function openProjectDirectory(projectId: string): Promise<string | null> {
  return serviceClient.request<string>("service_open_project_directory", { projectId });
}

// addProjectFromFolder asks the desktop shell to pick a folder and register it as a project.
// addProjectFromFolder 请求桌面壳选择一个文件夹并注册为项目。
async function addProjectFromFolder(): Promise<WorkbenchSnapshot | null> {
  return shellClient.addProjectFromFolder();
}

// toggleProjectPinned asks the desktop shell to toggle one project pinned state.
// toggleProjectPinned 请求桌面壳切换单个项目的置顶状态。
async function toggleProjectPinned(projectId: string): Promise<void> {
  if (!hasServiceRuntime()) {
    return;
  }
  await serviceClient.request<void>("service_toggle_project_pinned", { projectId });
}

// reorderProjects asks the desktop shell to persist one same-section project order.
// reorderProjects 请求桌面壳持久化同一分区内的项目顺序。
async function reorderProjects(projectIds: string[]): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_reorder_projects", { projectIds });
}

// renameProject asks the desktop shell to apply a new project display name.
// renameProject 请求桌面壳应用新的项目显示名称。
async function renameProject(projectId: string, name: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_rename_project", { projectId, name });
}

// setProjectIcon asks the desktop shell to persist one project icon override.
// setProjectIcon 请求桌面壳持久化单个项目图标覆盖。
async function setProjectIcon(projectId: string, icon: ProjectIconSelection): Promise<void> {
  if (!hasServiceRuntime()) {
    return;
  }
  await serviceClient.request<void>("service_set_project_icon", {
    projectId,
    iconPack: icon.pack,
    iconKey: icon.key,
  });
}

// resetProjectIcon asks the desktop shell to clear one project icon override.
// resetProjectIcon 请求桌面壳清除单个项目图标覆盖。
async function resetProjectIcon(projectId: string): Promise<void> {
  if (!hasServiceRuntime()) {
    return;
  }
  await serviceClient.request<void>("service_reset_project_icon", { projectId });
}

// setExpandedTaskProjects asks the desktop shell to persist expanded project rows.
// setExpandedTaskProjects 请求桌面壳持久化已展开的项目行。
async function setExpandedTaskProjects(projectIds: string[]): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_expanded_task_projects", { projectIds });
}

// archiveProjectConversations asks the desktop shell to archive all conversations under one project.
// archiveProjectConversations 请求桌面壳归档单个项目下的全部会话。
async function archiveProjectConversations(projectId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_archive_project_conversations", { projectId });
}

// renameConversation asks the desktop shell to apply a new conversation display name.
// renameConversation 请求桌面壳应用新的会话显示名称。
async function renameConversation(conversationId: string, name: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_rename_conversation", { conversationId, name });
}

// archiveConversation asks the desktop shell to archive one conversation.
// archiveConversation 请求桌面壳归档单个会话。
async function archiveConversation(conversationId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_archive_conversation", { conversationId });
}

// markConversationRead asks the desktop shell to persist one conversation as read.
// markConversationRead 请求桌面壳将单个会话持久化标记为已读。
async function markConversationRead(conversationId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_mark_conversation_read", { conversationId });
}

// selectConversation asks the desktop shell to update its active conversation marker.
// selectConversation 请求桌面壳同步当前活跃会话标记。
async function selectConversation(conversationId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_conversation", { conversationId });
}

// openExternalUrl asks the service to open one trusted external URL.
// openExternalUrl 请求服务打开单个可信外部链接。
async function openExternalUrl(url: string): Promise<void> {
  await serviceClient.request<string>("service_open_external_url", { url });
}

// App renders the Vulcan Code WebUI workbench.
// App 渲染 Vulcan Code WebUI 工作台。
function App() {
  // startup stores the latest startup snapshot.
  // startup 保存最近一次启动快照。
  const [startup, setStartup] = React.useState<StartupSnapshot | null>(null);
  // workbench stores the latest workbench snapshot.
  // workbench 保存最近一次工作台快照。
  const [workbench, setWorkbench] = React.useState<WorkbenchSnapshot | null>(null);
  // workbenchDelta stores the latest partial workbench update signal.
  // workbenchDelta 保存最近一次局部工作台更新信号。
  const [workbenchDelta, setWorkbenchDelta] = React.useState<WorkbenchDeltaSignal | null>(null);
  // workbenchDeltaBufferRef batches hot-path deltas until the next scheduled flush.
  // workbenchDeltaBufferRef 将热路径增量批处理到下一次计划刷新。
  const workbenchDeltaBufferRef = React.useRef<WorkbenchDeltaEnvelope[]>([]);
  // workbenchDeltaTimerRef stores the scheduled delta flush timer id.
  // workbenchDeltaTimerRef 保存已调度的增量刷新定时器标识。
  const workbenchDeltaTimerRef = React.useRef<number | null>(null);
  // error stores the latest loading error.
  // error 保存最近一次加载错误。
  const [error, setError] = React.useState<string | null>(null);
  // serviceWasDisconnectedRef suppresses recovery toasts before the first real disconnect.
  // serviceWasDisconnectedRef 在首次真实断开前抑制恢复提示。
  const serviceWasDisconnectedRef = React.useRef(false);

  React.useEffect(() => {
    // handleConnectionStatusChange maps transport-level connection changes to user-visible toasts.
    // handleConnectionStatusChange 将传输层连接变化映射为用户可见的提示。
    function handleConnectionStatusChange(event: Event) {
      const detail = (event as CustomEvent<BrowserServiceConnectionDetail>).detail;
      if (!detail) {
        return;
      }
      if (detail.status === "connected") {
        if (serviceWasDisconnectedRef.current) {
          serviceWasDisconnectedRef.current = false;
          toast.success("本地服务连接已恢复。", {
            id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
            duration: 2200,
          });
        }
        return;
      }
      serviceWasDisconnectedRef.current = true;
      if (detail.status === "reconnecting") {
        toast.loading("本地服务连接已断开，正在重连...", {
          id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
        });
        return;
      }
      toast.loading("本地服务连接已断开，正在检测恢复...", {
        id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
        description: detail.message,
      });
    }

    window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatusChange);
    return () => {
      window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatusChange);
    };
  }, []);

  React.useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    let retryTimer: number | null = null;
    let retryDelayMs = 500;

    // scheduleBootstrapRetry retries service discovery after transient startup races.
    // scheduleBootstrapRetry 在瞬时启动竞态后重试服务发现。
    function scheduleBootstrapRetry(error: unknown): boolean {
      if (!hasServiceRuntime() || !serviceBootstrapErrorIsRetryable(error)) {
        return false;
      }
      if (retryTimer !== null) {
        return true;
      }
      const delay = retryDelayMs;
      retryDelayMs = Math.min(retryDelayMs * 2, 5000);
      retryTimer = window.setTimeout(() => {
        retryTimer = null;
        void bootstrap();
      }, delay);
      return true;
    }

    // bootstrap loads the first startup and workbench snapshots.
    // bootstrap 加载第一份启动和工作台快照。
    async function bootstrap() {
      try {
        if (hasServiceRuntime()) {
          const subscriptions = await Promise.all([
            serviceClient.subscribe<StartupSnapshot>("startup", (payload) => {
              if (!cancelled) {
                setStartup(payload);
              }
            }),
            serviceClient.subscribe<WorkbenchSnapshot>("workbench", (payload) => {
              if (!cancelled) {
                setWorkbench(payload);
              }
            }),
            serviceClient.subscribe<WorkbenchDeltaEnvelope>("workbench.delta", (payload) => {
              if (cancelled) {
                return;
              }
              workbenchDeltaBufferRef.current.push(payload);
              if (workbenchDeltaTimerRef.current === null) {
                workbenchDeltaTimerRef.current = window.setTimeout(() => {
                  const envelopes = coalesceWorkbenchDeltaEnvelopes(workbenchDeltaBufferRef.current);
                  workbenchDeltaBufferRef.current = [];
                  workbenchDeltaTimerRef.current = null;
                  if (envelopes.length > 0) {
                    setWorkbenchDelta((current) => ({
                      seq: (current?.seq ?? 0) + 1,
                      envelopes,
                    }));
                  }
                }, STREAMING_DELTA_FLUSH_MS);
              }
            }),
          ]);
          if (cancelled) {
            subscriptions.forEach((unsubscribe) => unsubscribe());
          } else {
            unlisten = () => {
              subscriptions.forEach((unsubscribe) => unsubscribe());
            };
          }
        }
        const [nextStartup, nextWorkbench] = await Promise.all([
          loadStartupSnapshot(),
          loadWorkbenchSnapshot(),
        ]);
        if (!cancelled) {
          retryDelayMs = 500;
          setError(null);
          setStartup(nextStartup);
          setWorkbench(nextWorkbench);
        }
      } catch (nextError) {
        if (!cancelled && scheduleBootstrapRetry(nextError)) {
          setError(null);
          return;
        }
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : String(nextError));
        }
      }
    }

    void bootstrap();
    return () => {
      cancelled = true;
      if (workbenchDeltaTimerRef.current !== null) {
        window.clearTimeout(workbenchDeltaTimerRef.current);
        workbenchDeltaTimerRef.current = null;
      }
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
      unlisten?.();
    };
  }, []);

  if (error) {
    return <ErrorScreen error={error} />;
  }

  if (!startup || !workbench) {
    return (
      <I18nProvider>
        <LoadingScreen startup={startup} />
        <Toaster />
      </I18nProvider>
    );
  }

  return (
    <I18nProvider>
      <TooltipProvider>
        <WorkbenchLayout startup={startup} workbench={workbench} workbenchDelta={workbenchDelta} />
        <Toaster />
      </TooltipProvider>
    </I18nProvider>
  );
}

// LoadingScreen renders the earliest frontend loading state.
// LoadingScreen 渲染最早期的前端加载状态。
function LoadingScreen(props: {
  // Startup snapshot received before the workbench snapshot is ready.
  // 在工作台快照就绪前已经收到的启动快照。
  startup?: StartupSnapshot | null;
}) {
  return (
    <main className="loading-screen">
      {props.startup ? (
        <StartupProgressCard startup={props.startup} />
      ) : (
        <>
          <Activity className="spin" size={28} />
          <span>正在连接 Vulcan Core Service...</span>
        </>
      )}
    </main>
  );
}

// StartupOverlay renders the in-shell bootstrap blocker below the main header.
// StartupOverlay 渲染位于主标题栏下方的壳内启动阻塞层。
function StartupOverlay(props: {
  // Latest service startup snapshot.
  // 最近一次服务启动快照。
  startup: StartupSnapshot;
}) {
  return (
    <div className="startup-overlay" role="status" aria-live="polite" aria-busy="true">
      <StartupProgressCard startup={props.startup} />
    </div>
  );
}

// StartupProgressCard renders the blocking runtime bootstrap progress.
// StartupProgressCard 渲染阻塞式运行时启动进度。
function StartupProgressCard(props: {
  // Latest service startup snapshot.
  // 最近一次服务启动快照。
  startup: StartupSnapshot;
}) {
  const activeStep = activeStartupStep(props.startup.steps);
  return (
    <section className="startup-progress-card" aria-live="polite" aria-busy={!props.startup.ready}>
      <div className="startup-progress-heading">
        <Spinner className="size-5" />
        <div>
          <strong>{activeStep ? `正在加载 ${activeStep.label}` : "正在加载 Vulcan 运行时"}</strong>
          <span>{activeStep?.message ?? "正在加载 VLDB、LuaSkills 与 Token 预算运行时。"}</span>
        </div>
      </div>
      <div className="startup-progress-steps">
        {props.startup.steps.map((step) => (
          <StartupProgressStep key={step.id} step={step} />
        ))}
      </div>
    </section>
  );
}

// StartupProgressStep renders one runtime bootstrap row.
// StartupProgressStep 渲染单个运行时启动步骤行。
function StartupProgressStep(props: {
  // Step snapshot displayed by this row.
  // 此行展示的步骤快照。
  step: StartupStepSnapshot;
}) {
  return (
    <div className={cn("startup-progress-step", props.step.status)}>
      {props.step.status === "running" ? <Spinner className="size-4" /> : <StatusIcon status={props.step.status} />}
      <div>
        <strong>{props.step.label}</strong>
        <span>{props.step.message ?? "等待状态更新"}</span>
      </div>
    </div>
  );
}

// activeStartupStep returns the first in-progress or pending startup step for the headline.
// activeStartupStep 返回首个进行中或待处理启动步骤，用于标题显示。
function activeStartupStep(steps: StartupStepSnapshot[]): StartupStepSnapshot | null {
  return steps.find((step) => step.status === "running")
    ?? steps.find((step) => step.status === "pending")
    ?? null;
}

// ErrorScreen renders a startup failure.
// ErrorScreen 渲染启动失败状态。
function ErrorScreen(props: { error: string }) {
  return (
    <main className="loading-screen error">
      <CircleAlert size={28} />
      <span>启动失败：{props.error}</span>
    </main>
  );
}

// WorkbenchLayout renders the GUI-like workbench shell.
// WorkbenchLayout 渲染接近 GUI 的工作台外壳。
function WorkbenchLayout(props: {
  // Latest startup snapshot displayed by the shell blocker.
  // 壳层阻塞面板展示的最近启动快照。
  startup: StartupSnapshot;
  // Latest full workbench snapshot from the backend.
  // 后端返回的最近完整工作台快照。
  workbench: WorkbenchSnapshot;
  // Latest partial workbench update signal from the backend.
  // 后端返回的最近局部工作台更新信号。
  workbenchDelta: WorkbenchDeltaSignal | null;
}) {
  // i18n provides localized shell labels and the active language.
  // i18n 提供本地化外壳标签与当前语言。
  const i18n = useI18n();
  // localizedPrimaryFeatureAreas stores the translated top navigation metadata.
  // localizedPrimaryFeatureAreas 保存已翻译的顶部导航元数据。
  const localizedPrimaryFeatureAreas = React.useMemo(() => buildPrimaryFeatureAreas(i18n.t), [i18n.t]);
  // localizedSettingsFeatureArea stores the translated settings metadata.
  // localizedSettingsFeatureArea 保存已翻译的设置元数据。
  const localizedSettingsFeatureArea = React.useMemo<FeatureAreaSummary>(() => ({
    id: "settings",
    label: i18n.t("area.settings"),
    description: i18n.t("area.settings.description"),
    status: i18n.t("feature.status.preview"),
  }), [i18n]);
  // activeArea stores the currently selected feature area.
  // activeArea 保存当前选中的功能区域。
  const [activeArea, setActiveArea] = React.useState<MainAreaId>("chat");
  // theme stores the current shadcn visual theme.
  // theme 保存当前 shadcn 视觉主题。
  const [theme, setTheme] = React.useState<ThemeMode>(() => initialThemeMode());
  // selectedConversationId stores the conversation selected in the sidebar.
  // selectedConversationId 保存侧栏当前选中的会话。
  const [selectedConversationId, setSelectedConversationId] = React.useState<string | null>(
    props.workbench.selectedConversationId ?? props.workbench.conversations[0]?.id ?? null,
  );
  // selectedConversationIdRef lets async submit completions avoid stealing focus after the user switches sessions.
  // selectedConversationIdRef 让异步提交完成时避免在用户已切换会话后抢回焦点。
  const selectedConversationIdRef = React.useRef<string | null>(selectedConversationId);
  // subagentNavigationRequestSeqRef rejects stale async subagent route responses after a newer click.
  // subagentNavigationRequestSeqRef 在发生更新点击后丢弃过期的异步子代理路由响应。
  const subagentNavigationRequestSeqRef = React.useRef(0);
  // subagentRoutePreferenceRef stores the last user-selected subagent display route for WebUI-only routing.
  // subagentRoutePreferenceRef 保存用户最后选择的子代理显示路由，仅用于 WebUI 显示路由。
  const subagentRoutePreferenceRef = React.useRef<SubagentRoutePreference | null>(null);
  // inflightSubagentOpenRequestRef coalesces duplicate backend open commands for the same route.
  // inflightSubagentOpenRequestRef 合并同一路由的重复后端打开命令。
  const inflightSubagentOpenRequestRef = React.useRef<InflightSubagentOpenRequest | null>(null);
  // draftConversation stores the local-only conversation opened before the first user message is sent.
  // draftConversation 保存发送第一条用户消息前打开的本地草稿会话。
  const [draftConversation, setDraftConversation] = React.useState<DraftConversationSelection | null>(null);
  // draftConversationIdRef keeps the latest draft id without retriggering workbench snapshot hydration.
  // draftConversationIdRef 保存最新草稿标识，避免重新触发工作台快照水合。
  const draftConversationIdRef = React.useRef<string | null>(null);
  // subagentSurfaceMessageCacheRef retains every loaded child transcript surface for instant route switching.
  // subagentSurfaceMessageCacheRef 保留每个已载入的子代理转录界面，用于即时路由切换。
  const subagentSurfaceMessageCacheRef = React.useRef<SubagentSurfaceMessageCache>(new Map());
  // workbenchState stores local message hydration updates.
  // workbenchState 保存本地会话正文水合更新。
  const [workbenchState, setWorkbenchState] = React.useState<WorkbenchSnapshot>(
    () => workbenchWithSubagentSurfaceMessageCache(props.workbench, subagentSurfaceMessageCacheRef.current),
  );
  // workbenchStateRef exposes the latest workbench state to route-click callbacks without stale closures.
  // workbenchStateRef 向路由点击回调暴露最新工作台状态，避免闭包过期。
  const workbenchStateRef = React.useRef<WorkbenchSnapshot>(workbenchState);
  workbenchStateRef.current = workbenchState;
  // mergeWorkbenchWithLocalCaches centralizes service snapshot merges that must preserve loaded child surfaces.
  // mergeWorkbenchWithLocalCaches 集中处理必须保留已载入子代理界面的服务快照合并。
  const mergeWorkbenchWithLocalCaches = React.useCallback((current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => (
    mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache(current, incoming, subagentSurfaceMessageCacheRef.current)
  ), []);
  // mergeConversationMessagesWithLocalCaches centralizes lazy message hydration cache updates.
  // mergeConversationMessagesWithLocalCaches 集中处理懒加载正文时的缓存更新。
  const mergeConversationMessagesWithLocalCaches = React.useCallback((
    current: WorkbenchSnapshot,
    conversationId: string,
    messages: ConversationMessage[],
  ) => mergeConversationMessagesWithSubagentSurfaceMessageCache(
    current,
    conversationId,
    messages,
    subagentSurfaceMessageCacheRef.current,
  ), []);
  // applySkillWorkbenchAction runs one skill-page service action and merges the returned snapshot.
  // applySkillWorkbenchAction 执行单个技能页面服务动作并合并返回快照。
  const applySkillWorkbenchAction = React.useCallback(async (action: () => Promise<WorkbenchSnapshot>) => {
    const snapshot = await action();
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
  }, [mergeWorkbenchWithLocalCaches]);
  // applyAgentSkillsCatalog merges one lightweight Agent Skills catalog update.
  // applyAgentSkillsCatalog 合并单个轻量 Agent Skills 目录更新。
  const applyAgentSkillsCatalog = React.useCallback((agentSkills: AgentSkillsCatalogSummary) => {
    setWorkbenchState((current) => ({
      ...current,
      agentSkills,
    }));
  }, []);
  // loadingConversationId stores the conversation currently loading from the service.
  // loadingConversationId 保存当前正在从服务加载正文的会话标识。
  const [loadingConversationId, setLoadingConversationId] = React.useState<string | null>(null);
  // conversationLoadError stores the latest conversation hydration error.
  // conversationLoadError 保存最近一次会话正文水合错误。
  const [conversationLoadError, setConversationLoadError] = React.useState<string | null>(null);
  // renameTarget stores the project or conversation currently being renamed.
  // renameTarget 保存当前正在重命名的项目或会话目标。
  const [renameTarget, setRenameTarget] = React.useState<RenameTarget | null>(null);
  // projectIconTarget stores the project currently editing its sidebar icon.
  // projectIconTarget 保存当前正在编辑侧栏图标的项目。
  const [projectIconTarget, setProjectIconTarget] = React.useState<ProjectIconDialogTarget | null>(null);
  // projectSettingsTarget stores the project currently shown in the settings dialog.
  // projectSettingsTarget 保存当前显示设置弹窗的项目。
  const [projectSettingsTarget, setProjectSettingsTarget] = React.useState<ProjectSettingsDialogTarget | null>(null);
  // archiveProjectTarget stores the project waiting for archive confirmation.
  // archiveProjectTarget 保存正在等待归档确认的项目。
  const [archiveProjectTarget, setArchiveProjectTarget] = React.useState<ProjectSummary | null>(null);
  // pinnedConversationIds stores the locally pinned conversations until persistence is added.
  // pinnedConversationIds 保存本地置顶的会话，直到后续接入持久化。
  const [pinnedConversationIds, setPinnedConversationIds] = React.useState<Set<string>>(() => new Set());
  // activeSettingsSubArea stores the selected settings sidebar page.
  // activeSettingsSubArea 保存设置侧栏当前选择的页面。
  const [activeSettingsSubArea, setActiveSettingsSubArea] = React.useState<SettingsSubAreaId>("model-providers");
  // hydratedConversationIds stores conversations already requested from the service.
  // hydratedConversationIds 保存已经向服务请求过正文的会话标识。
  const [hydratedConversationIds, setHydratedConversationIds] = React.useState<Set<string>>(
    () => new Set(props.workbench.messages.map((message) => message.conversationId)),
  );
  // activeFeature stores the selected feature summary, including settings opened from the global button.
  // activeFeature 保存当前选中的功能区域摘要，并包含从全局按钮打开的设置页。
  const activeFeature = activeArea === "settings"
    ? localizedSettingsFeatureArea
    : localizedPrimaryFeatureAreas.find((area) => area.id === activeArea) ?? localizedPrimaryFeatureAreas[0];
  // selectedConversation stores the conversation used by the console header.
  // selectedConversation 保存控制台标题栏使用的当前会话。
  const selectedConversation = workbenchState.conversations.find((conversation) => conversation.id === selectedConversationId);
  // selectedConversationMessages stores the current parent conversation message window.
  // selectedConversationMessages 保存当前父会话消息窗口。
  const selectedConversationMessages = React.useMemo(
    () => workbenchState.messages.filter((message) => message.conversationId === selectedConversationId),
    [selectedConversationId, workbenchState.messages],
  );
  // selectedConversationRenderItems stores parsed rows for sidebar/runtime summaries.
  // selectedConversationRenderItems 保存侧栏与运行状态摘要复用的解析行。
  const selectedConversationRenderItems = React.useMemo(
    () => projectConversationRenderItems(selectedConversationMessages),
    [selectedConversationMessages],
  );
  // statusbarRuntimeActivities stores bottom-bar tool and subagent runtime badges for the selected parent conversation.
  // statusbarRuntimeActivities 保存当前选中父会话的底部工具与子代理运行徽标。
  const statusbarRuntimeActivities = React.useMemo(
    () => buildStatusbarRuntimeActivities(
      selectedConversation,
      selectedConversationRenderItems,
      workbenchState.subagentNavigation?.parentConversationId === selectedConversationId
        ? workbenchState.subagentNavigation
        : null,
      i18n.t,
    ),
    [i18n.t, selectedConversation, selectedConversationId, selectedConversationRenderItems, workbenchState.subagentNavigation],
  );
  // selectedDraftConversation stores the draft used by the console header and empty state.
  // selectedDraftConversation 保存控制台标题栏与空状态使用的草稿会话。
  const selectedDraftConversation = selectedConversationId === draftConversation?.id ? draftConversation : null;
  // consoleWorkbenchState clears selected-session projections while a local draft conversation is active.
  // consoleWorkbenchState 在本地草稿会话激活时清空当前会话投影，避免沿用上一会话的侧栏状态。
  const consoleWorkbenchState = React.useMemo<WorkbenchSnapshot>(() => {
    if (!selectedDraftConversation) {
      return workbenchState;
    }
    return {
      ...workbenchState,
      contextInjection: null,
    };
  }, [selectedDraftConversation, workbenchState]);
  // showStartupOverlay blocks workspace interaction until the real service bootstrap completes.
  // showStartupOverlay 在真实服务启动完成前阻塞工作区交互。
  const showStartupOverlay = !props.startup.ready && props.startup.phase !== "browser-preview";

  // Keeps the async selection guard aligned with the latest sidebar choice.
  // 保持异步选中保护与最新侧栏选择一致。
  React.useEffect(() => {
    selectedConversationIdRef.current = selectedConversationId;
  }, [selectedConversationId]);
  // showConsoleSidebar limits project and conversation context to the console view.
  // showConsoleSidebar 将项目与会话上下文限定在控制台页面内。
  const showConsoleSidebar = activeArea === "chat";
  // showWindowControls enables the custom titlebar only inside the Tauri desktop shell.
  // showWindowControls 仅在 Tauri 桌面壳内启用自定义标题栏。
  const showWindowControls = shellClient.supportsWindowControls();
  // isWindowMaximized tracks the current Tauri window maximize state for the titlebar button.
  // isWindowMaximized 跟踪当前 Tauri 窗口最大化状态，用于标题栏按钮。
  const [isWindowMaximized, setIsWindowMaximized] = React.useState(false);
  // refreshWindowMaximizedState synchronizes the local titlebar state with Tauri.
  // refreshWindowMaximizedState 将本地标题栏状态与 Tauri 同步。
  const refreshWindowMaximizedState = React.useCallback(async () => {
    setIsWindowMaximized(await shellClient.isWindowMaximized());
  }, []);
  // toggleWindowMaximize toggles native maximize state and refreshes the icon.
  // toggleWindowMaximize 切换原生最大化状态并刷新图标。
  const toggleWindowMaximize = React.useCallback(async () => {
    await shellClient.toggleWindowMaximize();
    window.setTimeout(() => void refreshWindowMaximizedState(), 40);
  }, [refreshWindowMaximizedState]);
  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }
    void refreshWindowMaximizedState();
  }, [refreshWindowMaximizedState, showWindowControls]);

  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }

    let cancelled = false;
    let unlisten: null | (() => void) = null;

    // subscribeWindowResize keeps the restore icon aligned when native drag-region double click changes state.
    // subscribeWindowResize 在原生拖动区域双击改变窗口状态后同步还原图标。
    async function subscribeWindowResize() {
      if (cancelled) {
        return;
      }
      unlisten = await shellClient.subscribeWindowResize(() => {
        void refreshWindowMaximizedState();
      });
    }

    void subscribeWindowResize();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refreshWindowMaximizedState, showWindowControls]);

  // applyProjectActionSnapshot merges a project-menu operation result into the local workbench.
  // applyProjectActionSnapshot 将项目菜单操作结果合并进本地工作台。
  const applyProjectActionSnapshot = React.useCallback((snapshot: WorkbenchSnapshot | null) => {
    if (!snapshot) {
      return;
    }
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    setDraftConversation(null);
    setSelectedConversationId((current) => {
      if (current && snapshot.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return snapshot.selectedConversationId ?? snapshot.conversations[0]?.id ?? null;
    });
  }, []);

  // handleProjectMenuAction executes immediate project-tree context menu commands.
  // handleProjectMenuAction 执行可立即处理的项目树右键菜单命令。
  const handleProjectMenuAction = React.useCallback(async (action: ProjectMenuAction, project: ProjectSummary) => {
    try {
      if (action === "open-directory") {
        const message = await openProjectDirectory(project.id);
        toast.success(message ?? `已请求打开项目目录：${project.name}`);
        return;
      }

      if (action === "rename") {
        setRenameTarget({ kind: "project", id: project.id, name: project.name });
        return;
      }

      if (action === "set-icon") {
        setProjectIconTarget({ project });
        return;
      }

      if (action === "archive-conversations") {
        setArchiveProjectTarget(project);
        return;
      }

      if (action === "settings") {
        setProjectSettingsTarget({ project });
        return;
      }

      const preview = applyPreviewProjectAction(workbenchState, project.id, action);
      applyProjectActionSnapshot(preview.snapshot);
      await toggleProjectPinned(project.id);
      toast.success(project.pinned ? `已取消置顶项目：${project.name}` : `已置顶项目：${project.name}`);
    } catch (nextError) {
      applyProjectActionSnapshot(workbenchState);
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleProjectSettingsVmmSave persists the project-scoped VMM settings draft.
  // handleProjectSettingsVmmSave 持久化项目级 VMM 设置草稿。
  const handleProjectSettingsVmmSave = React.useCallback(async (projectId: string, draft: VmmConfigDraftSummary) => {
    await applySkillWorkbenchAction(() => saveVmmMemoryConfig({ scope: "project", projectId, draft }));
    toast.success("已保存项目 VMM 设置。");
  }, [applySkillWorkbenchAction]);

  // handleProjectSettingsAgentSkillSave persists one project Agent Skill override.
  // handleProjectSettingsAgentSkillSave 持久化单个项目 Agent Skill 覆盖。
  const handleProjectSettingsAgentSkillSave = React.useCallback(async (projectId: string, name: string, stateKey: ProjectAgentSkillState) => {
    await setAgentSkillProjectState(projectId, name, stateKey);
    toast.success(projectAgentSkillSaveMessage(name, stateKey));
  }, []);

  // handleSetAgentSkillsEnabled persists the global Agent Skills switch with a lightweight catalog refresh.
  // handleSetAgentSkillsEnabled 使用轻量目录刷新持久化 Agent Skills 全局开关。
  const handleSetAgentSkillsEnabled = React.useCallback(async (enabled: boolean) => {
    const previousAgentSkills = workbenchState.agentSkills;
    applyAgentSkillsCatalog({
      ...previousAgentSkills,
      globalEnabled: enabled,
      skills: previousAgentSkills.skills.map((skill) => ({ ...skill, enabled: enabled && skill.enabled })),
    });
    try {
      applyAgentSkillsCatalog(await setAgentSkillsEnabled(enabled));
    } catch (error) {
      applyAgentSkillsCatalog(previousAgentSkills);
      throw error;
    }
  }, [applyAgentSkillsCatalog, workbenchState.agentSkills]);

  // handleSetAgentSkillEnabled persists one Agent Skill state with a lightweight catalog refresh.
  // handleSetAgentSkillEnabled 使用轻量目录刷新持久化单个 Agent Skill 状态。
  const handleSetAgentSkillEnabled = React.useCallback(async (name: string, enabled: boolean) => {
    const previousAgentSkills = workbenchState.agentSkills;
    applyAgentSkillsCatalog({
      ...previousAgentSkills,
      skills: previousAgentSkills.skills.map((skill) => (
        skill.name === name ? { ...skill, enabled: enabled && previousAgentSkills.globalEnabled } : skill
      )),
    });
    try {
      applyAgentSkillsCatalog(await setAgentSkillEnabled(name, enabled));
    } catch (error) {
      applyAgentSkillsCatalog(previousAgentSkills);
      throw error;
    }
  }, [applyAgentSkillsCatalog, workbenchState.agentSkills]);

  // handleProjectIconConfirm persists the selected project icon override.
  // handleProjectIconConfirm 持久化选择的项目图标覆盖。
  const handleProjectIconConfirm = React.useCallback(async (project: ProjectSummary, icon: ProjectIconSelection) => {
    const normalizedIcon = icon.pack === "badge"
      ? { pack: icon.pack, key: encodeProjectBadgeKey(decodeProjectBadgeKey(icon.key) ?? DEFAULT_PROJECT_BADGE_CONFIG) }
      : icon;
    try {
      applyProjectActionSnapshot(applyPreviewProjectIcon(workbenchState, project.id, normalizedIcon).snapshot);
      await setProjectIcon(project.id, normalizedIcon);
      setProjectIconTarget(null);
      toast.success(`已更新项目图标：${project.name}`);
    } catch (error) {
      applyProjectActionSnapshot(workbenchState);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleProjectIconReset clears the selected project icon override.
  // handleProjectIconReset 清除选择的项目图标覆盖。
  const handleProjectIconReset = React.useCallback(async (project: ProjectSummary) => {
    try {
      applyProjectActionSnapshot(applyPreviewProjectIcon(workbenchState, project.id, null).snapshot);
      await resetProjectIcon(project.id);
      setProjectIconTarget(null);
      toast.success(`已还原项目图标：${project.name}`);
    } catch (error) {
      applyProjectActionSnapshot(workbenchState);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleCreateConversation creates and selects a new empty conversation for a project.
  // handleCreateConversation 为项目创建并选中一个新的空白会话。
  const handleCreateConversation = React.useCallback(async (project: ProjectSummary) => {
    const draftId = `draft-${project.id}-${Date.now()}`;
    setDraftConversation({
      id: draftId,
      projectId: project.id,
      projectName: project.name,
    });
    setSelectedConversationId(draftId);
    setConversationLoadError(null);
    setLoadingConversationId(null);
  }, []);

  // handleCreateProject opens the native folder picker and merges the registered project snapshot.
  // handleCreateProject 打开原生文件夹选择器，并合并注册项目后的工作台快照。
  const handleCreateProject = React.useCallback(async () => {
    try {
      const snapshot = await addProjectFromFolder();
      if (!snapshot) {
        toast.info("已取消添加项目。");
        return;
      }
      applyProjectActionSnapshot(snapshot);
      toast.success("项目列表已更新。");
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot]);

  // handleReorderProjects persists a same-section project order using the existing order model.
  // handleReorderProjects 使用现有顺序模型持久化同一分区内的项目排序。
  const handleReorderProjects = React.useCallback(async (projectIds: string[]) => {
    setWorkbenchState((current) => applyPreviewProjectOrder(current, projectIds).snapshot);
    try {
      const snapshot = await reorderProjects(projectIds);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      try {
        const snapshot = await loadWorkbenchSnapshot();
        applyProjectActionSnapshot(snapshot);
      } catch {
        // Keep the optimistic order when the refresh fallback is unavailable.
        // 刷新回退不可用时保留乐观排序结果。
      }
    }
  }, [applyProjectActionSnapshot]);

  // handleExpandedProjectIdsChange silently persists project folder expansion state.
  // handleExpandedProjectIdsChange 静默持久化项目文件夹展开状态。
  const handleExpandedProjectIdsChange = React.useCallback(async (projectIds: string[]) => {
    setWorkbenchState((current) => ({ ...current, expandedProjectIds: projectIds }));
    try {
      const snapshot = await setExpandedTaskProjects(projectIds);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      console.warn("persist expanded projects failed", nextError);
    }
  }, []);

  // handleSubmitConversationPrompt sends the composer text through the shared backend runtime.
  // handleSubmitConversationPrompt 通过共享后端运行时发送输入区文本。
  const handleSubmitConversationPrompt = React.useCallback(async (submission: ComposerSubmission): Promise<boolean> => {
    // submittedSelectionId captures the sidebar choice that owned this submission.
    // submittedSelectionId 捕获发起本次提交时所属的侧栏选择。
    const submittedSelectionId = selectedConversationId;
    const activeProjectId =
      selectedDraftConversation?.projectId
      ?? selectedConversation?.projectId
      ?? workbenchState.projects[0]?.id
      ?? null;
    const activeConversationId = selectedDraftConversation
      ? null
      : selectedConversation?.id ?? null;
    try {
      const snapshot = await submitConversationPrompt(
        activeProjectId,
        activeConversationId,
        submission.prompt,
        submission.attachments,
        submission.executionMode,
        submission.reasoningEffort,
        submission.disabledAiCapabilityKinds,
        submission.disabledAiCapabilityTargetSelectors,
        submission.aiCapabilityTargetOrderOverrides,
      );
      if (!snapshot) {
        toast.info("浏览器预览模式下无法发送真实会话。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      setDraftConversation(null);
      const nextConversationId = snapshot.selectedConversationId ?? activeConversationId;
      if (nextConversationId) {
        setHydratedConversationIds((current) => new Set(current).add(nextConversationId));
        if (selectedConversationIdRef.current === submittedSelectionId) {
          setSelectedConversationId(nextConversationId);
        }
      }
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [selectedConversation, selectedConversationId, selectedDraftConversation, workbenchState.projects]);

  // handleEditPendingPrompt restores one queued prompt back into the active composer.
  // handleEditPendingPrompt 将单条排队提示词恢复到当前输入区。
  const handleEditPendingPrompt = React.useCallback(async (entryId: string): Promise<PendingPromptSummary | null> => {
    try {
      const pendingPrompt = await editPendingPrompt(entryId);
      if (!pendingPrompt) {
        return null;
      }
      const snapshot = await loadWorkbenchSnapshot();
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      return pendingPrompt;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return null;
    }
  }, []);

  // handleInsertPendingPrompt routes one queued prompt through the shared backend queue router.
  // handleInsertPendingPrompt 通过共享后端队列路由引导单条排队提示词。
  const handleInsertPendingPrompt = React.useCallback(async (entryId: string): Promise<void> => {
    webuiClickTrace("pending_prompt.insert.invoke", { entryId });
    try {
      const snapshot = await insertPendingPrompt(entryId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, []);

  // handleStopConversationRun stops the active backend conversation stream.
  // handleStopConversationRun 停止当前活跃的后端会话流。
  const handleStopConversationRun = React.useCallback(async (): Promise<boolean> => {
    webuiClickTrace("conversation.stop.invoke");
    try {
      const snapshot = await stopConversationRun();
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
      return Boolean(snapshot);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, []);

  // handleCompactSelectedConversationContext starts manual compaction from slash commands.
  // handleCompactSelectedConversationContext 从斜杠指令启动手动上下文压缩。
  const handleCompactSelectedConversationContext = React.useCallback(async (): Promise<boolean> => {
    webuiClickTrace("conversation.compact.invoke");
    try {
      const snapshot = await compactSelectedConversationContext();
      if (!snapshot) {
        toast.info("浏览器预览模式下无法执行真实上下文压缩。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, []);

  // handleOpenSubagentPanel asks the backend to display one child task surface and reports whether it succeeded.
  // handleOpenSubagentPanel 请求后端显示单个子任务界面，并返回是否打开成功。
  const handleOpenSubagentPanel = React.useCallback(async (parentConversationId: string, taskId: string): Promise<boolean> => {
    const routeKey = `${parentConversationId}:${taskId}`;
    const existingRequest = inflightSubagentOpenRequestRef.current;
    if (canOpenCachedSubagentSurface(
      workbenchStateRef.current,
      subagentSurfaceMessageCacheRef.current,
      parentConversationId,
      taskId,
    )) {
      subagentRoutePreferenceRef.current = { parentConversationId, taskId };
      webuiClickTrace("subagent.open.local_cache", { parentConversationId, taskId, routeKey });
      setWorkbenchState((current) => openSubagentSurfaceForConversation(
        injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current),
        parentConversationId,
        taskId,
      ));
      return true;
    }
    if (existingRequest?.key === routeKey) {
      subagentRoutePreferenceRef.current = { parentConversationId, taskId };
      webuiClickTrace("subagent.open.coalesced", { parentConversationId, taskId, routeKey });
      setWorkbenchState((current) => openSubagentSurfaceForConversation(
        injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current),
        parentConversationId,
        taskId,
      ));
      return true;
    }
    const requestSeq = subagentNavigationRequestSeqRef.current + 1;
    subagentNavigationRequestSeqRef.current = requestSeq;
    subagentRoutePreferenceRef.current = { parentConversationId, taskId };
    webuiClickTrace("subagent.open.invoke", { parentConversationId, taskId, requestSeq, routeKey });
    setWorkbenchState((current) => openSubagentSurfaceForConversation(
      injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current),
      parentConversationId,
      taskId,
    ));
    const requestPromise = openSubagentPanel(parentConversationId, taskId);
    inflightSubagentOpenRequestRef.current = { key: routeKey, promise: requestPromise };
    try {
      const snapshot = await requestPromise;
      if (inflightSubagentOpenRequestRef.current?.promise === requestPromise) {
        inflightSubagentOpenRequestRef.current = null;
      }
      if (snapshot) {
        if (subagentNavigationRequestSeqRef.current !== requestSeq) {
          webuiClickTrace("subagent.open.stale_result", {
            parentConversationId,
            taskId,
            requestSeq,
            latestRequestSeq: subagentNavigationRequestSeqRef.current,
          });
          return true;
        }
        setWorkbenchState((current) => applySubagentRoutePreference(
          mergeWorkbenchWithLocalCaches(current, snapshot),
          subagentRoutePreferenceRef.current,
        ));
        setHydratedConversationIds((current) => new Set(current).add(snapshot.subagentNavigation?.activeConversationId ?? taskId));
      }
      webuiClickTrace("subagent.open.result", {
        parentConversationId,
        taskId,
        requestSeq,
        opened: Boolean(snapshot),
        activeConversationId: snapshot?.subagentNavigation?.activeConversationId ?? null,
        activeTaskId: snapshot?.subagentNavigation?.activeTaskId ?? null,
      });
      return Boolean(snapshot);
    } catch (nextError) {
      if (inflightSubagentOpenRequestRef.current?.promise === requestPromise) {
        inflightSubagentOpenRequestRef.current = null;
      }
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      webuiClickTrace("subagent.open.error", { parentConversationId, taskId, message });
      if (message.includes("不可恢复") || message.includes("没有找到可打开的子代理任务")) {
        return false;
      }
      toast.error(message);
      return false;
    }
  }, []);

  // handleCloseSubagentPanel asks the backend to return to the parent conversation surface.
  // handleCloseSubagentPanel 请求后端返回父会话界面。
  const handleCloseSubagentPanel = React.useCallback(async (): Promise<void> => {
    const parentConversationId = selectedConversationIdRef.current;
    inflightSubagentOpenRequestRef.current = null;
    const requestSeq = subagentNavigationRequestSeqRef.current + 1;
    subagentNavigationRequestSeqRef.current = requestSeq;
    if (parentConversationId) {
      subagentRoutePreferenceRef.current = { parentConversationId, taskId: null };
      setWorkbenchState((current) => closeSubagentSurfaceForConversation(current, parentConversationId));
    }
    webuiClickTrace("subagent.close.invoke", { parentConversationId, requestSeq });
    try {
      const snapshot = await closeSubagentPanel();
      if (snapshot) {
        if (subagentNavigationRequestSeqRef.current !== requestSeq) {
          webuiClickTrace("subagent.close.stale_result", {
            parentConversationId,
            requestSeq,
            latestRequestSeq: subagentNavigationRequestSeqRef.current,
          });
          return;
        }
        setWorkbenchState((current) => applySubagentRoutePreference(
          mergeWorkbenchWithLocalCaches(current, snapshot),
          subagentRoutePreferenceRef.current,
        ));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, []);

  // handleStopSubagentTask asks the backend to stop one running child task.
  // handleStopSubagentTask 请求后端停止单个运行中的子任务。
  const handleStopSubagentTask = React.useCallback(async (parentConversationId: string, taskId: string): Promise<void> => {
    webuiClickTrace("subagent.stop.invoke", { parentConversationId, taskId });
    try {
      const snapshot = await stopSubagentTask(parentConversationId, taskId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, []);

  // handleRemovePendingPrompt removes one queued prompt through the shared service queue.
  // handleRemovePendingPrompt 通过共享服务队列移除单条排队提示词。
  const handleRemovePendingPrompt = React.useCallback(async (entryId: string): Promise<void> => {
    try {
      const snapshot = await removePendingPrompt(entryId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, []);

  // handleReorderPendingPrompts persists one visible pending prompt order update.
  // handleReorderPendingPrompts 持久化一次可见待发送提示词排序更新。
  const handleReorderPendingPrompts = React.useCallback(async (
    entryIds: string[],
    draggedEntryId: string,
    targetEntryId: string | null,
  ): Promise<void> => {
    setWorkbenchState((current) => ({ ...current, pendingPrompts: reorderPendingPromptSummaries(current.pendingPrompts ?? [], entryIds) }));
    try {
      const snapshot = await reorderPendingPrompts(selectedConversationIdRef.current, draggedEntryId, targetEntryId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      const snapshot = await loadWorkbenchSnapshot();
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
  }, []);

  // handleSelectWorkbenchModel persists one model picker choice through the shared service.
  // handleSelectWorkbenchModel 通过共享服务持久化单个模型筛选器选择。
  const handleSelectWorkbenchModel = React.useCallback(async (choice: ModelPickerChoice): Promise<boolean> => {
    try {
      const snapshot = await selectWorkbenchModel(choice);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
        toast.success(`当前模型已切换为 ${choice.modelName}（${choice.providerName}）。`);
      }
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, []);

  // handleSelectWorkbenchReasoningEffort persists one reasoning-effort choice through the shared service router.
  // handleSelectWorkbenchReasoningEffort 通过共享服务路由持久化单个思考档位选择。
  const handleSelectWorkbenchReasoningEffort = React.useCallback(async (effort: string): Promise<boolean> => {
    try {
      const snapshot = await selectWorkbenchReasoningEffort(effort);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, []);

  // handleConversationMenuAction executes conversation-tree context menu commands.
  // handleConversationMenuAction 执行会话树右键菜单命令。
  const handleConversationMenuAction = React.useCallback(async (action: ConversationMenuAction, conversation: ConversationSummary) => {
    try {
      if (action === "rename") {
        setRenameTarget({ kind: "conversation", id: conversation.id, name: conversation.title });
        return;
      }
      if (action === "pin") {
        setPinnedConversationIds((current) => {
          const next = new Set(current);
          if (next.has(conversation.id)) {
            next.delete(conversation.id);
            toast.success(`已取消置顶对话：${conversation.title}`);
          } else {
            next.add(conversation.id);
            toast.success(`已置顶对话：${conversation.title}`);
          }
          return next;
        });
        return;
      }
      if (action === "archive") {
        const snapshot = await archiveConversation(conversation.id);
        if (snapshot) {
          applyProjectActionSnapshot(snapshot);
        } else {
          applyProjectActionSnapshot(applyPreviewConversationAction(workbenchState, conversation.id, action).snapshot);
        }
        toast.success(`已归档对话：${conversation.title}`);
        return;
      }
      if (action === "open-directory") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error("没有找到这个对话所属的项目。");
          return;
        }
        const message = await openProjectDirectoryPath(project.path);
        toast.success(message ?? `已请求打开项目目录：${project.name}`);
        return;
      }
      if (action === "copy-workdir") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error("没有找到这个对话所属的项目。");
          return;
        }
        await navigator.clipboard.writeText(project.path);
        toast.success("已复制工作目录。");
        return;
      }
      if (action === "copy-session-id") {
        await navigator.clipboard.writeText(conversation.id);
        toast.success("已复制会话 ID。");
        return;
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleSelectConversation selects a conversation and persists its read marker.
  // handleSelectConversation 选择会话并持久化它的已读标记。
  const handleSelectConversation = React.useCallback((conversationId: string) => {
    subagentNavigationRequestSeqRef.current += 1;
    subagentRoutePreferenceRef.current = { parentConversationId: conversationId, taskId: null };
    setSelectedConversationId(conversationId);
    setWorkbenchState((current) => closeSubagentSurfaceForConversation(
      markConversationLocallyRead(current, conversationId),
      conversationId,
    ));
    void selectConversation(conversationId)
      .then((snapshot) => {
        if (snapshot) {
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
        }
        return markConversationRead(conversationId);
      })
      .then((snapshot) => {
        if (snapshot) {
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
        }
      })
      .catch((error) => {
        toast.error(error instanceof Error ? error.message : String(error));
      });
  }, []);

  // handleRenameConfirm commits the current rename dialog value.
  // handleRenameConfirm 提交当前重命名对话框值。
  const handleRenameConfirm = React.useCallback(async (target: RenameTarget, name: string) => {
    const normalizedName = name.trim();
    if (!normalizedName && target.kind !== "project") {
      toast.error("名称不能为空。");
      return;
    }
    const project = target.kind === "project"
      ? workbenchState.projects.find((candidate) => candidate.id === target.id) ?? null
      : null;
    const effectiveProjectName = normalizedName || restoreProjectDisplayName(project, target);
    try {
      const snapshot = target.kind === "project"
        ? await renameProject(target.id, effectiveProjectName)
        : await renameConversation(target.id, normalizedName);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      } else if (target.kind === "project") {
        applyProjectActionSnapshot(applyPreviewProjectRename(workbenchState, target.id, effectiveProjectName).snapshot);
      } else {
        applyProjectActionSnapshot(applyPreviewConversationRename(workbenchState, target.id, normalizedName).snapshot);
      }
      setRenameTarget(null);
      toast.success(target.kind === "project"
        ? normalizedName
          ? `已重命名项目：${effectiveProjectName}`
          : `已还原项目名称：${effectiveProjectName}`
        : `已重命名对话：${normalizedName}`);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleArchiveProjectConfirm archives all conversations under the selected project.
  // handleArchiveProjectConfirm 归档选定项目下的全部对话。
  const handleArchiveProjectConfirm = React.useCallback(async () => {
    if (!archiveProjectTarget) {
      return;
    }
    try {
      const snapshot = await archiveProjectConversations(archiveProjectTarget.id);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      } else {
        applyProjectActionSnapshot(applyPreviewProjectAction(workbenchState, archiveProjectTarget.id, "archive-conversations").snapshot);
      }
      toast.success(`已归档 ${projectConversationCount(workbenchState, archiveProjectTarget.id)} 个对话。`);
      setArchiveProjectTarget(null);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, archiveProjectTarget, workbenchState]);

  // handlePendingQuestionSubmit resumes a blocked ask tool call with validated modal answers.
  // handlePendingQuestionSubmit 使用弹窗校验后的答案恢复被阻塞的 ask 工具调用。
  const handlePendingQuestionSubmit = React.useCallback(async (requestId: string, answers: string[][]) => {
    try {
      const snapshot = await replyPendingQuestion(requestId, answers);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
      toast.success("已提交 ask 回答。");
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [mergeWorkbenchWithLocalCaches]);

  // handlePendingQuestionReject cancels a blocked ask tool call after a second confirmation.
  // handlePendingQuestionReject 在二次确认后取消被阻塞的 ask 工具调用。
  const handlePendingQuestionReject = React.useCallback(async (requestId: string) => {
    try {
      const snapshot = await rejectPendingQuestion(requestId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
      toast.info("已取消 ask 请求。");
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [mergeWorkbenchWithLocalCaches]);

  React.useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    document.documentElement.style.colorScheme = theme;
    window.localStorage.setItem("vulcan-webui-theme", theme);
  }, [theme]);

  React.useEffect(() => {
    draftConversationIdRef.current = draftConversation?.id ?? null;
  }, [draftConversation?.id]);

  React.useEffect(() => {
    setWorkbenchState((current) => applySubagentRoutePreference(
      mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache(
        current,
        props.workbench,
        subagentSurfaceMessageCacheRef.current,
      ),
      subagentRoutePreferenceRef.current,
    ));
    const nextSelected = props.workbench.selectedConversationId ?? props.workbench.conversations[0]?.id ?? null;
    setSelectedConversationId((current) => {
      if (current && current === draftConversationIdRef.current) {
        return current;
      }
      if (current && props.workbench.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return nextSelected;
    });
  }, [props.workbench]);

  React.useEffect(() => {
    if (!props.workbenchDelta) {
      return;
    }
    setWorkbenchState((current) => applySubagentRoutePreference(
      props.workbenchDelta!.envelopes.reduce(
        (nextWorkbench, envelope) => applyWorkbenchDeltaWithSubagentSurfaceMessageCache(
          nextWorkbench,
          envelope,
          subagentSurfaceMessageCacheRef.current,
        ),
        current,
      ),
      subagentRoutePreferenceRef.current,
    ));
    const conversationIds = props.workbenchDelta.envelopes
      .map(workbenchDeltaConversationId)
      .filter((conversationId): conversationId is string => Boolean(conversationId));
    if (conversationIds.length > 0) {
      setHydratedConversationIds((current) => {
        const next = new Set(current);
        for (const conversationId of conversationIds) {
          next.add(conversationId);
        }
        return next;
      });
    }
  }, [props.workbenchDelta]);

  React.useEffect(() => {
    if (!selectedConversationId) {
      return;
    }
    if (selectedConversationId === draftConversation?.id) {
      return;
    }
    const conversationId = selectedConversationId;
    if (hydratedConversationIds.has(conversationId)) {
      return;
    }

    let cancelled = false;
    setLoadingConversationId(conversationId);
    setConversationLoadError(null);

    // hydrateConversation loads the selected conversation without reloading the whole workbench.
    // hydrateConversation 在不重载整个工作台的情况下加载当前选中会话正文。
    async function hydrateConversation() {
      try {
        const messages = await loadConversationMessages(conversationId);
        if (cancelled) {
          return;
        }
        setWorkbenchState((current) => mergeConversationMessagesWithLocalCaches(current, conversationId, messages));
        setHydratedConversationIds((current) => new Set(current).add(conversationId));
      } catch (nextError) {
        if (!cancelled) {
          setConversationLoadError(nextError instanceof Error ? nextError.message : String(nextError));
        }
      } finally {
        if (!cancelled) {
          setLoadingConversationId(null);
        }
      }
    }

    void hydrateConversation();
    return () => {
      cancelled = true;
    };
  }, [draftConversation?.id, hydratedConversationIds, selectedConversationId]);

  // handleComposerSlashCommand routes slash commands into existing workbench actions.
  // handleComposerSlashCommand 将 slash 指令路由到现有工作台动作。
  const handleComposerSlashCommand = React.useCallback((command: ComposerSlashCommandSummary): ComposerSlashCommandResult => {
    switch (normalizedComposerSlashCommandId(command)) {
      case "model":
        return { handled: true, openTarget: "model" };
      case "reasoning":
        return { handled: true, openTarget: "reasoning" };
      case "skill":
        return { handled: true, replacementText: SLASH_SKILL_PROMPT };
      case "skills":
        setActiveArea("skills");
        return { handled: true };
      case "compact":
        void handleCompactSelectedConversationContext();
        return { handled: true };
      default:
        return { handled: false };
    }
  }, [handleCompactSelectedConversationContext]);

  return (
    <div className={`app-shell ${showConsoleSidebar ? "with-console-sidebar" : "without-console-sidebar"} ${showWindowControls ? "tauri-shell" : "browser-shell"}`}>
      <header
        className="main-menu"
        aria-label="主菜单"
      >
        <BrandHeader dragRegion={showWindowControls} />
        <FeatureNav
          areas={localizedPrimaryFeatureAreas}
          activeArea={activeArea}
          onSelect={setActiveArea}
        />
        {showWindowControls ? <div className="titlebar-drag-region" data-tauri-drag-region /> : null}
        <GlobalActions
          activeArea={activeArea}
          startup={props.startup}
          theme={theme}
          locale={i18n.locale}
          onChangeLocale={i18n.setLocale}
          onToggleTheme={() => setTheme((current) => current === "dark" ? "light" : "dark")}
          onOpenSettings={() => setActiveArea("settings")}
        />
        {showWindowControls ? (
          <WindowControls
            isMaximized={isWindowMaximized}
            onMaximizeToggle={toggleWindowMaximize}
            onMaximizeStateRefresh={refreshWindowMaximizedState}
          />
        ) : null}
      </header>
      <main className="workspace">
        {showConsoleSidebar ? (
          <ConsoleShell
            startup={props.startup}
            workbench={consoleWorkbenchState}
            selectedDraftConversation={selectedDraftConversation}
            selectedConversation={selectedConversation}
            selectedConversationId={selectedConversationId}
            loadingConversationId={loadingConversationId}
            conversationLoadError={conversationLoadError}
            pinnedConversationIds={pinnedConversationIds}
            onConversationMenuAction={handleConversationMenuAction}
            onCreateConversation={handleCreateConversation}
            onCreateProject={handleCreateProject}
            onExpandedProjectIdsChange={handleExpandedProjectIdsChange}
            onProjectMenuAction={handleProjectMenuAction}
            onReorderProjects={handleReorderProjects}
            onSelectConversation={handleSelectConversation}
            onSelectWorkbenchModel={handleSelectWorkbenchModel}
            onSelectWorkbenchReasoningEffort={handleSelectWorkbenchReasoningEffort}
            onSlashCommand={handleComposerSlashCommand}
            onSubmitPrompt={handleSubmitConversationPrompt}
            onStopConversationRun={handleStopConversationRun}
            onOpenSubagentPanel={handleOpenSubagentPanel}
            onCloseSubagentPanel={handleCloseSubagentPanel}
            onStopSubagentTask={handleStopSubagentTask}
            onEditPendingPrompt={handleEditPendingPrompt}
            onInsertPendingPrompt={handleInsertPendingPrompt}
            onRemovePendingPrompt={handleRemovePendingPrompt}
            onReorderPendingPrompts={handleReorderPendingPrompts}
          />
        ) : (
          <ExtractedFeatureShell
            activeArea={activeArea}
            activeFeature={activeFeature}
            activeSettingsSubArea={activeSettingsSubArea}
            onSelectSettingsSubArea={setActiveSettingsSubArea}
            skillActions={{
              onRefreshLuaSkills: () => applySkillWorkbenchAction(refreshLuaSkillsCatalog),
              onInstallLuaSkill: (skillId, source, sourceType) => applySkillWorkbenchAction(() => installLuaSkill(skillId, source, sourceType)),
              onUpdateLuaSkill: (skillId) => applySkillWorkbenchAction(() => updateLuaSkill(skillId)),
              onSetLuaSkillEnabled: (skillId, enabled) => applySkillWorkbenchAction(() => setLuaSkillEnabled(skillId, enabled)),
              onUninstallLuaSkill: (skillId) => applySkillWorkbenchAction(() => uninstallLuaSkill(skillId)),
              onSetLuaSkillToolRegistrationState: (skillId, localName, registrationState) => applySkillWorkbenchAction(() => setLuaSkillToolRegistrationState(skillId, localName, registrationState)),
              onRefreshAgentSkills: () => applySkillWorkbenchAction(refreshAgentSkillsCatalog),
              onInstallSystemAgentSkills: () => applySkillWorkbenchAction(installSystemAgentSkills),
              onSetAgentSkillsEnabled: handleSetAgentSkillsEnabled,
              onSetAgentSkillEnabled: handleSetAgentSkillEnabled,
            }}
            memoryActions={{
              onRunVmmAction: runVmmMemoryAction,
              onSaveVmmConfig: (request) => applySkillWorkbenchAction(() => saveVmmMemoryConfig(request)),
            }}
            settingsActions={{
              onSaveProvider: (request) => applySkillWorkbenchAction(async () => (await saveProviderConfiguration(request)) ?? workbenchStateRef.current),
              onRemoveProvider: (providerId) => applySkillWorkbenchAction(async () => (await removeProviderConfiguration(providerId)) ?? workbenchStateRef.current),
              onSaveProviderEnabledModels: (request) => applySkillWorkbenchAction(async () => (await saveProviderEnabledModels(request)) ?? workbenchStateRef.current),
              onSaveProviderModelOverride: (request) => applySkillWorkbenchAction(async () => (await saveProviderModelOverride(request)) ?? workbenchStateRef.current),
              onSaveAiCapabilityVendor: (request) => applySkillWorkbenchAction(async () => (await saveAiCapabilityVendor(request)) ?? workbenchStateRef.current),
              onRemoveAiCapabilityVendor: (vendorId) => applySkillWorkbenchAction(async () => (await removeAiCapabilityVendor(vendorId)) ?? workbenchStateRef.current),
              onSetAiCapabilityTargetEnabled: (request) => applySkillWorkbenchAction(async () => (await setAiCapabilityTargetEnabled(request)) ?? workbenchStateRef.current),
              onSaveAiCapabilityTargets: (request) => applySkillWorkbenchAction(async () => (await saveAiCapabilityTargets(request)) ?? workbenchStateRef.current),
              onSaveMcpServer: (request) => applySkillWorkbenchAction(async () => (await saveMcpServerConfiguration(request as unknown as Record<string, unknown>)) ?? workbenchStateRef.current),
              onRemoveMcpServer: (serverId) => applySkillWorkbenchAction(async () => (await removeMcpServerConfiguration(serverId)) ?? workbenchStateRef.current),
            }}
            startup={props.startup}
            workbench={workbenchState}
          />
        )}
        {showStartupOverlay ? <StartupOverlay startup={props.startup} /> : null}
      </main>
      <footer className="statusbar">
        <span>版本 {props.workbench.appVersion}</span>
        <StatusbarRuntimeActivities activities={statusbarRuntimeActivities} />
        <RepositoryLink />
      </footer>
      <RenameDialog
        target={renameTarget}
        onCancel={() => setRenameTarget(null)}
        onConfirm={handleRenameConfirm}
      />
      <ProjectIconDialog
        target={projectIconTarget}
        onCancel={() => setProjectIconTarget(null)}
        onConfirm={handleProjectIconConfirm}
        onReset={handleProjectIconReset}
      />
      <ProjectSettingsDialog
        target={projectSettingsTarget}
        workbench={workbenchState}
        onCancel={() => setProjectSettingsTarget(null)}
        onLoadAgentSkillOverrides={loadProjectAgentSkillOverrides}
        onLoadVmmDraft={loadProjectVmmConfig}
        onSaveVmmDraft={handleProjectSettingsVmmSave}
        onSetAgentSkillState={handleProjectSettingsAgentSkillSave}
      />
      <ArchiveProjectDialog
        conversationCount={archiveProjectTarget ? projectConversationCount(workbenchState, archiveProjectTarget.id) : 0}
        project={archiveProjectTarget}
        onCancel={() => setArchiveProjectTarget(null)}
        onConfirm={handleArchiveProjectConfirm}
      />
      <AskQuestionDialog
        request={workbenchState.pendingQuestion ?? null}
        onReject={handlePendingQuestionReject}
        onSubmit={handlePendingQuestionSubmit}
      />
    </div>
  );
}

// AskQuestionDialogProps describes callbacks and request data for the ask modal.
// AskQuestionDialogProps 描述 ask 弹窗使用的请求数据与回调。
interface AskQuestionDialogProps {
  // Pending request shown by the dialog.
  // 弹窗展示的待处理请求。
  request: PendingQuestionSnapshot | null;
  // Called when the user confirms abandoning the request.
  // 用户确认放弃请求时调用。
  onReject: (requestId: string) => Promise<void>;
  // Called when the user submits all validated answers.
  // 用户提交全部校验后的答案时调用。
  onSubmit: (requestId: string, answers: string[][]) => Promise<void>;
}

// AskQuestionDraft stores one editable answer draft.
// AskQuestionDraft 保存单个可编辑答案草稿。
interface AskQuestionDraft {
  // Selected structured option labels.
  // 已选择的结构化选项标签。
  selectedLabels: string[];
  // Whether the custom answer lane is enabled.
  // 是否启用自定义回答通道。
  customSelected: boolean;
  // Custom free-text answer.
  // 自定义自由文本回答。
  customText: string;
}

// AskQuestionValidationResult stores collected answers or the first validation error.
// AskQuestionValidationResult 保存收集到的答案或首个校验错误。
interface AskQuestionValidationResult {
  // Answers aligned with the request question order.
  // 与请求问题顺序对齐的答案。
  answers: string[][];
  // First validation error, when present.
  // 存在时表示首个校验错误。
  error: string | null;
  // Question index that owns the first validation error.
  // 首个校验错误所属的问题下标。
  errorIndex: number | null;
}

// AskQuestionDialog renders the blocking ask-tool UI with explicit cancel confirmation.
// AskQuestionDialog 渲染带显式取消确认的阻塞式 ask 工具界面。
function AskQuestionDialog(props: AskQuestionDialogProps) {
  const request = props.request;
  const [activeIndex, setActiveIndex] = React.useState(0);
  const [stage, setStage] = React.useState<"answering" | "confirm">("answering");
  const [drafts, setDrafts] = React.useState<AskQuestionDraft[]>([]);
  const [validationError, setValidationError] = React.useState<string | null>(null);
  const [cancelConfirmOpen, setCancelConfirmOpen] = React.useState(false);
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    setActiveIndex(0);
    setStage("answering");
    setDrafts(request ? buildAskQuestionDrafts(request.questions) : []);
    setValidationError(null);
    setCancelConfirmOpen(false);
    setBusy(false);
  }, [request?.id]);

  const questionCount = request?.questions.length ?? 0;
  const activeQuestion = request?.questions[Math.min(activeIndex, Math.max(questionCount - 1, 0))] ?? null;
  const activeDraft = drafts[Math.min(activeIndex, Math.max(drafts.length - 1, 0))] ?? null;
  const validation = React.useMemo(
    () => request ? collectAskQuestionAnswers(request.questions, drafts) : { answers: [], error: null, errorIndex: null },
    [drafts, request],
  );

  // updateDraft applies one immutable change to a draft row.
  // updateDraft 对单个草稿行应用一次不可变更新。
  const updateDraft = React.useCallback((questionIndex: number, update: (draft: AskQuestionDraft) => AskQuestionDraft) => {
    setDrafts((current) => current.map((draft, index) => index === questionIndex ? update(draft) : draft));
    setValidationError(null);
  }, []);

  // handleSubmit validates answers, then resumes the backend ask runtime.
  // handleSubmit 校验答案并恢复后端 ask 运行时。
  const handleSubmit = React.useCallback(async () => {
    if (!request) {
      return;
    }
    const nextValidation = collectAskQuestionAnswers(request.questions, drafts);
    if (nextValidation.error) {
      setValidationError(nextValidation.error);
      setStage("answering");
      setActiveIndex(nextValidation.errorIndex ?? 0);
      return;
    }
    setBusy(true);
    try {
      await props.onSubmit(request.id, nextValidation.answers);
    } finally {
      setBusy(false);
    }
  }, [drafts, props, request]);

  // handleReject confirms cancellation and rejects the backend ask runtime.
  // handleReject 确认取消并拒绝后端 ask 运行时。
  const handleReject = React.useCallback(async () => {
    if (!request) {
      return;
    }
    setBusy(true);
    try {
      await props.onReject(request.id);
      setCancelConfirmOpen(false);
    } finally {
      setBusy(false);
    }
  }, [props, request]);

  if (!request || questionCount === 0) {
    return null;
  }

  const allAnswered = validation.error === null;
  const isLastQuestion = activeIndex >= questionCount - 1;

  return (
    <>
      <Dialog open>
        <DialogContent
          className="max-h-[min(760px,calc(100vh-2rem))] w-[min(720px,calc(100vw-2rem))] overflow-hidden p-0 sm:max-w-[720px]"
          onEscapeKeyDown={(event) => {
            event.preventDefault();
            setCancelConfirmOpen(true);
          }}
          onInteractOutside={(event) => event.preventDefault()}
          showCloseButton={false}
        >
          <DialogHeader className="border-b px-5 py-4">
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <DialogTitle className="flex items-center gap-2 text-base">
                  <MessageSquareText className="size-4" />
                  ask 需要你的确认
                </DialogTitle>
                <DialogDescription className="mt-1">
                  当前 AI 调用正在等待这些回答，提交后会继续执行。
                </DialogDescription>
              </div>
              <Button
                aria-label="关闭 ask 弹窗"
                disabled={busy}
                onClick={() => setCancelConfirmOpen(true)}
                size="icon-sm"
                type="button"
                variant="ghost"
              >
                <X />
              </Button>
            </div>
          </DialogHeader>
          <div className="grid min-h-0 grid-cols-[180px_minmax(0,1fr)]">
            <aside className="border-r bg-muted/30 p-3">
              <div className="flex flex-col gap-1">
                {request.questions.map((question, questionIndex) => {
                  const draftValidation = collectAskQuestionPromptAnswers(question, drafts[questionIndex] ?? buildAskQuestionDraft(question));
                  const isActive = stage === "answering" && activeIndex === questionIndex;
                  return (
                    <Button
                      className={cn(
                        "h-auto justify-start gap-2 px-2 py-2 text-left",
                        isActive ? "bg-background shadow-xs" : "bg-transparent",
                      )}
                      key={`${request.id}-${question.header}-${questionIndex}`}
                      onClick={() => {
                        setStage("answering");
                        setActiveIndex(questionIndex);
                      }}
                      type="button"
                      variant="ghost"
                    >
                      <span className={cn("size-2 rounded-full", draftValidation.error ? "bg-muted-foreground/35" : "bg-primary")} />
                      <span className="min-w-0 truncate">{question.header || `问题 ${questionIndex + 1}`}</span>
                    </Button>
                  );
                })}
                <Button
                  className={cn("mt-2 h-auto justify-start gap-2 px-2 py-2 text-left", stage === "confirm" ? "bg-background shadow-xs" : "bg-transparent")}
                  onClick={() => setStage("confirm")}
                  type="button"
                  variant="ghost"
                >
                  <CheckCircle2 className={cn("size-4", allAnswered ? "text-primary" : "text-muted-foreground")} />
                  <span>确认提交</span>
                </Button>
              </div>
            </aside>
            <section className="min-h-[420px] overflow-y-auto p-5">
              {stage === "answering" && activeQuestion && activeDraft ? (
                <AskQuestionEditor
                  draft={activeDraft}
                  error={validationError}
                  onChange={(nextDraft) => updateDraft(activeIndex, () => nextDraft)}
                  question={activeQuestion}
                  questionIndex={activeIndex}
                  questionTotal={questionCount}
                />
              ) : (
                <AskQuestionConfirm
                  answers={validation.answers}
                  errorIndex={validation.errorIndex}
                  questions={request.questions}
                  onSelectQuestion={(questionIndex) => {
                    setStage("answering");
                    setActiveIndex(questionIndex);
                  }}
                />
              )}
            </section>
          </div>
          <DialogFooter className="rounded-none">
            <Button
              disabled={busy}
              onClick={() => setCancelConfirmOpen(true)}
              type="button"
              variant="outline"
            >
              取消
            </Button>
            {stage === "answering" ? (
              <Button
                disabled={busy}
                onClick={() => {
                  if (isLastQuestion) {
                    setStage("confirm");
                  } else {
                    setActiveIndex((current) => Math.min(current + 1, questionCount - 1));
                  }
                }}
                type="button"
              >
                {isLastQuestion ? "进入确认" : "下一题"}
              </Button>
            ) : (
              <Button disabled={busy || !allAnswered} onClick={handleSubmit} type="button">
                {busy ? "提交中..." : "提交回答"}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={cancelConfirmOpen} onOpenChange={setCancelConfirmOpen}>
        <DialogContent
          className="w-[min(420px,calc(100vw-2rem))] sm:max-w-[420px]"
          onEscapeKeyDown={(event) => {
            event.preventDefault();
            setCancelConfirmOpen(false);
          }}
          onInteractOutside={(event) => event.preventDefault()}
          showCloseButton={false}
        >
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <CircleAlert className="size-4 text-destructive" />
              确认取消 ask？
            </DialogTitle>
            <DialogDescription>
              取消后当前 ask 工具调用会以失败结束，本轮 AI 执行会被中断。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button disabled={busy} onClick={() => setCancelConfirmOpen(false)} type="button" variant="outline">
              继续回答
            </Button>
            <Button disabled={busy} onClick={handleReject} type="button" variant="destructive">
              {busy ? "取消中..." : "确认取消"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

// AskQuestionEditor renders one editable ask prompt page.
// AskQuestionEditor 渲染单个可编辑 ask 问题页面。
function AskQuestionEditor(props: {
  // Question currently being edited.
  // 当前正在编辑的问题。
  question: PendingQuestionPromptSummary;
  // Draft belonging to the current question.
  // 当前问题对应的草稿。
  draft: AskQuestionDraft;
  // Zero-based question index.
  // 从零开始的问题下标。
  questionIndex: number;
  // Total number of questions in the request.
  // 当前请求中的问题总数。
  questionTotal: number;
  // Current validation error shown by the parent modal.
  // 父级弹窗传入的当前校验错误。
  error: string | null;
  // Emits the edited draft.
  // 发出编辑后的草稿。
  onChange: (draft: AskQuestionDraft) => void;
}) {
  const hasOptions = props.question.options.length > 0;
  const customPlaceholder = hasOptions ? "填写其他回答..." : "输入你的回答...";
  const toggleOption = (option: PendingQuestionOptionSummary) => {
    if (props.question.multiple) {
      const selected = props.draft.selectedLabels.includes(option.label)
        ? props.draft.selectedLabels.filter((label) => label !== option.label)
        : [...props.draft.selectedLabels, option.label];
      props.onChange({ ...props.draft, selectedLabels: selected });
      return;
    }
    props.onChange({ ...props.draft, customSelected: false, selectedLabels: [option.label] });
  };

  return (
    <div className="flex min-h-full flex-col gap-5">
      <div>
        <div className="text-xs font-medium text-muted-foreground">
          第 {props.questionIndex + 1}/{props.questionTotal} 题
        </div>
        <h2 className="mt-2 text-lg font-semibold">{props.question.header}</h2>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">{props.question.question}</p>
      </div>
      {hasOptions ? (
        <div className="grid gap-2">
          {props.question.options.map((option) => {
            const selected = props.draft.selectedLabels.includes(option.label);
            return (
              <button
                className={cn(
                  "rounded-lg border p-3 text-left transition-colors",
                  selected ? "border-primary bg-primary/5" : "border-border hover:bg-muted/60",
                )}
                key={option.label}
                onClick={() => toggleOption(option)}
                type="button"
              >
                <span className="flex items-start gap-3">
                  <span className={cn("mt-1 size-3 rounded-full border", selected ? "border-primary bg-primary" : "border-muted-foreground/40")} />
                  <span className="min-w-0">
                    <span className="block text-sm font-medium">{option.label}</span>
                    <span className="mt-1 block text-xs leading-5 text-muted-foreground">{option.description}</span>
                  </span>
                </span>
              </button>
            );
          })}
          <button
            className={cn(
              "rounded-lg border p-3 text-left transition-colors",
              props.draft.customSelected ? "border-primary bg-primary/5" : "border-border hover:bg-muted/60",
            )}
            onClick={() => props.onChange({ ...props.draft, customSelected: !props.draft.customSelected })}
            type="button"
          >
            <span className="text-sm font-medium">其他回答</span>
            <span className="mt-1 block text-xs text-muted-foreground">使用自定义文本补充或替代上述选项。</span>
          </button>
        </div>
      ) : null}
      {(!hasOptions || props.draft.customSelected) ? (
        <Textarea
          autoFocus
          className="min-h-28 resize-none"
          onChange={(event) => props.onChange({
            ...props.draft,
            customSelected: true,
            customText: event.target.value,
          })}
          placeholder={customPlaceholder}
          value={props.draft.customText}
        />
      ) : null}
      {props.error ? (
        <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {props.error}
        </div>
      ) : null}
    </div>
  );
}

// AskQuestionConfirm renders the final answer review page.
// AskQuestionConfirm 渲染最终答案确认页面。
function AskQuestionConfirm(props: {
  // Original questions.
  // 原始问题集合。
  questions: PendingQuestionPromptSummary[];
  // Collected answers aligned with the original questions.
  // 与原始问题对齐的已收集答案。
  answers: string[][];
  // First invalid question index.
  // 首个无效问题下标。
  errorIndex: number | null;
  // Opens one question for editing.
  // 打开单个问题进行编辑。
  onSelectQuestion: (questionIndex: number) => void;
}) {
  return (
    <div className="flex min-h-full flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold">确认回答</h2>
        <p className="mt-2 text-sm text-muted-foreground">
          检查无误后提交，AI 会继续当前工具调用。
        </p>
      </div>
      <div className="grid gap-2">
        {props.questions.map((question, questionIndex) => {
          const answerText = props.answers[questionIndex]?.join("、") ?? "";
          const hasError = props.errorIndex === questionIndex || !answerText;
          return (
            <button
              className={cn(
                "rounded-lg border p-3 text-left transition-colors hover:bg-muted/60",
                hasError ? "border-destructive/40" : "border-border",
              )}
              key={`${question.header}-${questionIndex}`}
              onClick={() => props.onSelectQuestion(questionIndex)}
              type="button"
            >
              <span className="flex items-start justify-between gap-3">
                <span className="min-w-0">
                  <span className="block text-sm font-medium">{question.header}</span>
                  <span className="mt-1 block text-xs leading-5 text-muted-foreground">{question.question}</span>
                  <span className={cn("mt-2 block text-sm", answerText ? "text-foreground" : "text-destructive")}>
                    {answerText || "尚未填写"}
                  </span>
                </span>
                {hasError ? <CircleAlert className="size-4 shrink-0 text-destructive" /> : <CheckCircle2 className="size-4 shrink-0 text-primary" />}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

// buildAskQuestionDrafts creates fresh drafts for a pending ask request.
// buildAskQuestionDrafts 为待处理 ask 请求创建全新草稿。
function buildAskQuestionDrafts(questions: PendingQuestionPromptSummary[]): AskQuestionDraft[] {
  return questions.map(buildAskQuestionDraft);
}

// buildAskQuestionDraft creates one default draft for a prompt.
// buildAskQuestionDraft 为单条问题提示创建默认草稿。
function buildAskQuestionDraft(question: PendingQuestionPromptSummary): AskQuestionDraft {
  return {
    selectedLabels: [],
    customSelected: question.options.length === 0,
    customText: "",
  };
}

// collectAskQuestionAnswers validates every prompt and returns ordered answers.
// collectAskQuestionAnswers 校验全部问题并返回有序答案。
function collectAskQuestionAnswers(
  questions: PendingQuestionPromptSummary[],
  drafts: AskQuestionDraft[],
): AskQuestionValidationResult {
  const answers: string[][] = [];
  for (let index = 0; index < questions.length; index += 1) {
    const question = questions[index];
    const draft = drafts[index] ?? buildAskQuestionDraft(question);
    const result = collectAskQuestionPromptAnswers(question, draft);
    if (result.error) {
      return { answers, error: result.error, errorIndex: index };
    }
    answers.push(result.answers);
  }
  return { answers, error: null, errorIndex: null };
}

// collectAskQuestionPromptAnswers validates one prompt draft.
// collectAskQuestionPromptAnswers 校验单条问题草稿。
function collectAskQuestionPromptAnswers(
  question: PendingQuestionPromptSummary,
  draft: AskQuestionDraft,
): { answers: string[]; error: string | null } {
  const trimmedCustom = draft.customText.trim();
  if (question.options.length === 0) {
    return trimmedCustom
      ? { answers: [trimmedCustom], error: null }
      : { answers: [], error: `请先填写“${question.header}”的回答。` };
  }
  const answers = [...draft.selectedLabels];
  if (draft.customSelected) {
    if (!trimmedCustom) {
      return { answers: [], error: `请先填写“${question.header}”的其他回答。` };
    }
    answers.push(trimmedCustom);
  }
  if (answers.length === 0) {
    return { answers: [], error: `请先选择“${question.header}”的答案。` };
  }
  return { answers, error: null };
}

// StatusbarRuntimeActivities renders centered tool/subagent activities in the footer.
// StatusbarRuntimeActivities 在底部栏居中渲染工具与子代理活动。
function StatusbarRuntimeActivities(props: {
  // Activity badges currently visible for the selected conversation.
  // 当前选中会话可见的活动徽标。
  activities: StatusbarRuntimeActivity[];
}) {
  return (
    <div className="statusbar-runtime-activities" aria-live="polite">
      {props.activities.map((activity) => {
        const Icon = activity.kind === "subagent" ? Bot : Wrench;
        return (
          <span className={cn("statusbar-runtime-activity", activity.kind)} key={activity.kind}>
            <Icon size={13} />
            <ShinyText text={activity.text} speed={1.65} />
          </span>
        );
      })}
    </div>
  );
}

// initialThemeMode returns the persisted shadcn theme, defaulting to dark for continuity.
// initialThemeMode 返回持久化的 shadcn 主题，默认使用暗色以保持连续性。
function initialThemeMode(): ThemeMode {
  const stored = window.localStorage.getItem("vulcan-webui-theme");
  return stored === "light" || stored === "dark" ? stored : "dark";
}

// initialThinkingOverlayEnabled returns the persisted thinking display preference.
// initialThinkingOverlayEnabled 返回持久化的思考展示偏好。
function initialThinkingOverlayEnabled(): boolean {
  return window.localStorage.getItem(THINKING_OVERLAY_STORAGE_KEY) !== "false";
}

// WindowControls renders Tauri-only custom minimize, maximize, and close buttons.
// WindowControls 渲染仅 Tauri 使用的自定义最小化、最大化与关闭按钮。
function WindowControls(props: {
  // Whether the current Tauri window is maximized.
  // 当前 Tauri 窗口是否处于最大化状态。
  isMaximized: boolean;
  // Toggles maximize or restore using the shared titlebar behavior.
  // 使用共享标题栏行为切换最大化或还原。
  onMaximizeToggle: () => Promise<void>;
  // Refreshes maximize state after native window actions.
  // 在原生窗口操作后刷新最大化状态。
  onMaximizeStateRefresh: () => Promise<void>;
}) {
  // runWindowAction invokes one native window command and reports failures as toasts.
  // runWindowAction 调用单个原生窗口命令，并用 toast 报告失败。
  const runWindowAction = React.useCallback(async (action: "minimize" | "close") => {
    try {
      if (action === "minimize") {
        await shellClient.minimizeWindow();
      } else {
        await shellClient.closeWindow();
      }
      await props.onMaximizeStateRefresh();
    } catch (error) {
      toast.error(`窗口操作失败：${error instanceof Error ? error.message : String(error)}`);
    }
  }, [props]);

  // handleMaximizeClick shares the same maximize/restore behavior as titlebar double-click.
  // handleMaximizeClick 与标题栏双击共享同一套最大化/还原行为。
  const handleMaximizeClick = React.useCallback(async () => {
    try {
      await props.onMaximizeToggle();
    } catch (error) {
      toast.error(`窗口操作失败：${error instanceof Error ? error.message : String(error)}`);
    }
  }, [props]);

  return (
    <div className="window-controls" data-no-drag>
      <Button
        aria-label="最小化窗口"
        className="window-control-button"
        onClick={() => void runWindowAction("minimize")}
        size="icon"
        type="button"
        variant="ghost"
      >
        <MinusIcon size={16} />
      </Button>
      <Button
        aria-label={props.isMaximized ? "还原窗口" : "最大化窗口"}
        className="window-control-button"
        onClick={() => void handleMaximizeClick()}
        size="icon"
        type="button"
        variant="ghost"
      >
        {props.isMaximized ? <Minimize2 size={15} /> : <SquareIcon size={13} />}
      </Button>
      <Button
        aria-label="关闭窗口"
        className="window-control-button close"
        onClick={() => void runWindowAction("close")}
        size="icon"
        type="button"
        variant="ghost"
      >
        <X size={16} />
      </Button>
    </div>
  );
}

// RepositoryLink renders the statusbar GitHub repository link.
// RepositoryLink 渲染状态栏 GitHub 仓库链接。
function RepositoryLink() {
  // handleClick opens the repository externally in Tauri and in a new tab in browsers.
  // handleClick 在 Tauri 中外部打开仓库，在浏览器中打开新标签。
  const handleClick = React.useCallback((event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    void openExternalUrl(GITHUB_REPOSITORY_URL).catch((error) => {
      toast.error(`打开 GitHub 失败：${error instanceof Error ? error.message : String(error)}`);
    });
  }, []);

  return (
    <a
      aria-label="打开 OpenVulcan/vulcan-code GitHub 仓库"
      className="repository-link"
      href={GITHUB_REPOSITORY_URL}
      onClick={handleClick}
      rel="noreferrer"
      target="_blank"
    >
      <GitHubMark />
      <span>OpenVulcan/vulcan-code</span>
    </a>
  );
}

// GitHubMark renders the filled GitHub logo mark used in the statusbar.
// GitHubMark 渲染状态栏使用的填充版 GitHub 标识。
function GitHubMark() {
  return (
    <svg
      aria-hidden="true"
      className="repository-github-mark"
      focusable="false"
      viewBox="0 0 16 16"
    >
      <path
        fill="currentColor"
        d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82A7.65 7.65 0 0 1 8 3.87c.68 0 1.36.09 2 .26 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
      />
    </svg>
  );
}

// SubagentRoutePreference describes the WebUI-only visible route selected by the user.
// SubagentRoutePreference 描述用户选择的仅 WebUI 可见子代理路由。
interface SubagentRoutePreference {
  // Parent conversation that owns the displayed subagent navigation.
  // 拥有所显示子代理导航的父会话。
  parentConversationId: string;
  // Child task id to display, or null when the parent conversation is displayed.
  // 要显示的子任务标识；为 null 时显示父会话。
  taskId: string | null;
}

// InflightSubagentOpenRequest tracks one backend route command that duplicate clicks can share.
// InflightSubagentOpenRequest 跟踪一条可被重复点击复用的后端路由命令。
interface InflightSubagentOpenRequest {
  // Stable parent/task route key for request coalescing.
  // 用于请求合并的稳定父级/任务路由键。
  key: string;
  // Backend command promise for the route change.
  // 路由变更对应的后端命令 Promise。
  promise: Promise<WorkbenchSnapshot | null>;
}

// SubagentSurfaceMessageCache stores loaded child transcript messages by child task conversation id.
// SubagentSurfaceMessageCache 按子任务会话标识保存已载入的子代理转录消息。
type SubagentSurfaceMessageCache = Map<string, ConversationMessage[]>;

// ConversationScrollSnapshot stores one transcript surface scroll restoration point.
// ConversationScrollSnapshot 保存单个转录界面的滚动恢复点。
interface ConversationScrollSnapshot {
  // Distance from the visual viewport bottom to the transcript bottom.
  // 从视觉视口底部到转录底部的距离。
  distanceFromBottom: number;
  // Whether this surface should continue following newly appended content.
  // 该界面是否应继续跟随新追加的内容。
  wasAtBottom: boolean;
}

// conversationDistanceFromBottom returns the scroll metric used by the sticky-footer transcript.
// conversationDistanceFromBottom 返回粘性底部转录布局使用的滚动距离指标。
function conversationDistanceFromBottom(element: HTMLElement): number {
  return Math.max(0, element.scrollHeight - element.scrollTop - element.clientHeight);
}

// conversationScrollToDistanceFromBottom restores a visual bottom distance in the current layout.
// conversationScrollToDistanceFromBottom 在当前布局中恢复视觉底部距离。
function conversationScrollToDistanceFromBottom(element: HTMLElement, distanceFromBottom: number): void {
  const maxScrollTop = Math.max(0, element.scrollHeight - element.clientHeight);
  element.scrollTop = Math.max(0, Math.min(maxScrollTop, element.scrollHeight - element.clientHeight - distanceFromBottom));
}

// mergeWorkbenchSnapshot merges backend workbench updates without discarding lazily loaded messages.
// mergeWorkbenchSnapshot 合并后端工作台更新且不丢弃已懒加载的会话正文。
function mergeWorkbenchSnapshot(current: WorkbenchSnapshot, incoming: WorkbenchSnapshot): WorkbenchSnapshot {
  const incomingConversationIds = new Set(incoming.messages.map((message) => message.conversationId));
  const currentMessagesByConversation = groupMessagesByConversation(current.messages);
  const incomingMessagesByConversation = groupMessagesByConversation(incoming.messages);
  const incomingConversationsById = new Map(incoming.conversations.map((conversation) => [conversation.id, conversation]));
  const currentConversationsById = new Map(current.conversations.map((conversation) => [conversation.id, conversation]));
  const mergedIncomingMessages = [...incomingMessagesByConversation.entries()].flatMap(([conversationId, messages]) => (
    mergeIncomingConversationMessages(
      currentMessagesByConversation.get(conversationId) ?? [],
      messages,
      currentConversationsById.get(conversationId),
      incomingConversationsById.get(conversationId),
    )
  ));
  const incomingMessageIds = new Set(mergedIncomingMessages.map((message) => message.id));
  const retainedMessages = current.messages.filter((message) => (
    !incomingConversationIds.has(message.conversationId) && !incomingMessageIds.has(message.id)
  ));
  const selectedConversationId = current.selectedConversationId;
  return {
    ...incoming,
    selectedConversationId,
    conversations: mergeConversationSummaries(current.conversations, incoming.conversations, selectedConversationId),
    messages: [...retainedMessages, ...mergedIncomingMessages],
  };
}

// workbenchDeltaConversationId returns the conversation touched by one partial update.
// workbenchDeltaConversationId 返回单条局部更新影响的会话标识。
function workbenchDeltaConversationId(envelope: WorkbenchDeltaEnvelope): string | null {
  switch (envelope.delta.kind) {
    case "appendMessageBody":
      return envelope.delta.payload.conversationId;
  }
}

// coalesceWorkbenchDeltaEnvelopes merges adjacent append-only chunks for the same message.
// coalesceWorkbenchDeltaEnvelopes 合并同一消息的相邻追加式片段。
function coalesceWorkbenchDeltaEnvelopes(envelopes: WorkbenchDeltaEnvelope[]): WorkbenchDeltaEnvelope[] {
  const coalesced: WorkbenchDeltaEnvelope[] = [];
  for (const envelope of envelopes) {
    const previous = coalesced.at(-1);
    if (previous && canCoalesceAppendMessageBodyDelta(previous, envelope)) {
      const previousPayload = previous.delta.payload;
      const nextPayload = envelope.delta.payload;
      previous.generation = Math.max(previous.generation, envelope.generation);
      previousPayload.bodyDelta += nextPayload.bodyDelta;
      previousPayload.currentBody = nextPayload.currentBody ?? previousPayload.currentBody ?? null;
      if (nextPayload.initialMessage && (!previousPayload.initialMessage || nextPayload.initialMessage.body.length > previousPayload.initialMessage.body.length)) {
        previousPayload.initialMessage = { ...nextPayload.initialMessage };
      } else if (previousPayload.initialMessage) {
        previousPayload.initialMessage = {
          ...previousPayload.initialMessage,
          body: previousPayload.currentBody ?? appendStreamingTextDelta(previousPayload.initialMessage.body, nextPayload.bodyDelta),
        };
      }
      previousPayload.runPhase = nextPayload.runPhase ?? previousPayload.runPhase;
      continue;
    }
    coalesced.push(cloneWorkbenchDeltaEnvelope(envelope));
  }
  return coalesced;
}

// canCoalesceAppendMessageBodyDelta checks whether two envelopes target the same append-only row.
// canCoalesceAppendMessageBodyDelta 检查两个信封是否指向同一条追加式消息行。
function canCoalesceAppendMessageBodyDelta(previous: WorkbenchDeltaEnvelope, next: WorkbenchDeltaEnvelope): boolean {
  if (previous.delta.kind !== "appendMessageBody" || next.delta.kind !== "appendMessageBody") {
    return false;
  }
  return previous.delta.payload.conversationId === next.delta.payload.conversationId
    && previous.delta.payload.messageId === next.delta.payload.messageId;
}

// cloneWorkbenchDeltaEnvelope clones mutable delta envelopes before frame-level coalescing edits them.
// cloneWorkbenchDeltaEnvelope 在帧级合并修改信封前克隆可变增量数据。
function cloneWorkbenchDeltaEnvelope(envelope: WorkbenchDeltaEnvelope): WorkbenchDeltaEnvelope {
  if (envelope.delta.kind === "appendMessageBody") {
    return {
      generation: envelope.generation,
      delta: {
        kind: "appendMessageBody",
        payload: {
          ...envelope.delta.payload,
          initialMessage: envelope.delta.payload.initialMessage
            ? { ...envelope.delta.payload.initialMessage }
            : envelope.delta.payload.initialMessage,
        },
      },
    };
  }
  return envelope;
}

// applyWorkbenchDeltaWithSubagentSurfaceMessageCache applies one partial update while keeping child transcript caches coherent.
// applyWorkbenchDeltaWithSubagentSurfaceMessageCache 应用单条局部更新，并保持子代理转录缓存一致。
function applyWorkbenchDeltaWithSubagentSurfaceMessageCache(
  workbench: WorkbenchSnapshot,
  envelope: WorkbenchDeltaEnvelope,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  switch (envelope.delta.kind) {
    case "appendMessageBody": {
      const nextWorkbench = applyAppendMessageBodyDelta(workbench, envelope.delta.payload);
      syncSubagentSurfaceMessageCacheForDelta(cache, nextWorkbench, envelope.delta.payload.conversationId);
      return injectCachedSubagentSurfaceMessages(nextWorkbench, cache);
    }
  }
}

// applyAppendMessageBodyDelta appends streamed text to one local workbench message window.
// applyAppendMessageBodyDelta 将流式文本追加到本地工作台的单个消息窗口。
function applyAppendMessageBodyDelta(
  workbench: WorkbenchSnapshot,
  delta: WorkbenchDeltaEnvelope["delta"]["payload"],
): WorkbenchSnapshot {
  const messages = appendMessageBodyDeltaToMessages(workbench.messages, delta);
  const runPhase = delta.runPhase ?? undefined;
  return {
    ...workbench,
    conversations: workbench.conversations.map((conversation) => {
      if (conversation.id !== delta.conversationId) {
        return conversation;
      }
      return {
        ...conversation,
        runPhase: runPhase ?? conversation.runPhase,
        status: runPhase && !["idle", "ready", "failed", "stopped"].includes(normalizedSessionRunPhase(runPhase) ?? "")
          ? "Running"
          : conversation.status,
        lastUpdatedLabel: "刚刚",
      };
    }),
    subagentNavigation: updateSubagentNavigationRunPhase(workbench.subagentNavigation, delta.conversationId, runPhase),
    messages,
  };
}

// appendMessageBodyDeltaToMessages updates one message array without cloning unrelated message objects.
// appendMessageBodyDeltaToMessages 更新单个消息数组，并避免克隆无关消息对象。
function appendMessageBodyDeltaToMessages(
  messages: ConversationMessage[],
  delta: WorkbenchDeltaEnvelope["delta"]["payload"],
): ConversationMessage[] {
  let found = false;
  const nextMessages = messages.map((message) => {
    if (message.id !== delta.messageId) {
      return message;
    }
    found = true;
    return mergeAppendMessageBodyDelta(message, delta);
  });
  if (found || !delta.initialMessage) {
    return nextMessages;
  }

  const appendedInitialMessage = appendInitialMessageBodyDelta(delta.initialMessage, delta.bodyDelta);
  const replacementIndex = findAppendMessageBodyReplacementIndex(nextMessages, delta, appendedInitialMessage);
  if (replacementIndex >= 0) {
    return nextMessages.map((message, index) => (index === replacementIndex ? appendedInitialMessage : message));
  }
  return [...nextMessages, appendedInitialMessage];
}

// mergeAppendMessageBodyDelta applies a streamed append over an existing row or a fuller initial snapshot.
// mergeAppendMessageBodyDelta 将流式追加应用到已有行或更完整的初始快照上。
function mergeAppendMessageBodyDelta(
  message: ConversationMessage,
  delta: WorkbenchDeltaEnvelope["delta"]["payload"],
): ConversationMessage {
  if (typeof delta.currentBody === "string") {
    return {
      ...(delta.initialMessage && delta.initialMessage.body.length >= message.body.length ? delta.initialMessage : message),
      body: delta.currentBody,
    };
  }
  if (delta.initialMessage && delta.initialMessage.body.length > message.body.length) {
    return delta.initialMessage;
  }
  return {
    ...message,
    body: appendStreamingTextDelta(message.body, delta.bodyDelta),
  };
}

// appendInitialMessageBodyDelta returns the host-projected full row for a newly materialized stream.
// appendInitialMessageBodyDelta 返回宿主为新流式记录投影出的完整行。
function appendInitialMessageBodyDelta(message: ConversationMessage, bodyDelta: string): ConversationMessage {
  void bodyDelta;
  return {
    ...message,
    body: message.body,
  };
}

// findAppendMessageBodyReplacementIndex finds a live row that already represents the same streamed assistant output.
// findAppendMessageBodyReplacementIndex 查找已经代表同一条流式助手输出的实时行。
function findAppendMessageBodyReplacementIndex(
  messages: ConversationMessage[],
  delta: WorkbenchDeltaEnvelope["delta"]["payload"],
  candidate: ConversationMessage,
): number {
  if (!isAssistantOutputMessage(candidate)) {
    return -1;
  }
  let lastUserIndex = -1;
  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index];
    if (message?.conversationId === delta.conversationId && message.role === "user") {
      lastUserIndex = index;
    }
  }
  const stableIndex = findStableAppendMessageBodyReplacementIndex(messages, delta, candidate, lastUserIndex);
  if (stableIndex >= 0) {
    return stableIndex;
  }
  for (let index = messages.length - 1; index > lastUserIndex; index -= 1) {
    const message = messages[index];
    if (
      message?.conversationId === delta.conversationId
      && message.id !== candidate.id
      && isAssistantOutputMessage(message)
      && assistantOutputsLookLikeSameStream(candidate, message)
    ) {
      return index;
    }
  }
  return -1;
}

// findStableAppendMessageBodyReplacementIndex prefers durable stream identifiers over body heuristics.
// findStableAppendMessageBodyReplacementIndex 优先使用持久流标识，而不是正文启发式。
function findStableAppendMessageBodyReplacementIndex(
  messages: ConversationMessage[],
  delta: WorkbenchDeltaEnvelope["delta"]["payload"],
  candidate: ConversationMessage,
  lastUserIndex: number,
): number {
  for (let index = messages.length - 1; index > lastUserIndex; index -= 1) {
    const message = messages[index];
    if (
      message?.conversationId === delta.conversationId
      && message.id !== candidate.id
      && isAssistantOutputMessage(message)
      && messagesShareStableStreamIdentity(candidate, message)
    ) {
      return index;
    }
  }
  return -1;
}

// messagesShareStableStreamIdentity checks metadata keys that identify the same streamed assistant row.
// messagesShareStableStreamIdentity 检查可识别同一条助手流式行的元数据键。
function messagesShareStableStreamIdentity(left: ConversationMessage, right: ConversationMessage): boolean {
  if (left.conversationId !== right.conversationId) {
    return false;
  }
  const sharedMetadataPaths = [
    "webui_projection_barrier_id",
    "webuiProjectionBarrierId",
    "stream_id",
    "streamId",
    "response_id",
    "responseId",
    "provider_response_id",
    "providerResponseId",
    "output_id",
    "outputId",
  ];
  for (const path of sharedMetadataPaths) {
    const leftValue = metadataString(left.metadata, [path]);
    const rightValue = metadataString(right.metadata, [path]);
    if (leftValue && rightValue && leftValue === rightValue) {
      return true;
    }
  }
  return false;
}

// isAssistantOutputMessage checks whether one message is a visible assistant prose row.
// isAssistantOutputMessage 检查单条消息是否为可见助手正文行。
function isAssistantOutputMessage(message: ConversationMessage): boolean {
  return message.role === "assistant"
    && normalizedRecordKind(message.recordKind) === "assistantoutput"
    && message.body.trim().length > 0;
}

// assistantOutputsLookLikeSameStream compares two assistant rows using their overlapping streamed body.
// assistantOutputsLookLikeSameStream 通过重叠的流式正文比较两条助手行是否属于同一输出。
function assistantOutputsLookLikeSameStream(left: ConversationMessage, right: ConversationMessage): boolean {
  if (left.conversationId !== right.conversationId) {
    return false;
  }
  const leftBody = left.body.trim();
  const rightBody = right.body.trim();
  if (!leftBody || !rightBody) {
    return false;
  }
  if (leftBody === rightBody || leftBody.includes(rightBody) || rightBody.includes(leftBody)) {
    return true;
  }
  const shorterLength = Math.min(leftBody.length, rightBody.length);
  const prefixLength = Math.min(96, shorterLength);
  return shorterLength >= 16 && leftBody.slice(0, prefixLength) === rightBody.slice(0, prefixLength);
}

// appendStreamingTextDelta appends provider deltas without guessing textual overlap.
// appendStreamingTextDelta 追加 Provider 增量文本，不猜测文本重叠。
function appendStreamingTextDelta(current: string, incoming: string): string {
  if (!incoming) {
    return current;
  }
  return `${current}${incoming}`;
}

// updateSubagentNavigationRunPhase mirrors delta progress into the active child-task badge.
// updateSubagentNavigationRunPhase 将增量进度同步到活动子任务徽标。
function updateSubagentNavigationRunPhase(
  navigation: WorkbenchSnapshot["subagentNavigation"],
  conversationId: string,
  runPhase?: string,
): WorkbenchSnapshot["subagentNavigation"] {
  if (!navigation?.tasks?.some((task) => task.taskId === conversationId)) {
    return navigation;
  }
  const running = Boolean(runPhase && !["idle", "ready", "failed", "stopped"].includes(normalizedSessionRunPhase(runPhase) ?? ""));
  return {
    ...navigation,
    tasks: navigation.tasks.map((task) => (
      task.taskId === conversationId
        ? { ...task, isRunning: running || task.isRunning, runState: running ? "running" : task.runState }
        : task
    )),
  };
}

// syncSubagentSurfaceMessageCacheForDelta updates the cached child transcript touched by one delta.
// syncSubagentSurfaceMessageCacheForDelta 更新单条增量影响的子代理转录缓存。
function syncSubagentSurfaceMessageCacheForDelta(
  cache: SubagentSurfaceMessageCache,
  workbench: WorkbenchSnapshot,
  conversationId: string,
): void {
  const belongsToVisibleTask = workbench.subagentNavigation?.tasks?.some((task) => task.taskId === conversationId) ?? false;
  if (!belongsToVisibleTask && !cache.has(conversationId)) {
    return;
  }
  cache.set(
    conversationId,
    workbench.messages.filter((message) => message.conversationId === conversationId),
  );
}

// groupMessagesByConversation groups a flat message window by owning conversation.
// groupMessagesByConversation 按所属会话分组扁平消息窗口。
function groupMessagesByConversation(messages: ConversationMessage[]): Map<string, ConversationMessage[]> {
  const grouped = new Map<string, ConversationMessage[]>();
  for (const message of messages) {
    const bucket = grouped.get(message.conversationId) ?? [];
    bucket.push(message);
    grouped.set(message.conversationId, bucket);
  }
  return grouped;
}

// mergeIncomingConversationMessages preserves a live streamed answer until the durable final answer appears.
// mergeIncomingConversationMessages 在持久化最终回答出现前保留实时流式回答。
function mergeIncomingConversationMessages(
  currentMessages: ConversationMessage[],
  incomingMessages: ConversationMessage[],
  currentConversation: ConversationSummary | undefined,
  incomingConversation: ConversationSummary | undefined,
): ConversationMessage[] {
  const latestCurrentOutput = latestAssistantOutputAfterLastUser(currentMessages);
  if (!latestCurrentOutput) {
    return incomingMessages;
  }
  const incomingOutput = latestAssistantOutputAfterLastUser(incomingMessages);
  if (incomingOutput) {
    if (assistantOutputCoversStreamedOutput(incomingOutput, latestCurrentOutput)) {
      return incomingMessages;
    }
    if (assistantOutputCoversStreamedOutput(latestCurrentOutput, incomingOutput)) {
      return replaceIncomingAssistantOutput(incomingMessages, incomingOutput.id, latestCurrentOutput);
    }
  }
  const incomingTerminal = incomingConversationHasTerminalStats(incomingMessages);
  const currentRunning = conversationIsRunning(currentConversation);
  const incomingRunning = conversationIsRunning(incomingConversation);
  if (!currentRunning && !incomingRunning && !incomingTerminal) {
    return incomingMessages;
  }
  return insertCarriedStreamingOutput(incomingMessages, latestCurrentOutput);
}

// latestAssistantOutputAfterLastUser returns the newest visible assistant output in the active user turn.
// latestAssistantOutputAfterLastUser 返回当前用户回合内最新的可见助手输出。
function latestAssistantOutputAfterLastUser(messages: ConversationMessage[]): ConversationMessage | null {
  const lastUserTime = Math.max(
    -1,
    ...messages
      .filter((message) => message.role === "user")
      .map((message) => conversationMessageCreatedAtValue(message)),
  );
  return [...messages]
    .reverse()
    .find((message) => (
      isAssistantOutputMessage(message)
      && conversationMessageCreatedAtValue(message) >= lastUserTime
    )) ?? null;
}

// assistantOutputCoversStreamedOutput checks whether a durable output already supersedes a streamed output.
// assistantOutputCoversStreamedOutput 判断持久化输出是否已经覆盖临时流式输出。
function assistantOutputCoversStreamedOutput(incoming: ConversationMessage, streamed: ConversationMessage): boolean {
  const incomingBody = incoming.body.trim();
  const streamedBody = streamed.body.trim();
  return incomingBody === streamedBody
    || incomingBody.includes(streamedBody)
    || (incomingBody.length >= streamedBody.length && incomingBody.startsWith(streamedBody.slice(0, Math.min(96, streamedBody.length))));
}

// incomingConversationHasTerminalStats detects whether the backend has already published turn completion records.
// incomingConversationHasTerminalStats 检测后端是否已经发布回合完成统计记录。
function incomingConversationHasTerminalStats(messages: ConversationMessage[]): boolean {
  return messages.some((message) => normalizedRecordKind(message.recordKind) === "turnstats");
}

// replaceIncomingAssistantOutput swaps a stale snapshot row for the newer streamed row without showing both.
// replaceIncomingAssistantOutput 用更新的流式行替换滞后的快照行，避免同时显示两份输出。
function replaceIncomingAssistantOutput(
  messages: ConversationMessage[],
  incomingOutputId: string,
  streamedOutput: ConversationMessage,
): ConversationMessage[] {
  let replaced = false;
  const nextMessages: ConversationMessage[] = [];
  for (const message of messages) {
    if (message.id === streamedOutput.id && message.id !== incomingOutputId) {
      continue;
    }
    if (message.id === incomingOutputId) {
      nextMessages.push(streamedOutput);
      replaced = true;
      continue;
    }
    nextMessages.push(message);
  }
  return replaced ? nextMessages : insertCarriedStreamingOutput(messages, streamedOutput);
}

// insertCarriedStreamingOutput inserts a carried streamed output before terminal-looking records.
// insertCarriedStreamingOutput 将保留的流式输出插入到终态记录之前。
function insertCarriedStreamingOutput(messages: ConversationMessage[], output: ConversationMessage): ConversationMessage[] {
  if (messages.some((message) => message.id === output.id)) {
    return messages;
  }
  const duplicateIndex = messages.findIndex((message) => isAssistantOutputMessage(message) && assistantOutputsLookLikeSameStream(message, output));
  if (duplicateIndex >= 0) {
    return messages.map((message, index) => (index === duplicateIndex ? output : message));
  }
  const insertIndex = messages.findIndex((message) => {
    const recordKind = normalizedRecordKind(message.recordKind);
    return recordKind === "summaryoutput" || recordKind === "turnstats";
  });
  if (insertIndex < 0) {
    return [...messages, output];
  }
  return [...messages.slice(0, insertIndex), output, ...messages.slice(insertIndex)];
}

// normalizedRecordKind normalizes backend record kind text for semantic comparisons.
// normalizedRecordKind 规范化后端记录类型文本以便语义比较。
function normalizedRecordKind(recordKind?: string | null): string {
  return recordKind?.trim().toLowerCase().replace(/[-_]/g, "") ?? "";
}

// conversationMessageCreatedAtValue parses a message timestamp for turn-local ordering.
// conversationMessageCreatedAtValue 解析消息时间戳用于回合内排序。
function conversationMessageCreatedAtValue(message: ConversationMessage): number {
  const value = Number.parseInt(message.createdAt ?? "", 10);
  return Number.isFinite(value) ? value : -1;
}

// workbenchWithSubagentSurfaceMessageCache absorbs and injects cached child transcript windows for one snapshot.
// workbenchWithSubagentSurfaceMessageCache 为单个快照吸收并注入已缓存的子代理转录窗口。
function workbenchWithSubagentSurfaceMessageCache(
  workbench: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  rememberSubagentSurfaceMessages(cache, workbench);
  return injectCachedSubagentSurfaceMessages(workbench, cache);
}

// mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache merges one backend snapshot while preserving all loaded child surfaces.
// mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache 合并后端快照，同时保留所有已载入的子代理界面。
function mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache(
  current: WorkbenchSnapshot,
  incoming: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  rememberSubagentSurfaceMessages(cache, incoming);
  return injectCachedSubagentSurfaceMessages(mergeWorkbenchSnapshot(current, incoming), cache);
}

// mergeConversationMessagesWithSubagentSurfaceMessageCache replaces one message window and updates child-surface cache when applicable.
// mergeConversationMessagesWithSubagentSurfaceMessageCache 替换单个消息窗口，并在适用时更新子代理界面缓存。
function mergeConversationMessagesWithSubagentSurfaceMessageCache(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  messages: ConversationMessage[],
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  if (workbench.subagentNavigation?.tasks?.some((task) => task.taskId === conversationId)) {
    cache.set(conversationId, messages);
  }
  return injectCachedSubagentSurfaceMessages(mergeConversationMessages(workbench, conversationId, messages), cache);
}

// rememberSubagentSurfaceMessages records message windows belonging to the current subagent task list.
// rememberSubagentSurfaceMessages 记录属于当前子代理任务列表的消息窗口。
function rememberSubagentSurfaceMessages(cache: SubagentSurfaceMessageCache, workbench: WorkbenchSnapshot): void {
  const taskIds = new Set(workbench.subagentNavigation?.tasks?.map((task) => task.taskId) ?? []);
  if (taskIds.size === 0) {
    return;
  }
  const messagesByTaskId = new Map<string, ConversationMessage[]>();
  for (const message of workbench.messages) {
    if (!taskIds.has(message.conversationId)) {
      continue;
    }
    const taskMessages = messagesByTaskId.get(message.conversationId) ?? [];
    taskMessages.push(message);
    messagesByTaskId.set(message.conversationId, taskMessages);
  }
  for (const [taskId, messages] of messagesByTaskId) {
    cache.set(taskId, messages);
  }
}

// injectCachedSubagentSurfaceMessages reattaches every cached child message window for the visible parent task list.
// injectCachedSubagentSurfaceMessages 为当前可见父任务列表重新挂回所有已缓存的子代理消息窗口。
function injectCachedSubagentSurfaceMessages(
  workbench: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  const taskIdsWithCache = new Set(
    (workbench.subagentNavigation?.tasks ?? [])
      .map((task) => task.taskId)
      .filter((taskId) => cache.has(taskId)),
  );
  if (taskIdsWithCache.size === 0) {
    return workbench;
  }
  const retainedMessages = workbench.messages.filter((message) => !taskIdsWithCache.has(message.conversationId));
  const cachedMessages = [...taskIdsWithCache].flatMap((taskId) => cache.get(taskId) ?? []);
  return {
    ...workbench,
    messages: [...retainedMessages, ...cachedMessages],
  };
}

// hasCachedSubagentSurfaceMessages returns whether one child transcript is already loaded in this frontend session.
// hasCachedSubagentSurfaceMessages 返回当前前端会话中是否已经载入指定子转录。
function hasCachedSubagentSurfaceMessages(cache: SubagentSurfaceMessageCache, taskId: string): boolean {
  return (cache.get(taskId)?.length ?? 0) > 0;
}

// canOpenCachedSubagentSurface verifies that cached messages and the current navigation can both satisfy a local route switch.
// canOpenCachedSubagentSurface 校验缓存消息与当前导航是否都足以完成本地路由切换。
function canOpenCachedSubagentSurface(
  workbench: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
  parentConversationId: string,
  taskId: string,
): boolean {
  return hasCachedSubagentSurfaceMessages(cache, taskId)
    && workbench.subagentNavigation?.parentConversationId === parentConversationId
    && Boolean(workbench.subagentNavigation.tasks?.some((task) => task.taskId === taskId));
}

// applySubagentRoutePreference keeps late backend snapshots from overriding the user's latest WebUI route choice.
// applySubagentRoutePreference 防止迟到的后端快照覆盖用户最后选择的 WebUI 路由。
function applySubagentRoutePreference(workbench: WorkbenchSnapshot, preference: SubagentRoutePreference | null): WorkbenchSnapshot {
  if (!preference || workbench.subagentNavigation?.parentConversationId !== preference.parentConversationId) {
    return workbench;
  }
  if (!preference.taskId) {
    return closeSubagentSurfaceForConversation(workbench, preference.parentConversationId);
  }
  return openSubagentSurfaceForConversation(workbench, preference.parentConversationId, preference.taskId);
}

// openSubagentSurfaceForConversation switches a known child task into view without changing backend execution state.
// openSubagentSurfaceForConversation 在不改变后端执行状态的情况下将已知子任务切入视图。
function openSubagentSurfaceForConversation(
  workbench: WorkbenchSnapshot,
  parentConversationId: string,
  taskId: string,
): WorkbenchSnapshot {
  const navigation = workbench.subagentNavigation;
  if (!navigation || navigation.parentConversationId !== parentConversationId) {
    return workbench;
  }
  if (!navigation.tasks?.some((task) => task.taskId === taskId)) {
    return workbench;
  }
  return {
    ...workbench,
    subagentNavigation: {
      ...navigation,
      activeConversationId: taskId,
      activeTaskId: taskId,
      tasks: navigation.tasks.map((task) => ({ ...task, isActive: task.taskId === taskId })),
    },
  };
}

// closeSubagentSurfaceForConversation returns the visible route to the selected parent conversation while preserving its task list.
// closeSubagentSurfaceForConversation 将可见路由返回到选中的父会话，同时保留该父会话的子任务列表。
function closeSubagentSurfaceForConversation(workbench: WorkbenchSnapshot, conversationId: string): WorkbenchSnapshot {
  const navigation = workbench.subagentNavigation;
  if (!navigation) {
    return workbench;
  }
  if (navigation.parentConversationId !== conversationId) {
    return {
      ...workbench,
      subagentNavigation: null,
    };
  }
  return {
    ...workbench,
    subagentNavigation: {
      ...navigation,
      activeConversationId: conversationId,
      activeTaskId: null,
      tasks: navigation.tasks?.map((task) => ({ ...task, isActive: false })),
    },
  };
}

// mergeConversationSummaries preserves only optimistic local read state while trusting backend lifecycle state.
// mergeConversationSummaries 仅保留本地乐观已读状态，同时信任后端生命周期状态。
function mergeConversationSummaries(
  current: ConversationSummary[],
  incoming: ConversationSummary[],
  selectedConversationId: string | null | undefined,
): ConversationSummary[] {
  const currentById = new Map(current.map((conversation) => [conversation.id, conversation]));
  return incoming.map((conversation) => {
    const currentConversation = currentById.get(conversation.id);
    if (!currentConversation) {
      return conversation;
    }
    const keepUnread = conversation.id !== selectedConversationId
      && (conversation.unread || currentConversation.unread);
    return {
      ...conversation,
      unread: keepUnread,
    };
  });
}

// reorderPendingPromptSummaries applies an optimistic pending-prompt order by id.
// reorderPendingPromptSummaries 根据标识应用乐观待发送提示词排序。
function reorderPendingPromptSummaries(entries: PendingPromptSummary[], entryIds: string[]): PendingPromptSummary[] {
  const entriesById = new Map(entries.map((entry) => [entry.id, entry]));
  const reordered = entryIds
    .map((entryId) => entriesById.get(entryId))
    .filter((entry): entry is PendingPromptSummary => Boolean(entry));
  const orderedIds = new Set(entryIds);
  return [
    ...reordered,
    ...entries.filter((entry) => !orderedIds.has(entry.id)),
  ];
}

// useVerticalSortableItem centralizes the sidebar-proven vertical sortable item behavior.
// useVerticalSortableItem 集中左侧侧栏已验证的纵向排序条目行为。
function useVerticalSortableItem(id: string, disabled = false) {
  // sortable owns the dnd-kit state and drag listeners for one vertical item.
  // sortable 持有单个纵向条目的 dnd-kit 状态与拖拽监听。
  const sortable = useSortable({ id, disabled });
  // sortableTransform keeps movement vertical-only and prevents size-based text scaling.
  // sortableTransform 将移动限制为仅纵向，并阻止基于尺寸差异的文字缩放。
  const sortableTransform = sortable.transform ? { ...sortable.transform, x: 0, scaleX: 1, scaleY: 1 } : null;
  // sortableStyle is shared by project rows and pending prompt rows.
  // sortableStyle 由项目行和待发送提示词行共同使用。
  const sortableStyle: React.CSSProperties = {
    transform: CSS.Transform.toString(sortableTransform),
    transition: sortable.transition,
  };
  return {
    ...sortable,
    sortableStyle,
  };
}

// mergeConversationMessages replaces one conversation message window in the local workbench state.
// mergeConversationMessages 替换本地工作台状态中单个会话的消息窗口。
function mergeConversationMessages(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  messages: ConversationMessage[],
): WorkbenchSnapshot {
  return {
    ...workbench,
    messages: [
      ...workbench.messages.filter((message) => message.conversationId !== conversationId),
      ...messages,
    ],
  };
}

// markConversationLocallyRead clears the unread dot optimistically in the sidebar.
// markConversationLocallyRead 在侧栏中乐观清除未读圆点。
function markConversationLocallyRead(workbench: WorkbenchSnapshot, conversationId: string): WorkbenchSnapshot {
  return {
    ...workbench,
    conversations: workbench.conversations.map((conversation) => (
      conversation.id === conversationId ? { ...conversation, unread: false } : conversation
    )),
  };
}

// ProjectMenuAction names project-tree context menu commands.
// ProjectMenuAction 命名项目树右键菜单命令。
type ProjectMenuAction = "toggle-pin" | "open-directory" | "rename" | "set-icon" | "archive-conversations" | "settings";

// ProjectIconDialogTarget stores the project currently editing its icon.
// ProjectIconDialogTarget 保存当前正在编辑图标的项目。
interface ProjectIconDialogTarget {
  // Project being edited by the icon dialog.
  // 图标弹窗正在编辑的项目。
  project: ProjectSummary;
}

// ConversationMenuAction names conversation-tree context menu commands.
// ConversationMenuAction 命名会话树右键菜单命令。
type ConversationMenuAction =
  | "pin"
  | "rename"
  | "archive"
  | "open-directory"
  | "copy-workdir"
  | "copy-session-id";

// RenameTarget describes one pending rename dialog target.
// RenameTarget 描述一个待处理的重命名对话框目标。
interface RenameTarget {
  // Target kind used to route the final rename command.
  // 用于路由最终重命名命令的目标类型。
  kind: "project" | "conversation";
  // Stable target identifier.
  // 稳定目标标识。
  id: string;
  // Current display name.
  // 当前显示名称。
  name: string;
}

// applyPreviewProjectAction simulates project-menu commands for browser-only preview mode.
// applyPreviewProjectAction 为纯浏览器预览模式模拟项目菜单命令。
function applyPreviewProjectAction(
  workbench: WorkbenchSnapshot,
  projectId: string,
  action: ProjectMenuAction,
): { snapshot: WorkbenchSnapshot; message: string } {
  const project = workbench.projects.find((candidate) => candidate.id === projectId);
  if (!project) {
    return { snapshot: workbench, message: "没有找到对应项目。" };
  }

  if (action === "toggle-pin") {
    const pinned = !project.pinned;
    return {
      snapshot: {
        ...workbench,
        projects: workbench.projects.map((candidate) => candidate.id === projectId ? { ...candidate, pinned } : candidate),
      },
      message: pinned ? `已将项目 ${project.name} 置顶。` : `已取消项目 ${project.name} 的置顶状态。`,
    };
  }

  const archivedConversationIds = new Set(
    workbench.conversations
      .filter((conversation) => conversation.projectId === projectId)
      .map((conversation) => conversation.id),
  );
  const conversations = workbench.conversations.filter((conversation) => !archivedConversationIds.has(conversation.id));
  const selectedConversationId = archivedConversationIds.has(workbench.selectedConversationId ?? "")
    ? conversations[0]?.id ?? null
    : workbench.selectedConversationId;

  return {
    snapshot: {
      ...workbench,
      conversations,
      selectedConversationId,
      messages: workbench.messages.filter((message) => !archivedConversationIds.has(message.conversationId)),
    },
    message: `已归档项目 ${project.name} 下的对话。`,
  };
}

// applyPreviewProjectRename simulates project rename for browser-only preview mode.
// applyPreviewProjectRename 为纯浏览器预览模式模拟项目重命名。
function applyPreviewProjectRename(
  workbench: WorkbenchSnapshot,
  projectId: string,
  name: string,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      projects: workbench.projects.map((candidate) => candidate.id === projectId ? { ...candidate, name } : candidate),
    },
  };
}

// applyPreviewProjectIcon simulates project icon persistence for browser-only preview mode.
// applyPreviewProjectIcon 为纯浏览器预览模式模拟项目图标持久化。
function applyPreviewProjectIcon(
  workbench: WorkbenchSnapshot,
  projectId: string,
  icon: ProjectIconSelection | null,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      projects: workbench.projects.map((candidate) => candidate.id === projectId ? {
        ...candidate,
        iconPack: icon?.pack ?? null,
        iconKey: icon?.key ?? null,
      } : candidate),
    },
  };
}

// applyPreviewConversationRename simulates conversation rename for browser-only preview mode.
// applyPreviewConversationRename 为纯浏览器预览模式模拟会话重命名。
function applyPreviewConversationRename(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  title: string,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      conversations: workbench.conversations.map((conversation) => (
        conversation.id === conversationId ? { ...conversation, title } : conversation
      )),
    },
  };
}

// applyPreviewProjectOrder simulates same-section project ordering in browser preview mode.
// applyPreviewProjectOrder 在浏览器预览模式下模拟同分区项目排序。
function applyPreviewProjectOrder(workbench: WorkbenchSnapshot, orderedProjectIds: string[]): { snapshot: WorkbenchSnapshot } {
  if (orderedProjectIds.length === 0) {
    return { snapshot: workbench };
  }

  const orderedSet = new Set(orderedProjectIds);
  const sectionPinned = Boolean(workbench.projects.find((project) => project.id === orderedProjectIds[0])?.pinned);
  const orderedProjects: ProjectSummary[] = [];
  for (const projectId of orderedProjectIds) {
    const project = workbench.projects.find((candidate) => candidate.id === projectId);
    if (project && Boolean(project.pinned) === sectionPinned) {
      orderedProjects.push(project);
    }
  }
  const remainingSectionProjects = workbench.projects.filter((project) => (
    Boolean(project.pinned) === sectionPinned && !orderedSet.has(project.id)
  ));
  const oppositeSectionProjects = workbench.projects.filter((project) => Boolean(project.pinned) !== sectionPinned);
  const nextSectionProjects = [...orderedProjects, ...remainingSectionProjects];
  const nextProjects = sectionPinned
    ? [...nextSectionProjects, ...oppositeSectionProjects]
    : [...oppositeSectionProjects, ...nextSectionProjects];
  return { snapshot: { ...workbench, projects: nextProjects } };
}

// applyPreviewConversationAction simulates conversation-menu commands for browser-only preview mode.
// applyPreviewConversationAction 为纯浏览器预览模式模拟会话菜单命令。
function applyPreviewConversationAction(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  action: ConversationMenuAction,
): { snapshot: WorkbenchSnapshot } {
  if (action !== "archive") {
    return { snapshot: workbench };
  }
  const conversations = workbench.conversations.filter((conversation) => conversation.id !== conversationId);
  const selectedConversationId = workbench.selectedConversationId === conversationId
    ? conversations[0]?.id ?? null
    : workbench.selectedConversationId;
  return {
    snapshot: {
      ...workbench,
      conversations,
      selectedConversationId,
      messages: workbench.messages.filter((message) => message.conversationId !== conversationId),
    },
  };
}

// projectForConversation returns the project that owns one conversation.
// projectForConversation 返回拥有某个会话的项目。
function projectForConversation(
  workbench: WorkbenchSnapshot,
  conversation: ConversationSummary,
): ProjectSummary | null {
  const fallbackProject = workbench.projects[0] ?? null;
  return workbench.projects.find((project) => project.id === conversation.projectId) ?? fallbackProject;
}

// encodeProjectBadgeKey serializes a generated badge config for project persistence.
// encodeProjectBadgeKey 将生成式徽标配置序列化用于项目持久化。
function encodeProjectBadgeKey(config: ProjectIconBadgeConfig): string {
  return JSON.stringify(config);
}

// decodeProjectBadgeKey restores a generated badge config from persistence.
// decodeProjectBadgeKey 从持久化内容还原生成式徽标配置。
function decodeProjectBadgeKey(value?: string | null): ProjectIconBadgeConfig | null {
  if (!value) {
    return null;
  }
  try {
    const parsed = JSON.parse(value) as Partial<ProjectIconBadgeConfig>;
    const shape = normalizeProjectBadgeShape(parsed.shape);
    const text = normalizeProjectBadgeText(parsed.text ?? "");
    if (!shape || !text) {
      return null;
    }
    return {
      shape,
      text,
      background: normalizeCssColor(parsed.background, DEFAULT_PROJECT_BADGE_CONFIG.background),
      border: normalizeCssColor(parsed.border, DEFAULT_PROJECT_BADGE_CONFIG.border),
      foreground: normalizeCssColor(parsed.foreground, DEFAULT_PROJECT_BADGE_CONFIG.foreground),
    };
  } catch {
    return null;
  }
}

// normalizeProjectBadgeShape keeps new badges on the simplified circle/square model.
// normalizeProjectBadgeShape 将新徽标限制在简化后的圆形/方形模型。
function normalizeProjectBadgeShape(value: unknown): ProjectIconBadgeShape | null {
  if (value === "circle") {
    return "circle";
  }
  if (value === "square" || value === "rounded" || value === "sharp") {
    return "square";
  }
  return null;
}

// ProjectIcon renders either a generated badge, a selected icon, or the default folder state.
// ProjectIcon 渲染生成式徽标、已选图标或默认文件夹状态。
function ProjectIcon(props: { project: ProjectSummary; isOpen?: boolean; size?: number }) {
  if (props.project.iconPack === "badge") {
    const config = decodeProjectBadgeKey(props.project.iconKey) ?? DEFAULT_PROJECT_BADGE_CONFIG;
    return <ProjectBadgeIcon config={config} size={props.size ?? 16} />;
  }

  if (isIconifyProjectIconPack(props.project.iconPack) && props.project.iconKey) {
    return (
      <IconifyIcon
        aria-hidden="true"
        className="project-iconify-icon"
        height={props.size ?? 16}
        icon={`${props.project.iconPack}:${props.project.iconKey}`}
        width={props.size ?? 16}
      />
    );
  }

  if (props.project.iconPack === "lucide" && props.project.iconKey) {
    const option = PROJECT_ICON_MAP.get(props.project.iconKey);
    if (option?.Icon) {
      return <option.Icon size={props.size ?? 16} />;
    }
  }

  return props.isOpen ? <FolderOpen size={props.size ?? 16} /> : <Folder size={props.size ?? 16} />;
}

// isIconifyProjectIconPack checks whether a persisted project icon pack belongs to Iconify color sets.
// isIconifyProjectIconPack 检查持久化的项目图标包是否属于 Iconify 彩色图标集。
function isIconifyProjectIconPack(value: string | null | undefined): value is Extract<ProjectIconPack, "skill-icons" | "material-icon-theme"> {
  return value === "skill-icons" || value === "material-icon-theme";
}

// projectIconOptionsForTab returns the icon option list owned by one picker tab.
// projectIconOptionsForTab 返回某个选择器标签页拥有的图标选项列表。
function projectIconOptionsForTab(tab: ProjectIconTab): ProjectIconOption[] {
  if (tab === "skill-icons") {
    return SKILL_PROJECT_ICON_OPTIONS;
  }
  if (tab === "material-icon-theme") {
    return MATERIAL_PROJECT_ICON_OPTIONS;
  }
  if (tab === "lucide") {
    return LUCIDE_PROJECT_ICON_OPTIONS;
  }
  return [];
}

// projectIconPackLabel returns the user-facing name for one project icon pack.
// projectIconPackLabel 返回单个项目图标包的面向用户名称。
function projectIconPackLabel(pack: ProjectIconPack): string {
  if (pack === "skill-icons") {
    return "Skill Icons";
  }
  if (pack === "material-icon-theme") {
    return "Material Icon Theme";
  }
  return "Lucide";
}

// ProjectBadgeIcon renders the generated project badge as an inline SVG.
// ProjectBadgeIcon 将生成式项目徽标渲染为内联 SVG。
function ProjectBadgeIcon(props: { config: ProjectIconBadgeConfig; size: number }) {
  const isCircle = props.config.shape === "circle";
  // fontSize uses the 64px badge template scale, matching the visual density of the reference SVG.
  // fontSize 使用 64px 徽标模板比例，匹配参考 SVG 的视觉密度。
  const fontSize = props.config.text.length > 1 ? 36 : 38;
  // labelY follows the reference badge baseline so Latin and CJK glyphs sit at the optical center.
  // labelY 采用参考徽标的文字基线，让拉丁与中文字符落在视觉中心。
  const labelY = props.config.text.length > 1 ? 36 : 35.5;
  return (
    <svg
      aria-hidden="true"
      className="project-badge-icon"
      height={props.size}
      viewBox="0 0 64 64"
      width={props.size}
    >
      {isCircle ? (
        <>
          <circle
            cx="32"
            cy="32"
            fill={props.config.background}
            r="32"
          />
          <circle
            cx="32"
            cy="32"
            fill="none"
            r="29.5"
            stroke={props.config.border}
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="5"
          />
        </>
      ) : (
        <rect
          fill={props.config.background}
          height="59"
          rx="6"
          stroke={props.config.border}
          strokeWidth="5"
          width="59"
          x="2.5"
          y="2.5"
        />
      )}
      <text
        dominantBaseline="middle"
        fill={props.config.foreground}
        fontFamily="system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, Microsoft YaHei, Noto Sans CJK SC, sans-serif"
        fontSize={fontSize}
        fontWeight="600"
        textAnchor="middle"
        x="32"
        y={labelY}
      >
        {props.config.text}
      </text>
    </svg>
  );
}

// normalizeProjectBadgeText keeps one Chinese character or two Latin letters.
// normalizeProjectBadgeText 保留一个中文字符或两个英文字母。
function normalizeProjectBadgeText(value: string): string {
  const trimmed = value.trim();
  const chinese = Array.from(trimmed).find((char) => /\p{Script=Han}/u.test(char));
  if (chinese) {
    return chinese;
  }
  return trimmed.replace(/[^a-zA-Z]/g, "").slice(0, 2).toUpperCase();
}

// normalizeCssColor keeps a simple hex color or falls back to a known value.
// normalizeCssColor 保留简单十六进制颜色，或回退到已知值。
function normalizeCssColor(value: unknown, fallback: string): string {
  if (typeof value !== "string") {
    return fallback;
  }
  return /^#[0-9a-fA-F]{6}$/.test(value) ? value : fallback;
}

// projectConversationCount returns the visible conversation count under one project.
// projectConversationCount 返回单个项目下的可见会话数量。
function projectConversationCount(workbench: WorkbenchSnapshot, projectId: string): number {
  return workbench.conversations.filter((conversation) => conversation.projectId === projectId).length;
}

// markdownPlugins stores the Markdown extension set shared by inline and modal renderers.
// markdownPlugins 保存内联与弹窗渲染器共享的 Markdown 扩展集合。
const markdownPlugins = [remarkGfm];

// markdownSanitizers stores the HTML sanitization pipeline for untrusted transcript content.
// markdownSanitizers 保存用于不可信转录内容的 HTML 清理管线。
const markdownSanitizers = [rehypeSanitize];

// MarkdownImageWorkspacePathContext carries the current project root for relative image links.
// MarkdownImageWorkspacePathContext 为相对图片链接携带当前项目根目录。
const MarkdownImageWorkspacePathContext = React.createContext<string | null>(null);

// STREAMING_DELTA_FLUSH_MS batches hot transcript deltas so React and GC are not driven at display-frame cadence.
// STREAMING_DELTA_FLUSH_MS 将高频转录增量批处理，避免 React 与 GC 被显示帧率牵引。
const STREAMING_DELTA_FLUSH_MS = 80;

// CONVERSATION_VIRTUALIZATION_MIN_ITEMS avoids adding measurement wrappers to short transcripts.
// CONVERSATION_VIRTUALIZATION_MIN_ITEMS 避免给短转录添加测量包装层。
const CONVERSATION_VIRTUALIZATION_MIN_ITEMS = 80;

// CONVERSATION_VIRTUALIZATION_OVERSCAN_PX keeps nearby transcript rows mounted around the viewport.
// CONVERSATION_VIRTUALIZATION_OVERSCAN_PX 让视口附近的转录行保持挂载。
const CONVERSATION_VIRTUALIZATION_OVERSCAN_PX = 1200;

// CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX is the fallback row height before measurement.
// CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX 是测量前使用的兜底行高。
const CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX = 220;

// CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX is the fallback process-group height before measurement.
// CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX 是测量前处理过程分组使用的兜底高度。
const CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX = 320;

// USER_PROMPT_COLLAPSE_LINE_LIMIT limits long user bubbles before the user expands them.
// USER_PROMPT_COLLAPSE_LINE_LIMIT 限制用户手动展开前长用户气泡的显示行数。
const USER_PROMPT_COLLAPSE_LINE_LIMIT = 20;

// STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT limits tail-follow signatures to messages that can affect the viewport tail.
// STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT 将尾部追随签名限制在会影响视口尾部的消息范围。
const STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT = 8;

// webuiClickTrace emits lightweight browser-console diagnostics for user initiated WebUI actions.
// webuiClickTrace 为用户触发的 WebUI 操作输出轻量浏览器控制台诊断日志。
function webuiClickTrace(_action: string, _detail: Record<string, unknown> = {}): void {
  // Kept as a no-op compatibility hook so existing click paths stay unchanged without emitting dev logs.
  // 保留为空实现兼容钩子，避免改变现有点击路径，同时不再输出开发日志。
}

// ProjectSettingsDialogTarget stores the project currently shown in the settings dialog.
// ProjectSettingsDialogTarget 保存当前显示设置弹窗的项目。
interface ProjectSettingsDialogTarget {
  // Project being configured by the settings dialog.
  // 设置弹窗正在配置的项目。
  project: ProjectSummary;
}

// directiveKinds maps Vulcan directive names to WebUI conversation block kinds.
// directiveKinds 将 Vulcan 指令名称映射为 WebUI 会话块类型。
const directiveKinds: Record<string, ConversationBlockKind> = {
  "tool-call": "tool-call",
  "tool-result": "tool-result",
  "turn-stats": "turn-stats",
  thinking: "thinking",
  diff: "diff",
  notice: "notice",
};

// detailEligibleBlockKinds lists block types that should open a dedicated detail modal.
// detailEligibleBlockKinds 列出应打开专用详情弹窗的块类型。
const detailEligibleBlockKinds = new Set<ConversationBlockKind>(["tool-call", "tool-result", "command", "diff", "notice"]);

// projectConversationRenderItems merges related low-level transcript rows for display.
// projectConversationRenderItems 合并相关底层转录行以生成最终展示项。
function projectConversationRenderItems(messages: ConversationMessage[]): ConversationRenderItem[] {
  const items: ConversationRenderItem[] = [];
  const toolCallItems = new Map<string, ConversationRenderItem>();
  const compactionItems = new Map<string, ConversationRenderItem>();
  let activeCompactionSectionId: string | null = null;

  const drainTrailingCompactionThinkingBlocks = (sectionId: string): ConversationBlock[] => {
    const collected: ConversationBlock[] = [];
    while (items.length > 0) {
      const candidate = items[items.length - 1];
      if (!candidate || candidate.role !== "assistant" || !candidate.blocks.every((block) => block.kind === "thinking")) {
        break;
      }
      items.pop();
      collected.unshift(...candidate.blocks.map((block) => ({
        ...block,
        compactionSectionId: sectionId,
        compactionRole: "thinking",
      })));
    }
    return collected;
  };

  const appendCompactionBlock = (sectionId: string, block: ConversationBlock, message: ConversationMessage) => {
    const existing = compactionItems.get(sectionId);
    if (existing) {
      existing.blocks = mergeContextCompactionBlocks(existing.blocks, block);
      return existing;
    }
    const leadingThinkingBlocks = drainTrailingCompactionThinkingBlocks(sectionId);
    const item: ConversationRenderItem = {
      id: `context-compaction-${sectionId}`,
      role: message.role === "system" ? "system" : "assistant",
      blocks: mergeContextCompactionBlocks(leadingThinkingBlocks, block),
      sourceMessage: message,
    };
    items.push(item);
    compactionItems.set(sectionId, item);
    return item;
  };

  for (const message of messages) {
    if (message.role === "user") {
      items.push({
        id: message.id,
        role: "user",
        blocks: [],
        sourceMessage: message,
        userMessage: message,
      });
      continue;
    }

    const blocks = parseConversationBlocks(message);
    for (const block of blocks) {
      if (isContextCompactionBlock(block)) {
        const sectionId = block.compactionSectionId ?? block.id;
        appendCompactionBlock(sectionId, block, message);
        if (block.compactionRole === "start") {
          activeCompactionSectionId = sectionId;
        }
        if (block.compactionRole === "end") {
          activeCompactionSectionId = activeCompactionSectionId === sectionId ? null : activeCompactionSectionId;
        }
        continue;
      }

      if (activeCompactionSectionId && block.kind === "thinking") {
        appendCompactionBlock(activeCompactionSectionId, {
          ...block,
          compactionSectionId: activeCompactionSectionId,
          compactionRole: "thinking",
        }, message);
        continue;
      }

      if (isMergeableToolBlock(block)) {
        const existing = block.kind !== "tool-call" ? findToolMergeItem(toolCallItems, block) : undefined;
        if (existing) {
          existing.blocks = mergeToolBlocks(existing.blocks, block);
          registerToolMergeItem(toolCallItems, existing);
          continue;
        }

        const item: ConversationRenderItem = {
          id: block.id,
          role: message.role === "system" ? "system" : "assistant",
          blocks: [block],
          sourceMessage: message,
        };
        items.push(item);
        registerToolMergeItem(toolCallItems, item);
        continue;
      }

      items.push({
        id: block.id,
        role: message.role === "system" ? "system" : "assistant",
        blocks: [block],
        sourceMessage: message,
      });
    }
  }

  return items;
}

// isContextCompactionBlock detects host-provided context-compaction status rows.
// isContextCompactionBlock 检测宿主提供的上下文压缩状态行。
function isContextCompactionBlock(block: ConversationBlock): boolean {
  return block.kind === "notice" && Boolean(block.compactionSectionId || block.compactionRole || block.compactionState);
}

// mergeContextCompactionBlocks keeps one start/body/end order for a compaction section.
// mergeContextCompactionBlocks 为单个压缩分段保持 start/body/end 顺序。
function mergeContextCompactionBlocks(blocks: ConversationBlock[], block: ConversationBlock): ConversationBlock[] {
  const next = blocks.filter((candidate) => candidate.id !== block.id);
  next.push(block);
  return next.sort((left, right) => contextCompactionRoleOrder(left.compactionRole) - contextCompactionRoleOrder(right.compactionRole));
}

// contextCompactionRoleOrder returns the display order for merged compaction rows.
// contextCompactionRoleOrder 返回合并压缩行的展示顺序。
function contextCompactionRoleOrder(role?: string): number {
  if (role === "start") return 0;
  if (role === "thinking") return 1;
  if (role === "body") return 2;
  if (role === "end") return 3;
  return 4;
}

// conversationProcessCollapseMemory keeps manual process fold state for this WebUI session only.
// conversationProcessCollapseMemory 仅在当前 WebUI 会话内保留手动处理过程折叠状态。
const conversationProcessCollapseMemory = new Map<string, boolean>();

// conversationUserPromptExpansionMemory keeps manual user prompt expansion state for this WebUI session only.
// conversationUserPromptExpansionMemory 仅在当前 WebUI 会话内保留手动用户提示词展开状态。
const conversationUserPromptExpansionMemory = new Set<string>();

// projectConversationTimelineItems groups each user turn into a visible processing shell.
// projectConversationTimelineItems 将每个用户轮次分组成可见处理过程外壳。
function projectConversationTimelineItems(
  renderItems: ConversationRenderItem[],
  conversation: ConversationSummary | undefined,
  nowMs: number,
): ConversationTimelineItem[] {
  const timeline: ConversationTimelineItem[] = [];
  const turnStartUserIds = conversationTurnStartUserIds(renderItems);
  const turnStartIdsWithLaterTurn = new Set(turnStartUserIds.slice(0, -1));
  let currentTurn: {
    userItem: ConversationRenderItem;
    bodyItems: ConversationRenderItem[];
    guidanceItems: ConversationRenderItem[];
    statsItem: ConversationRenderItem | null;
  } | null = null;

  const flushTurn = (forceRunning: boolean) => {
    if (!currentTurn) {
      return;
    }
    timeline.push({ kind: "item", item: currentTurn.userItem });
    const stats = currentTurn.statsItem?.blocks[0]?.rawMetadata;
    const statsStatus = turnStatsStatus(stats);
    const stopped = statsStatus === "stopped";
    const terminal = Boolean(currentTurn.statsItem);
    const running = forceRunning && !terminal;
    const bodyItems = [...currentTurn.bodyItems];
    const finalItem = !running && !stopped ? popFinalAssistantItem(bodyItems) : null;
    const processItems = dedupeProcessAssistantMarkdownItems(bodyItems.filter((item) => !isTurnStatsRenderItem(item)));
    const shouldDefaultCollapse = Boolean(finalItem) && !running && !stopped && turnStartIdsWithLaterTurn.has(currentTurn.userItem.id);
    const startedAtMs = conversationMessageUnixMillis(currentTurn.userItem.sourceMessage ?? currentTurn.userItem.userMessage);
    const terminalMessageMs = conversationMessageUnixMillis(finalItem?.sourceMessage ?? currentTurn.statsItem?.sourceMessage);
    const projectedDurationMs = startedAtMs && terminalMessageMs
      ? Math.max(0, terminalMessageMs - startedAtMs)
      : null;
    const runningDurationMs = running
      ? (startedAtMs ? Math.max(0, nowMs - startedAtMs) : null)
      : null;
    if (processItems.length > 0 || running || stopped || currentTurn.guidanceItems.length > 0) {
      timeline.push({
        kind: "process",
        process: {
          id: `process-${currentTurn.userItem.id}`,
          items: processItems,
          guidanceItems: currentTurn.guidanceItems,
          collapsible: Boolean(finalItem) && !running && !stopped,
          defaultCollapsed: shouldDefaultCollapse,
          running,
          stopped,
          startedAtMs,
          durationMs: turnStatsDurationMs(stats) ?? runningDurationMs ?? projectedDurationMs,
        },
      });
    }
    if (finalItem) {
      timeline.push({ kind: "item", item: finalItem });
    }
    if (currentTurn.statsItem) {
      timeline.push({ kind: "item", item: currentTurn.statsItem });
    }
    currentTurn = null;
  };

  for (const item of renderItems) {
    if (item.role === "user") {
      if (!currentTurn || currentTurn.statsItem) {
        flushTurn(false);
        currentTurn = { userItem: item, bodyItems: [], guidanceItems: [], statsItem: null };
        continue;
      }
      currentTurn.guidanceItems.push(item);
      continue;
    }
    if (!currentTurn) {
      timeline.push({ kind: "item", item });
      continue;
    }
    if (isTurnStatsRenderItem(item)) {
      currentTurn.statsItem = item;
      flushTurn(false);
      continue;
    }
    currentTurn.bodyItems.push(item);
  }

  flushTurn(conversationIsRunning(conversation));
  return timeline;
}

// conversationTurnStartUserIds derives true user-turn starts, excluding guidance prompts inside a running turn.
// conversationTurnStartUserIds 推导真实用户轮次起点，并排除运行中追加的引导输入。
function conversationTurnStartUserIds(renderItems: ConversationRenderItem[]): string[] {
  const turnStartIds: string[] = [];
  let hasOpenTurn = false;
  let currentTurnHasStats = false;
  for (const item of renderItems) {
    if (item.role === "user") {
      if (!hasOpenTurn || currentTurnHasStats) {
        turnStartIds.push(item.id);
        hasOpenTurn = true;
        currentTurnHasStats = false;
      }
      continue;
    }
    if (!hasOpenTurn) {
      continue;
    }
    if (isTurnStatsRenderItem(item)) {
      currentTurnHasStats = true;
    }
  }
  return turnStartIds;
}

// isTurnStatsRenderItem returns whether one projected row is the terminal turn-statistics row.
// isTurnStatsRenderItem 返回单条投影行是否为终态轮次统计行。
function isTurnStatsRenderItem(item: ConversationRenderItem): boolean {
  return item.blocks.some((block) => block.kind === "turn-stats");
}

// popFinalAssistantItem removes and returns the final assistant markdown row when present.
// popFinalAssistantItem 在存在最终助手 Markdown 行时移除并返回它。
function popFinalAssistantItem(items: ConversationRenderItem[]): ConversationRenderItem | null {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (item.role === "assistant" && item.blocks.length === 1 && item.blocks[0]?.kind === "markdown") {
      items.splice(index, 1);
      return item;
    }
  }
  return null;
}

// dedupeProcessAssistantMarkdownItems hides repeated short assistant prose inside one turn process group.
// dedupeProcessAssistantMarkdownItems 隐藏同一轮处理过程中的重复短助手正文。
function dedupeProcessAssistantMarkdownItems(items: ConversationRenderItem[]): ConversationRenderItem[] {
  const hiddenIndexes = new Set<number>();
  for (let index = 0; index < items.length; index += 1) {
    const item = items[index];
    const body = processAssistantMarkdownBody(item);
    if (!body) {
      continue;
    }
    for (let nextIndex = index + 1; nextIndex < items.length; nextIndex += 1) {
      const nextBody = processAssistantMarkdownBody(items[nextIndex]);
      if (!nextBody) {
        continue;
      }
      if (assistantMarkdownProcessBodyCoveredByLater(body, nextBody)) {
        hiddenIndexes.add(index);
        break;
      }
    }
  }
  return items.filter((_, index) => !hiddenIndexes.has(index));
}

// processAssistantMarkdownBody returns the plain Markdown body for assistant prose rows only.
// processAssistantMarkdownBody 仅返回助手正文行的 Markdown 正文。
function processAssistantMarkdownBody(item: ConversationRenderItem | undefined): string | null {
  const block = item?.blocks.length === 1 ? item.blocks[0] : undefined;
  if (!block || item?.role !== "assistant" || block.kind !== "markdown") {
    return null;
  }
  const body = block.body.trim();
  return body ? body : null;
}

// assistantMarkdownProcessBodyCoveredByLater detects duplicate or superseded streamed process prose.
// assistantMarkdownProcessBodyCoveredByLater 检测重复或已被后续流式正文覆盖的过程正文。
function assistantMarkdownProcessBodyCoveredByLater(body: string, laterBody: string): boolean {
  const normalizedBody = normalizeProcessAssistantText(body);
  const normalizedLaterBody = normalizeProcessAssistantText(laterBody);
  if (!normalizedBody || !normalizedLaterBody) {
    return false;
  }
  if (normalizedBody === normalizedLaterBody) {
    return true;
  }
  return normalizedBody.length <= 240 && normalizedLaterBody.includes(normalizedBody);
}

// normalizeProcessAssistantText normalizes prose for duplicate checks without changing display content.
// normalizeProcessAssistantText 规范化正文以便去重检查，但不改变显示内容。
function normalizeProcessAssistantText(source: string): string {
  return source.replace(/\s+/g, " ").trim();
}

// conversationMessageUnixMillis parses the optional Unix-millis timestamp carried by one message.
// conversationMessageUnixMillis 解析单条消息携带的可选 Unix 毫秒时间戳。
function conversationMessageUnixMillis(message?: ConversationMessage): number | null {
  const raw = message?.createdAt?.trim();
  if (!raw) {
    return null;
  }
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

// formatFullConversationTimestamp renders a complete local timestamp for transcript metadata.
// formatFullConversationTimestamp 为转录元信息渲染完整本地时间戳。
function formatFullConversationTimestamp(message?: ConversationMessage): string | null {
  const timestamp = conversationMessageUnixMillis(message);
  if (!timestamp) {
    return null;
  }
  const date = new Date(timestamp);
  const pad = (value: number) => value.toString().padStart(2, "0");
  return `${date.getFullYear()}年${pad(date.getMonth() + 1)}月${pad(date.getDate())}日 ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

// turnStatsDurationMs reads the total turn duration from structured turn statistics.
// turnStatsDurationMs 从结构化轮次统计中读取总轮次耗时。
function turnStatsDurationMs(metadata: unknown): number | null {
  const stats = turnStatsObject(metadata);
  return numericMetadata(stats?.duration_ms ?? stats?.durationMs);
}

// formatWorkProcessDuration formats elapsed duration for the process header.
// formatWorkProcessDuration 格式化过程标题使用的处理耗时。
function formatWorkProcessDuration(durationMs: number | null): string {
  if (durationMs === null || !Number.isFinite(durationMs)) {
    return "";
  }
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}小时${minutes}分钟${seconds}秒`;
  }
  if (minutes > 0) {
    return `${minutes}分钟${seconds}秒`;
  }
  return `${seconds}秒`;
}

// latestThinkingBlockIdAfterLastUser returns the newest thinking block that belongs to the active user turn.
// latestThinkingBlockIdAfterLastUser 返回属于当前用户 turn 的最新思考块。
function latestThinkingBlockIdAfterLastUser(items: ConversationRenderItem[]): string | null {
  let latestThinkingBlockId: string | null = null;
  for (const item of items) {
    if (item.role === "user") {
      latestThinkingBlockId = null;
      continue;
    }
    for (const block of item.blocks) {
      if (block.kind === "thinking") {
        latestThinkingBlockId = block.id;
      }
    }
  }
  return latestThinkingBlockId;
}

// thinkingOverlayBlockSignature returns a stable content key for delayed popup retention.
// thinkingOverlayBlockSignature 返回用于延迟保留思考弹窗的稳定内容键。
function thinkingOverlayBlockSignature(block: ConversationBlock): string {
  return [
    block.id,
    block.title,
    block.body,
    block.summary,
    block.status ?? "",
  ].join("\u0000");
}

// isMergeableToolBlock returns whether a block belongs to one tool execution row.
// isMergeableToolBlock 返回某个块是否属于同一工具执行行。
function isMergeableToolBlock(block: ConversationBlock): boolean {
  if (block.kind === "tool-call" || block.kind === "tool-result" || block.kind === "command") {
    return blockHasToolExecutionEvidence(block);
  }
  return block.kind === "diff" && blockHasToolExecutionEvidence(block);
}

// blockHasToolExecutionEvidence checks that a special block really belongs to a tool execution.
// blockHasToolExecutionEvidence 检查特殊块确实属于一次工具执行。
function blockHasToolExecutionEvidence(block: ConversationBlock): boolean {
  if (block.toolEvidence || block.toolCallId || block.toolResultId || block.commandSessionId || block.taskSessionId || block.toolName) {
    return true;
  }
  const hostKind = metadataString(block.rawMetadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ]);
  return Boolean(hostKind && ["tool_error", "tool_result", "tool_call", "task_session", "command_session", "command_run", "change_set"].includes(hostKind));
}

// findToolMergeItem resolves an existing tool execution row using exact ids before the tool-name fallback.
// findToolMergeItem 优先使用精确标识，其次用工具名回退，解析已有工具执行行。
function findToolMergeItem(toolCallItems: Map<string, ConversationRenderItem>, block: ConversationBlock): ConversationRenderItem | undefined {
  for (const key of toolMergeKeysForBlock(block)) {
    const item = toolCallItems.get(key);
    if (item) {
      return item;
    }
  }
  return undefined;
}

// registerToolMergeItem indexes every known key for one visible tool execution row.
// registerToolMergeItem 为单条可见工具执行行登记所有已知合并键。
function registerToolMergeItem(toolCallItems: Map<string, ConversationRenderItem>, item: ConversationRenderItem): void {
  for (const block of item.blocks) {
    for (const key of toolMergeKeysForBlock(block)) {
      toolCallItems.set(key, item);
    }
  }
}

// toolMergeKeysForBlock resolves stable grouping keys from precise ids to the tool-name fallback.
// toolMergeKeysForBlock 按精确标识到工具名回退的顺序解析稳定分组键。
function toolMergeKeysForBlock(block: ConversationBlock): string[] {
  const keys = [
    block.toolCallId ? `call:${block.toolCallId}` : null,
    block.commandSessionId ? `command:${block.commandSessionId}` : null,
    block.toolResultId ? toolCallIdFromToolResultId(block.toolResultId) : null,
    block.toolName ? `name:${block.toolName}` : null,
  ];
  return keys.filter((key): key is string => Boolean(key));
}

// toolCallIdFromToolResultId recovers conventional tool-call ids from host-generated result ids.
// toolCallIdFromToolResultId 从宿主生成的结果标识中恢复约定形式的工具调用标识。
function toolCallIdFromToolResultId(toolResultId: string): string | null {
  const trimmed = toolResultId.trim();
  if (!trimmed.startsWith("tool-result-")) {
    return null;
  }
  const toolCallId = trimmed.slice("tool-result-".length).trim();
  return toolCallId ? `call:${toolCallId}` : null;
}

// mergeToolBlocks merges one tool result into an existing tool execution row.
// mergeToolBlocks 将一个工具返回合并到已有工具执行行中。
function mergeToolBlocks(blocks: ConversationBlock[], incoming: ConversationBlock): ConversationBlock[] {
  const withoutSameKind = blocks.filter((block) => block.kind !== incoming.kind);
  return [...withoutSameKind, incoming].sort((left, right) => toolBlockOrder(left.kind) - toolBlockOrder(right.kind));
}

// toolBlockOrder returns stable order for call/result display and detail assembly.
// toolBlockOrder 返回调用与返回展示及详情组装的稳定顺序。
function toolBlockOrder(kind: ConversationBlockKind): number {
  if (kind === "tool-call") return 0;
  if (kind === "command") return 1;
  if (kind === "tool-result" || kind === "diff") return 2;
  return 3;
}

// parseConversationBlocks converts legacy transcript text into renderable WebUI blocks.
// parseConversationBlocks 将旧转录文本转换为可渲染的 WebUI 块。
function parseConversationBlocks(message: ConversationMessage): ConversationBlock[] {
  if (message.role === "user") {
    return [conversationMarkdownBlock(message.id, 0, message.body)];
  }

  const structuredKind = structuredMessageBlockKind(message);
  if (structuredKind) {
    const attributes = structuredMessageAttributes(message);
    if (structuredKindLooksLikeToolBlock(structuredKind) && !structuredMessageHasToolEvidence(message, attributes)) {
      return [conversationMarkdownBlock(message.id, 0, message.body)];
    }
    return [conversationSpecialBlock(message.id, 0, structuredKind, message.body, attributes, message.metadata)];
  }

  const source = message.body.replace(/\r\n/g, "\n");
  const directiveBlocks = parseDirectiveConversationBlocks(message.id, source);
  if (directiveBlocks.some((block) => block.kind !== "markdown")) {
    return directiveBlocks;
  }

  return parseFallbackConversationBlocks(message.id, source);
}

// structuredMessageBlockKind maps service-provided record kinds to WebUI block kinds.
// structuredMessageBlockKind 将服务提供的记录类型映射到 WebUI 块类型。
function structuredMessageBlockKind(message: ConversationMessage): ConversationBlockKind | null {
  if (message.recordKind === "tool-call-output") return "tool-call";
  if (message.recordKind === "tool-result-output") {
    const hostKind = metadataString(message.metadata, ["kind", "host_result.kind", "hostResult.kind"]);
    if (hostKind === "command_session") return "command";
    if (hostKind === "change_set") return "diff";
    return "tool-result";
  }
  if (message.recordKind === "thinking-output") return "thinking";
  if (message.recordKind === "turn-stats") return "turn-stats";
  if (message.recordKind === "compaction-status") return "notice";
  return null;
}

// structuredMessageAttributes extracts render metadata from the service payload.
// structuredMessageAttributes 从服务载荷中提取渲染元数据。
function structuredMessageAttributes(message: ConversationMessage): Record<string, string> {
  const attributes: Record<string, string> = {};
  const toolName = metadataString(message.metadata, [
    "tool_name",
    "toolName",
    "host_result.tool_name",
    "hostResult.toolName",
    "host_result.payload.tool_name",
    "hostResult.payload.toolName",
    "payload.tool_name",
    "payload.toolName",
    "tool.name",
    "payload.tool",
    "host_result.payload.tool",
    "hostResult.payload.tool",
    "tool",
    "name",
  ]) ?? extractToolNameFromText(message.title ?? "");
  const toolCallId = metadataString(message.metadata, [
    "tool_call_id",
    "toolCallId",
    "host_result.tool_call_id",
    "hostResult.toolCallId",
    "host_result.payload.tool_call_id",
    "hostResult.payload.toolCallId",
    "payload.tool_call_id",
    "payload.toolCallId",
  ]);
  const toolResultId = metadataString(message.metadata, [
    "result.tool_result_id",
    "result.toolResultId",
    "tool_result_id",
    "toolResultId",
    "host_result.tool_result_id",
    "hostResult.toolResultId",
    "host_result.payload.tool_result_id",
    "hostResult.payload.toolResultId",
    "payload.tool_result_id",
    "payload.toolResultId",
  ]);
  const commandSessionId = metadataString(message.metadata, [
    "command_session_id",
    "commandSessionId",
    "host_result.command_session_id",
    "hostResult.commandSessionId",
    "host_result.payload.command_session_id",
    "hostResult.payload.commandSessionId",
    "payload.command_session_id",
    "payload.commandSessionId",
  ]);
  const taskSessionId = taskSessionIdFromToolMetadata(message.metadata);
  const taskSessionStatus = taskSessionId ? metadataString(message.metadata, [
      "tool_display_state",
      "toolDisplayState",
      "display_state",
      "displayState",
      "payload.execution_status",
      "payload.executionStatus",
      "host_result.payload.execution_status",
      "hostResult.payload.executionStatus",
    ]) : null;
  const explicitStatus = metadataString(message.metadata, [
      "result.status",
      "tool_display_state",
      "toolDisplayState",
      "display_state",
      "displayState",
      "payload.execution_status",
      "payload.executionStatus",
      "host_result.payload.execution_status",
      "hostResult.payload.executionStatus",
      "status",
      "state",
      "payload.status",
      "host_result.status",
      "hostResult.status",
    ]);
  const hasReturnedToolPayload = message.recordKind === "tool-result-output"
    && toolMetadataContainsReturnPayload(message.metadata, message.body);
  const fallbackStatus = hasReturnedToolPayload
    ? "completed"
    : explicitStatus
      ?? metadataToolFailureStatus(message.metadata)
      ?? turnStatsStatus(message.metadata);
  const status = taskSessionStatus ?? fallbackStatus;
  const summary = metadataString(message.metadata, [
      "result.summary",
      "preview",
      "summary",
    ]) ?? turnStatsSummary(message.metadata);
  if (message.title) attributes.title = message.title;
  if (message.recordKind) attributes.record_kind = message.recordKind;
  if (toolName) attributes.tool_name = toolName;
  if (toolCallId) attributes.tool_call_id = toolCallId;
  if (toolResultId) attributes.tool_result_id = toolResultId;
  if (commandSessionId) attributes.command_session_id = commandSessionId;
  if (taskSessionId) attributes.task_session_id = taskSessionId;
  if (status) attributes.status = status;
  if (summary) attributes.summary = summary;
  return attributes;
}

// taskSessionIdFromToolMetadata extracts the routed child task-session id from host task metadata.
// taskSessionIdFromToolMetadata 从宿主 task 元数据中提取路由用的子任务会话标识。
function taskSessionIdFromToolMetadata(metadata: unknown): string | null {
  return metadataString(metadata, [
    "payload.task_session_id",
    "payload.taskSessionId",
    "host_result.payload.task_session_id",
    "hostResult.payload.taskSessionId",
    "task_session_id",
    "taskSessionId",
  ]);
}

// metadataString reads a dotted path string from unknown JSON metadata.
// metadataString 从未知 JSON 元数据中读取点分路径字符串。
function metadataString(metadata: unknown, paths: string[]): string | null {
  for (const path of paths) {
    const value = metadataValue(metadata, [path]);
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

// metadataValue reads a dotted path value from unknown JSON metadata.
// metadataValue 从未知 JSON 元数据中读取点分路径值。
function metadataValue(metadata: unknown, paths: string[]): unknown {
  for (const path of paths) {
    const value = path.split(".").reduce<unknown>((current, segment) => {
      if (current && typeof current === "object" && segment in current) {
        return (current as Record<string, unknown>)[segment];
      }
      return undefined;
    }, metadata);
    if (value !== undefined && value !== null) {
      return value;
    }
  }
  return undefined;
}

// metadataToolFailureStatus detects host-side tool failures that are stored as structured metadata.
// metadataToolFailureStatus 识别以结构化元数据保存的宿主侧工具失败状态。
function metadataToolFailureStatus(metadata: unknown): string | null {
  const hostKind = metadataString(metadata, ["host_result.kind", "hostResult.kind", "kind"]);
  if (hostKind === "tool_error") {
    return "failed";
  }
  const payloadStatus = metadataString(metadata, ["payload.status", "status"]);
  return payloadStatus === "failed" || payloadStatus === "error" ? "failed" : null;
}

// turnStatsObject reads the structured turn-stats payload from transcript metadata.
// turnStatsObject 从转录元数据中读取结构化轮次统计载荷。
function turnStatsObject(metadata: unknown): Record<string, unknown> | null {
  if (!metadata || typeof metadata !== "object") {
    return null;
  }
  const value = (metadata as Record<string, unknown>).turn_stats ?? (metadata as Record<string, unknown>).turnStats;
  return value && typeof value === "object" ? value as Record<string, unknown> : null;
}

// turnStatsStatus reads the compact turn-stats terminal status.
// turnStatsStatus 读取紧凑轮次统计终态。
function turnStatsStatus(metadata: unknown): string | null {
  const stats = turnStatsObject(metadata);
  const status = stats?.status;
  return typeof status === "string" && status.trim() ? status.trim() : null;
}

// turnStatsSummary builds the same compact summary line used by the legacy GUI.
// turnStatsSummary 构建与旧 GUI 一致的紧凑摘要行。
function turnStatsSummary(metadata: unknown): string | null {
  const stats = turnStatsObject(metadata);
  if (!stats) {
    return null;
  }
  const parts = [
    `${numberMetadata(stats.provider_call_count ?? stats.providerCallCount)} 次调用`,
    `${numberMetadata(stats.tool_call_count ?? stats.toolCallCount)} 个工具`,
  ];
  const totalTokens = numberMetadata(stats.total_tokens ?? stats.totalTokens);
  if (totalTokens > 0) {
    parts.push(`${totalTokens} tokens`);
  }
  const cachedTokens = numberMetadata(stats.cached_input_tokens ?? stats.cachedInputTokens);
  if (cachedTokens > 0) {
    parts.push(`缓存命中 ${cachedTokens}`);
  }
  const speed = numericMetadata(stats.output_tokens_per_second ?? stats.outputTokensPerSecond);
  if (speed && Number.isFinite(speed) && speed > 0) {
    parts.push(`${speed.toFixed(1)}/s`);
  }
  return parts.join(" · ");
}

// numberMetadata reads a safe integer from unknown JSON values.
// numberMetadata 从未知 JSON 值中读取安全整数。
function numberMetadata(value: unknown): number {
  const numberValue = numericMetadata(value);
  return numberValue && Number.isFinite(numberValue) ? Math.max(0, Math.trunc(numberValue)) : 0;
}

// numericMetadata reads a number from unknown JSON values.
// numericMetadata 从未知 JSON 值中读取数字。
function numericMetadata(value: unknown): number | null {
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

// parseDirectiveConversationBlocks parses Vulcan triple-colon transcript directives.
// parseDirectiveConversationBlocks 解析 Vulcan 三冒号转录指令。
function parseDirectiveConversationBlocks(messageId: string, source: string): ConversationBlock[] {
  const blocks: ConversationBlock[] = [];
  const pattern = /^:::(\S+)([^\n]*)\n([\s\S]*?)\n:::\s*$/gm;
  let lastIndex = 0;
  let blockIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(source)) !== null) {
    const before = source.slice(lastIndex, match.index);
    if (before.trim()) {
      blockIndex = pushFallbackSegments(blocks, messageId, blockIndex, before.trim());
    }

    const directiveName = match[1] ?? "notice";
    const attributes = parseDirectiveAttributes(match[2] ?? "");
    const body = match[3] ?? "";
    const kind = directiveKinds[directiveName] ?? "notice";
    blocks.push(conversationSpecialBlock(messageId, blockIndex, kind, body, attributes));
    blockIndex += 1;
    lastIndex = pattern.lastIndex;
  }

  const after = source.slice(lastIndex);
  if (after.trim()) {
    pushFallbackSegments(blocks, messageId, blockIndex, after.trim());
  }

  return blocks;
}

// pushFallbackSegments appends Markdown and diff segments while preserving block indexes.
// pushFallbackSegments 追加 Markdown 与 diff 片段并保持块索引连续。
function pushFallbackSegments(
  blocks: ConversationBlock[],
  messageId: string,
  startIndex: number,
  source: string,
): number {
  let nextIndex = startIndex;
  for (const segment of splitFencedDiffSegments(source)) {
    if (!segment.body.trim()) {
      continue;
    }
    if (segment.kind === "diff") {
      blocks.push(conversationSpecialBlock(messageId, nextIndex, "diff", segment.body, {}));
    } else {
      blocks.push(conversationMarkdownBlock(messageId, nextIndex, segment.body));
    }
    nextIndex += 1;
  }
  return nextIndex;
}

// parseDirectiveAttributes parses loose key=value attributes from one directive header.
// parseDirectiveAttributes 从单条指令头中解析宽松的 key=value 属性。
function parseDirectiveAttributes(source: string): Record<string, string> {
  const attributes: Record<string, string> = {};
  const tokens = source.trim().match(/(?:[^\s"']+|"[^"]*"|'[^']*')+/g) ?? [];
  for (const token of tokens) {
    const [rawKey, ...rawValueParts] = token.split("=");
    const key = rawKey.trim();
    if (!key) {
      continue;
    }
    const value = rawValueParts.join("=").trim().replace(/^["']|["']$/g, "");
    attributes[key] = value || "true";
  }
  return attributes;
}

// parseFallbackConversationBlocks detects common legacy tool, command, and diff formats.
// parseFallbackConversationBlocks 识别常见旧版工具、命令与 diff 格式。
function parseFallbackConversationBlocks(messageId: string, source: string): ConversationBlock[] {
  const segments = splitFencedDiffSegments(source);
  if (segments.some((segment) => segment.kind === "diff")) {
    return segments.map((segment, index) => {
      if (segment.kind === "diff") {
        return conversationSpecialBlock(messageId, index, "diff", segment.body, {});
      }
      return conversationMarkdownBlock(messageId, index, segment.body);
    });
  }

  const toolKind = detectToolBlockKind(source);
  if (toolKind) {
    return [conversationSpecialBlock(messageId, 0, toolKind, source, detectToolAttributes(source))];
  }

  return [conversationMarkdownBlock(messageId, 0, source)];
}

// splitFencedDiffSegments separates Markdown around diff fences and raw git patches.
// splitFencedDiffSegments 将 diff 围栏与原始 git patch 周围的 Markdown 分离。
function splitFencedDiffSegments(source: string): Array<{ kind: "markdown" | "diff"; body: string }> {
  const segments: Array<{ kind: "markdown" | "diff"; body: string }> = [];
  const pattern = /```(?:diff|patch)\n([\s\S]*?)```|(^diff --git [\s\S]*?)(?=\n\n(?![+\- @\\])|\n#{1,6}\s|\n:::\S+|\s*$)/gm;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(source)) !== null) {
    const before = source.slice(lastIndex, match.index);
    if (before.trim()) {
      segments.push({ kind: "markdown", body: before.trim() });
    }
    segments.push({ kind: "diff", body: (match[1] ?? match[2] ?? "").trim() });
    lastIndex = pattern.lastIndex;
  }

  const after = source.slice(lastIndex);
  if (after.trim()) {
    segments.push({ kind: "markdown", body: after.trim() });
  }

  return segments.length > 0 ? segments : [{ kind: "markdown", body: source }];
}

// detectToolBlockKind classifies legacy text-only tool and runtime blocks.
// detectToolBlockKind 对旧版纯文本工具与运行时块进行分类。
function detectToolBlockKind(source: string): ConversationBlockKind | null {
  const normalized = source.toLowerCase();
  if (normalized.includes("turn stats") || normalized.includes("provider 调用") || normalized.includes("总 tokens")) {
    return "turn-stats";
  }
  if (hasLegacyToolCallBlockMarker(source)) {
    return "tool-call";
  }
  if (hasLegacyToolResultBlockMarker(source)) {
    return "tool-result";
  }
  if (hasLegacyCommandBlockMarker(source)) {
    return "command";
  }
  if (hasLegacyThinkingBlockMarker(source)) {
    return "thinking";
  }
  return null;
}

// hasLegacyToolCallBlockMarker detects explicit legacy tool-call transcript blocks.
// hasLegacyToolCallBlockMarker 检测明确的旧版工具调用转录块。
function hasLegacyToolCallBlockMarker(source: string): boolean {
  return /^#{1,6}\s*Tool Call\b/im.test(source)
    || /(?:^|\n)\s*tool_call_id["'`:\s=]+[A-Za-z0-9_.:-]+/i.test(source)
    || /(?:^|\n)\s*工具调用\s*[·:：]\s*[A-Za-z0-9_.:-]+/i.test(source)
    || /(?:^|\n)\s*调用\s+[A-Za-z0-9_.:-]+\s+工具\s*(?:进行中|运行中|已完成|失败)?\s*$/im.test(source);
}

// hasLegacyToolResultBlockMarker detects explicit legacy tool-result transcript blocks.
// hasLegacyToolResultBlockMarker 检测明确的旧版工具返回转录块。
function hasLegacyToolResultBlockMarker(source: string): boolean {
  return /^#{1,6}\s*Tool Result\b/im.test(source)
    || /^#{1,6}\s*Result ID\b/im.test(source)
    || /(?:^|\n)\s*tool_result_id["'`:\s=]+[A-Za-z0-9_.:-]+/i.test(source)
    || /(?:^|\n)\s*工具返回\s*[·:：]\s*[A-Za-z0-9_.:-]+/i.test(source);
}

// hasLegacyCommandBlockMarker detects explicit command-session transcript blocks without matching ordinary tool lists.
// hasLegacyCommandBlockMarker 检测明确的命令会话转录块，避免误匹配普通工具清单。
function hasLegacyCommandBlockMarker(source: string): boolean {
  const normalized = source.toLowerCase();
  return normalized.includes("command_session_id")
    || /^#{1,6}\s*(command session|命令会话)\b/im.test(source)
    || /^(command session|命令会话)\s*[:：]/im.test(source)
    || /(?:类型|kind|type)\s*[:：=]\s*(?:command_session|command session|命令会话)/i.test(source);
}

// hasLegacyThinkingBlockMarker detects explicit thinking transcript blocks without matching normal prose.
// hasLegacyThinkingBlockMarker 检测明确的思考转录块，避免误匹配普通正文。
function hasLegacyThinkingBlockMarker(source: string): boolean {
  return /^#{1,6}\s*(thinking|思考过程|思考)\b/im.test(source)
    || /^(thinking|思考过程|思考)\s*[:：]/im.test(source)
    || /(?:类型|kind|type)\s*[:：=]\s*(?:thinking|思考过程|思考)/i.test(source);
}

// detectToolAttributes extracts compact metadata from legacy tool text.
// detectToolAttributes 从旧版工具文本中提取紧凑元数据。
function detectToolAttributes(source: string): Record<string, string> {
  const attributes: Record<string, string> = {};
  const toolName = extractToolNameFromText(source);
  const toolCallId = source.match(/tool_call_id["'`:\s=]+([A-Za-z0-9_.:-]+)/i)?.[1];
  const toolResultId = extractToolResultIdFromText(source);
  const commandSessionId = source.match(/command_session_id["'`:\s=]+([A-Za-z0-9_.:-]+)/i)?.[1];
  const status = extractToolStatusFromText(source);
  const hasToolMarker = hasLegacyToolCallBlockMarker(source) || hasLegacyToolResultBlockMarker(source);
  if (toolName) attributes.tool_name = toolName;
  if (toolCallId) attributes.tool_call_id = toolCallId;
  if (toolResultId) attributes.tool_result_id = toolResultId;
  if (commandSessionId) attributes.command_session_id = commandSessionId;
  if (status) attributes.status = status;
  if (hasToolMarker) attributes.tool_evidence = "true";
  return attributes;
}

// extractToolResultIdFromText recovers host result ids from legacy Markdown or compact plain text.
// extractToolResultIdFromText 从旧版 Markdown 或紧凑纯文本中恢复宿主结果标识。
function extractToolResultIdFromText(source: string): string | null {
  const normalized = source.replace(/\r\n/g, "\n");
  const patterns = [
    /tool_result_id["'`:\s=]+([A-Za-z0-9_.:-]+)/i,
    /##\s*Result ID\s*\n+\s*`([^`]+)`/im,
    /(?:^|\n)\s*Result ID\s*\n+\s*`?([A-Za-z0-9_.:-]+)`?/im,
    /(?:^|\n)\s*Result ID\s+([A-Za-z0-9_.:-]+)/im,
  ];
  for (const pattern of patterns) {
    const value = normalized.match(pattern)?.[1]?.trim();
    if (value) {
      return value;
    }
  }
  return null;
}

// extractToolNameFromText recovers a tool name from legacy localized titles and Markdown bodies.
// extractToolNameFromText 从旧版本地化标题与 Markdown 正文中恢复工具名称。
function extractToolNameFromText(source: string): string | null {
  const normalized = source.replace(/\r\n/g, "\n");
  const patterns = [
    /##\s*Tool Call\s*\n+\s*`([^`]+)`/im,
    /##\s*Tool Result\s*\n+\s*`([^`]+)`/im,
    /tool_name["'`:\s=]+([A-Za-z0-9_.:-]+)/i,
    /"tool_name"\s*:\s*"([^"]+)"/i,
    /"toolName"\s*:\s*"([^"]+)"/i,
    /工具调用\s*[·:：]\s*[A-Za-z0-9_.:-]+\s*[·:：]\s*([A-Za-z0-9_.:-]+)/,
    /(?:工具调用|工具返回)\s*[·:：]?\s*([A-Za-z0-9_.:-]+)/,
    /调用\s+([A-Za-z0-9_.:-]+)\s+工具/,
    /Tool (?:Call|Result)\s+([A-Za-z0-9_.:-]+)/i,
    /Call\s+([A-Za-z0-9_.:-]+)/i,
  ];
  for (const pattern of patterns) {
    const value = normalized.match(pattern)?.[1]?.trim();
    if (value && !genericToolNameLabels.has(value)) {
      return value;
    }
  }
  return null;
}

// extractToolStatusFromText recovers a display state from legacy tool output text.
// extractToolStatusFromText 从旧版工具输出文本中恢复显示状态。
function extractToolStatusFromText(source: string): string | null {
  const normalized = source.toLowerCase();
  if (/失败|报错|错误|failed|error|panic|exit code\s*-\d|退出码\s*-\d/i.test(source)) {
    return "failed";
  }
  if (/已完成|完成|completed|success|succeeded/i.test(source)) {
    return "completed";
  }
  if (/运行中|进行中|running|pending|in progress/i.test(normalized)) {
    return "running";
  }
  return null;
}

// genericToolNameLabels lists localized labels that are headings rather than actual tool names.
// genericToolNameLabels 列出只是标题而不是实际工具名的本地化标签。
const genericToolNameLabels = new Set(["工具调用", "工具返回", "Tool Call", "Tool Result", "Raw Tool Result"]);

// conversationMarkdownBlock creates a standard Markdown block descriptor.
// conversationMarkdownBlock 创建标准 Markdown 块描述。
function conversationMarkdownBlock(messageId: string, index: number, body: string): ConversationBlock {
  return {
    id: `${messageId}-markdown-${index}`,
    kind: "markdown",
    title: "Markdown",
    body,
    summary: compactTextPreview(body),
  };
}

// conversationSpecialBlock creates a semantic block with compact summary metadata.
// conversationSpecialBlock 创建带紧凑摘要元数据的语义块。
function conversationSpecialBlock(
  messageId: string,
  index: number,
  kind: ConversationBlockKind,
  body: string,
  attributes: Record<string, string>,
  rawMetadata?: unknown,
): ConversationBlock {
  const toolName = attributes.tool_name ?? attributes.name ?? extractToolNameFromText(attributes.title ?? "");
  const title = attributes.title ?? blockTitle(kind, toolName);
  const mediaAttachments = generatedMediaAttachmentsFromMetadata(rawMetadata);
  const toolEvidence = conversationBlockHasToolEvidence(kind, attributes, rawMetadata);
  const compactionSectionId = metadataString(rawMetadata, ["context_compaction_section_id"]);
  const compactionRole = metadataString(rawMetadata, ["context_compaction_role"]);
  const compactionState = metadataString(rawMetadata, ["context_compaction_state"]);
  const compactionTrigger = metadataString(rawMetadata, ["context_compaction_trigger"]);
  const compactionBudgetNote = metadataString(rawMetadata, ["context_compaction_budget_note"]);
  const status = mediaAttachments.length > 0
    ? "completed"
    : normalizeToolDisplayState(attributes.status ?? attributes.state ?? extractToolStatusFromText(body));
  return {
    id: `${messageId}-${kind}-${index}`,
    kind,
    title,
    body: body.trim(),
    summary: attributes.summary ?? compactTextPreview(body),
    status,
    toolName,
    toolEvidence,
    toolCallId: attributes.tool_call_id,
    toolResultId: attributes.tool_result_id,
    commandSessionId: attributes.command_session_id,
    taskSessionId: attributes.task_session_id,
    rawMetadata,
    mediaAttachments: mediaAttachments.length > 0 ? mediaAttachments : undefined,
    compactionSectionId: compactionSectionId ?? undefined,
    compactionRole: compactionRole ?? undefined,
    compactionState: compactionState ?? undefined,
    compactionTrigger: compactionTrigger ?? undefined,
    compactionBudgetNote: compactionBudgetNote ?? undefined,
  };
}

// normalizeToolDisplayState maps provider and host state words into WebUI display states.
// normalizeToolDisplayState 将 Provider 与宿主状态词映射为 WebUI 展示状态。
function normalizeToolDisplayState(status?: string | null): string | undefined {
  if (!status?.trim()) {
    return undefined;
  }
  const normalized = status.trim().toLowerCase();
  if (normalized === "failed" || normalized === "error" || normalized === "errored" || normalized === "failure") {
    return "failed";
  }
  if (normalized === "completed" || normalized === "complete" || normalized === "success" || normalized === "succeeded" || normalized === "ready") {
    return "completed";
  }
  if (normalized === "stopped" || normalized === "cancelled" || normalized === "canceled" || normalized === "aborted") {
    return "stopped";
  }
  if (normalized === "running" || normalized === "pending" || normalized === "preview" || normalized === "in_progress") {
    return "running";
  }
  if (status.includes("失败") || status.includes("错误")) {
    return "failed";
  }
  if (status.includes("完成") || status.includes("成功")) {
    return "completed";
  }
  if (status.includes("运行") || status.includes("进行")) {
    return "running";
  }
  return status.trim();
}

// blockTitle returns a localized title for one semantic transcript block.
// blockTitle 返回单个语义转录块的本地化标题。
function blockTitle(kind: ConversationBlockKind, toolName?: string): string {
  if (kind === "tool-call") return toolName ? `工具调用 · ${toolName}` : "工具调用";
  if (kind === "tool-result") return toolName ? `工具返回 · ${toolName}` : "工具返回";
  if (kind === "command") return "命令会话";
  if (kind === "diff") return "Git Diff";
  if (kind === "thinking") return "思考过程";
  if (kind === "turn-stats") return "Turn Stats";
  if (kind === "notice") return "运行提示";
  return "Markdown";
}

// compactTextPreview returns one short single-line preview for cards and modal headers.
// compactTextPreview 返回用于卡片与弹窗标题区的单行短预览。
function compactTextPreview(source: string): string {
  const text = source
    .replace(/```[\s\S]*?```/g, "代码块")
    .replace(/[#>*_`|~-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!text) {
    return "无正文内容";
  }
  return text.length > 140 ? `${text.slice(0, 140)}...` : text;
}

// copyText writes transcript detail text to the system clipboard when available.
// copyText 在可用时将转录详情文本写入系统剪贴板。
async function copyText(text: string): Promise<void> {
  await navigator.clipboard?.writeText(text);
}

// BrandHeader renders the product brand area.
// BrandHeader 渲染产品品牌区域。
function BrandHeader(props: {
  // Whether the brand block should behave as a native Tauri drag region.
  // 品牌块是否应作为 Tauri 原生拖动区域。
  dragRegion?: boolean;
}) {
  // t translates shell labels used by the brand block.
  // t 翻译品牌区域使用的外壳标签。
  const { t } = useI18n();
  return (
    <div className="brand" data-tauri-drag-region={props.dragRegion ? "" : undefined}>
      <div className="brand-mark">V</div>
      <div>
        <strong>Vulcan Code</strong>
        <span>{t("app.subtitle")}</span>
      </div>
    </div>
  );
}

// FeatureNav renders the primary workbench navigation.
// FeatureNav 渲染工作台主导航。
function FeatureNav(props: { areas: FeatureAreaSummary[]; activeArea: MainAreaId; onSelect: (area: MainAreaId) => void }) {
  // t translates navigation accessibility labels.
  // t 翻译导航无障碍标签。
  const { t } = useI18n();
  // scrollRef points to the horizontal navigation viewport.
  // scrollRef 指向横向导航视口。
  const scrollRef = React.useRef<HTMLElement | null>(null);
  // navOverflow stores whether each arrow can move the navigation viewport.
  // navOverflow 保存左右箭头是否还能移动导航视口。
  const [navOverflow, setNavOverflow] = React.useState({ left: false, right: false });

  // updateNavOverflow refreshes arrow enabled state from the scroll viewport.
  // updateNavOverflow 根据滚动视口刷新箭头启用状态。
  const updateNavOverflow = React.useCallback(() => {
    const element = scrollRef.current;
    if (!element) {
      setNavOverflow({ left: false, right: false });
      return;
    }
    const maxScrollLeft = element.scrollWidth - element.clientWidth;
    setNavOverflow({
      left: element.scrollLeft > 2,
      right: element.scrollLeft < maxScrollLeft - 2,
    });
  }, []);

  // scrollNav moves the navigation viewport by one compact page.
  // scrollNav 按一个紧凑页宽移动导航视口。
  const scrollNav = React.useCallback((direction: -1 | 1) => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    element.scrollBy({ left: direction * Math.max(120, element.clientWidth * 0.7), behavior: "smooth" });
    window.setTimeout(updateNavOverflow, 180);
  }, [updateNavOverflow]);

  React.useEffect(() => {
    updateNavOverflow();
    window.addEventListener("resize", updateNavOverflow);
    return () => window.removeEventListener("resize", updateNavOverflow);
  }, [props.areas.length, updateNavOverflow]);

  return (
    <div className="feature-nav-shell">
      {navOverflow.left ? (
        <Button
          aria-label="Scroll main features left"
          className="feature-nav-arrow"
          onClick={() => scrollNav(-1)}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <ChevronLeft size={15} />
        </Button>
      ) : null}
      <nav className="feature-nav" aria-label={t("sidebar.overview")} onScroll={updateNavOverflow} ref={scrollRef}>
        {props.areas.map((area) => (
          <Button
            className="feature-button"
            key={area.id}
            onClick={() => props.onSelect(area.id)}
            size="sm"
            title={area.description}
            type="button"
            variant={props.activeArea === area.id ? "secondary" : "ghost"}
          >
            {areaIconMap[area.id]}
            <span>{area.label}</span>
          </Button>
        ))}
      </nav>
      {navOverflow.right ? (
        <Button
          aria-label="Scroll main features right"
          className="feature-nav-arrow"
          onClick={() => scrollNav(1)}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <ChevronRight size={15} />
        </Button>
      ) : null}
    </div>
  );
}

// GlobalActions renders app-wide controls in the primary menu bar.
// GlobalActions 渲染主菜单栏中的全局控制。
function GlobalActions(props: {
  // Active top-level area used to lock the settings button visual state.
  // 当前顶层区域，用于锁定设置按钮视觉状态。
  activeArea: MainAreaId;
  // Startup snapshot used for the compact online indicator.
  // 紧凑在线状态指示使用的启动快照。
  startup: StartupSnapshot;
  // Current shadcn theme mode.
  // 当前 shadcn 主题模式。
  theme: ThemeMode;
  // Current UI language.
  // 当前 UI 语言。
  locale: LocaleCode;
  // Updates the current UI language.
  // 更新当前 UI 语言。
  onChangeLocale: (locale: LocaleCode) => void;
  // Toggles between light and dark theme.
  // 在亮色与暗色主题之间切换。
  onToggleTheme: () => void;
  // Opens the settings feature area.
  // 打开设置功能区域。
  onOpenSettings: () => void;
}) {
  // t translates global action labels.
  // t 翻译全局操作标签。
  const { t } = useI18n();
  // isPreviewMode detects fallback browser data instead of a live service bridge.
  // isPreviewMode 检测当前是否为浏览器回退预览数据而非真实服务桥接。
  const isPreviewMode = props.startup.phase === "browser-preview";
  // statusLabel translates internal startup phases into compact user-facing labels.
  // statusLabel 将内部启动阶段转换为面向用户的紧凑显示文案。
  const statusLabel = isPreviewMode ? t("global.preview") : props.startup.ready ? t("global.online") : props.startup.phase;
  // statusKind keeps preview visually distinct from a live ready service.
  // statusKind 让预览状态与真实在线服务在视觉上保持区分。
  const statusKind = isPreviewMode ? "warning" : props.startup.ready ? "ready" : "running";

  return (
    <div className="global-actions" data-no-drag>
      <StatusPill status={statusKind} label={statusLabel} />
      <Button
        aria-label={props.theme === "dark" ? t("global.toggleLight") : t("global.toggleDark")}
        className="theme-toggle"
        onClick={props.onToggleTheme}
        size="icon"
        type="button"
        variant="outline"
      >
        {props.theme === "dark" ? <Moon size={16} /> : <Sun size={16} />}
      </Button>
      <Select value={props.locale} onValueChange={(value) => props.onChangeLocale(value as LocaleCode)}>
        <SelectTrigger aria-label={t("global.language")} className="language-select-trigger">
          <span>{props.locale === "zh-CN" ? "中" : "EN"}</span>
        </SelectTrigger>
        <SelectContent align="end" position="popper">
          <SelectItem value="zh-CN">中</SelectItem>
          <SelectItem value="en-US">EN</SelectItem>
        </SelectContent>
      </Select>
      <Button
        aria-pressed={props.activeArea === "settings"}
        aria-label={t("global.openSettings")}
        className="icon-button"
        onClick={props.onOpenSettings}
        size="icon"
        type="button"
        variant={props.activeArea === "settings" ? "secondary" : "outline"}
      >
        <Settings size={18} />
      </Button>
    </div>
  );
}

// ConsoleShell renders the shadcn dashboard-like fused sidebar and inset console area.
// ConsoleShell 渲染类似 shadcn dashboard 的融合侧栏与 inset 控制台区域。
function ConsoleShell(props: {
  startup: StartupSnapshot;
  workbench: WorkbenchSnapshot;
  selectedDraftConversation: DraftConversationSelection | null;
  selectedConversation?: ConversationSummary;
  selectedConversationId: string | null;
  loadingConversationId: string | null;
  conversationLoadError: string | null;
  pinnedConversationIds: Set<string>;
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  onCreateConversation: (project: ProjectSummary) => void;
  onCreateProject: () => void;
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onReorderProjects: (projectIds: string[]) => void;
  onSelectConversation: (conversationId: string) => void;
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  onStopConversationRun: () => Promise<boolean>;
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  onCloseSubagentPanel: () => Promise<void>;
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
}) {
  return (
    <SidebarProvider className="console-shell">
      <ConsoleShellContent {...props} />
    </SidebarProvider>
  );
}

// generatedMediaAttachmentsFromMetadata extracts host-generated media outputs from nested or flattened tool metadata.
// generatedMediaAttachmentsFromMetadata 从嵌套或扁平工具元数据中提取宿主生成媒体输出。
function generatedMediaAttachmentsFromMetadata(metadata: unknown): GeneratedMediaAttachment[] {
  const hostResult = resolvedHostResult(metadata);
  if (hostResult?.kind === "generated_attachments") {
    return generatedMediaAttachmentsFromPayload(hostResult.payload);
  }
  const directAttachments = generatedMediaAttachmentsFromPayload(metadata);
  if (directAttachments.length > 0) {
    return directAttachments;
  }
  return generatedMediaAttachmentsFromPayload(metadataValue(metadata, ["display"]));
}

// generatedMediaAttachmentsFromToolDetailPayload extracts media outputs from the hidden full tool detail row.
// generatedMediaAttachmentsFromToolDetailPayload 从隐藏完整工具详情行中提取媒体输出。
function generatedMediaAttachmentsFromToolDetailPayload(payload: ToolDetailPayload): GeneratedMediaAttachment[] {
  const metadataAttachments = generatedMediaAttachmentsFromMetadata(payload.metadata);
  if (metadataAttachments.length > 0) {
    return metadataAttachments;
  }
  const parsedBody = parseJsonObject(payload.body);
  return generatedMediaAttachmentsFromPayload(parsedBody);
}

// generatedMediaAttachmentsFromPayload maps a generated-capability JSON payload to WebUI attachments.
// generatedMediaAttachmentsFromPayload 将生成能力 JSON 载荷映射为 WebUI 附件。
function generatedMediaAttachmentsFromPayload(payload: unknown): GeneratedMediaAttachment[] {
  const object = objectValue(payload);
  const outputs = Array.isArray(object?.outputs) ? object.outputs : [];
  return outputs
    .map((output): GeneratedMediaAttachment | null => {
      if (!output || typeof output !== "object") {
        return null;
      }
      const object = output as Record<string, unknown>;
      const rawKind = typeof object.kind === "string" ? object.kind.trim().toLowerCase() : "";
      const kind = normalizeGeneratedMediaKind(rawKind);
      const localPath = stringValue(object.local_path) || stringValue(object.source_path);
      if (!kind || !localPath) {
        return null;
      }
      return {
        kind,
        fileName: stringValue(object.file_name) || localPath.split(/[\\/]/).pop() || localPath,
        localPath,
        previewLocalPath: stringValue(object.preview_local_path) || undefined,
        mimeType: stringValue(object.mime_type) || undefined,
      };
    })
    .filter((output): output is GeneratedMediaAttachment => output !== null);
}

// parseJsonObject parses one JSON object string without treating plain tool text as structured data.
// parseJsonObject 解析单个 JSON 对象字符串，并避免把普通工具文本当成结构化数据。
function parseJsonObject(source: string): Record<string, unknown> | null {
  const trimmed = source.trim();
  if (!trimmed.startsWith("{") || !trimmed.endsWith("}")) {
    return null;
  }
  try {
    return objectValue(JSON.parse(trimmed));
  } catch {
    return null;
  }
}

// resolvedHostResult returns a normalized generated/tool host-result object from known runtime metadata shapes.
// resolvedHostResult 从已知运行时元数据形态中返回规范化的工具宿主结果对象。
function resolvedHostResult(metadata: unknown): { kind?: string; payload?: Record<string, unknown> } | null {
  const candidate = metadataValue(metadata, ["host_result", "hostResult"]) ?? metadata;
  if (!candidate || typeof candidate !== "object") {
    return null;
  }
  const object = candidate as Record<string, unknown>;
  const kind = stringValue(object.kind);
  const payload = object.payload && typeof object.payload === "object"
    ? object.payload as Record<string, unknown>
    : undefined;
  if (!kind || !payload) {
    return null;
  }
  return { kind, payload };
}

// normalizeGeneratedMediaKind maps stored attachment kind labels to WebUI media controls.
// normalizeGeneratedMediaKind 将已存储附件类型标签映射为 WebUI 媒体控件。
function normalizeGeneratedMediaKind(kind: string): GeneratedMediaKind | null {
  if (kind === "image") return "image";
  if (kind === "audio") return "audio";
  if (kind === "video") return "video";
  if (kind === "pdf" || kind === "document") return "document";
  return null;
}

// stringValue returns one trimmed string from an unknown JSON value.
// stringValue 从未知 JSON 值中返回一条清理后的字符串。
function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

// mediaSourceUrl converts local media paths into URLs usable by either Tauri or browser preview mode.
// mediaSourceUrl 将本地媒体路径转换为 Tauri 或浏览器预览模式可用的 URL。
function mediaSourceUrl(path: string): string {
  return localMediaSourceUrl(path);
}

// normalizeMarkdownImageSource converts generated local image paths into renderable media URLs.
// normalizeMarkdownImageSource 将 Markdown 中的生成本地图片路径转换为可渲染媒体 URL。
function normalizeMarkdownImageSource(src: string | undefined, workspacePath?: string | null): string | undefined {
  const value = src?.trim();
  if (!value) {
    return src;
  }
  const unwrapped = value.startsWith("<") && value.endsWith(">") ? value.slice(1, -1).trim() : value;
  if (/^vulcan-media:/i.test(unwrapped) || /^https?:\/\/vulcan-media\.localhost(?:\/|$)/i.test(unwrapped)) {
    return mediaSourceUrl(unwrapped);
  }
  if (/^(https?:|data:|blob:|asset:)/i.test(unwrapped)) {
    return unwrapped;
  }
  if (/^file:/i.test(unwrapped)) {
    return mediaSourceUrl(unwrapped);
  }
  if (looksLikeLocalFilesystemPath(unwrapped)) {
    return mediaSourceUrl(unwrapped);
  }
  const relativeSource = resolveWorkspaceRelativeMarkdownImageSource(unwrapped, workspacePath);
  if (relativeSource) {
    return relativeSource;
  }
  return src;
}

// structuredKindLooksLikeToolBlock returns whether a structured kind needs tool evidence before special rendering.
// structuredKindLooksLikeToolBlock 返回结构化类型是否需要工具证据才能按特殊块渲染。
function structuredKindLooksLikeToolBlock(kind: ConversationBlockKind): boolean {
  return kind === "tool-call" || kind === "tool-result" || kind === "command" || kind === "diff";
}

// structuredMessageHasToolEvidence prevents assistant prose from being misrendered as a failed tool row.
// structuredMessageHasToolEvidence 防止助手正文被误渲染为失败工具行。
function structuredMessageHasToolEvidence(message: ConversationMessage, attributes: Record<string, string>): boolean {
  if (attributes.tool_evidence === "true" || attributes.tool_name || attributes.tool_call_id || attributes.tool_result_id || attributes.command_session_id || attributes.task_session_id) {
    return true;
  }
  const hostKind = metadataString(message.metadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ]);
  if (hostKind && ["tool_error", "tool_result", "tool_call", "task_session", "command_session", "command_run", "change_set"].includes(hostKind)) {
    return true;
  }
  const payloadTool = metadataString(message.metadata, [
    "payload.tool",
    "host_result.payload.tool",
    "hostResult.payload.tool",
    "payload.tool_name",
    "host_result.payload.tool_name",
    "hostResult.payload.toolName",
  ]);
  if (payloadTool) {
    return true;
  }
  const title = message.title ?? "";
  return /调用\s+\S+\s+工具|工具调用|工具返回|Tool Call|Tool Result|Command Session|命令会话/i.test(title);
}

// markdownImageAttachment converts one rendered Markdown image URL into a preview attachment.
// markdownImageAttachment 将单个已渲染 Markdown 图片 URL 转换为预览附件。
function markdownImageAttachment(src: string | undefined, alt: string | undefined): GeneratedMediaAttachment | null {
  const value = src?.trim();
  if (!value) {
    return null;
  }
  const cleanValue = value.startsWith("<") && value.endsWith(">") ? value.slice(1, -1).trim() : value;
  const fileName = markdownImageFileName(cleanValue, alt);
  return {
    kind: "image",
    fileName,
    localPath: cleanValue,
    mimeType: markdownImageMimeType(cleanValue),
  };
}

// markdownImageFileName returns a stable display name for Markdown image previews.
// markdownImageFileName 为 Markdown 图片预览返回稳定展示名称。
function markdownImageFileName(src: string, alt: string | undefined): string {
  const cleanAlt = alt?.replace(/\|\s*\d+\s*$/u, "").trim();
  if (cleanAlt) {
    return cleanAlt;
  }
  const withoutQuery = src.split(/[?#]/u, 1)[0] ?? src;
  const normalized = withoutQuery.replace(/\\/g, "/");
  const tail = normalized.split("/").filter(Boolean).pop();
  return tail ? decodeURIComponentSafe(tail) : "markdown-image";
}

// markdownImageMimeType infers a lightweight MIME type from a Markdown image URL.
// markdownImageMimeType 从 Markdown 图片 URL 推断轻量 MIME 类型。
function markdownImageMimeType(src: string): string | undefined {
  if (/^data:image\/([^;,]+)/i.test(src)) {
    const match = src.match(/^data:(image\/[^;,]+)/i);
    return match?.[1];
  }
  const clean = src.split(/[?#]/u, 1)[0]?.toLowerCase() ?? "";
  if (clean.endsWith(".png")) return "image/png";
  if (clean.endsWith(".jpg") || clean.endsWith(".jpeg")) return "image/jpeg";
  if (clean.endsWith(".webp")) return "image/webp";
  if (clean.endsWith(".gif")) return "image/gif";
  if (clean.endsWith(".svg")) return "image/svg+xml";
  return undefined;
}

// decodeURIComponentSafe decodes URL text while tolerating malformed percent escapes.
// decodeURIComponentSafe 在容忍非法百分号转义的情况下解码 URL 文本。
function decodeURIComponentSafe(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

// normalizeMarkdownLocalImageLinks rewrites local image links before Markdown sanitization removes them.
// normalizeMarkdownLocalImageLinks 在 Markdown 清理移除本地路径前重写本地图片链接。
function normalizeMarkdownLocalImageLinks(content: string, workspacePath?: string | null): string {
  return content.replace(/!\[([^\]\n]*)\]\(([^)\n]+)\)/g, (full, alt: string, destination: string, offset: number) => {
    const normalized = normalizeMarkdownImageDestination(destination, workspacePath);
    const width = markdownImageWidthFromAlt(alt);
    const cleanAlt = width ? alt.replace(/\|\s*\d+\s*$/u, "").trim() : alt;
    const nextDestination = normalized ?? destination.trim();
    const widthTitle = width ? ` "width=${width}"` : "";
    const imageMarkdown = normalized || width ? `![${cleanAlt}](${nextDestination}${widthTitle})` : full;
    return `${markdownImageLeadingBlockPrefix(content, offset)}${imageMarkdown}`;
  });
}

// markdownImageLeadingBlockPrefix returns the newline prefix needed to render an image as its own block.
// markdownImageLeadingBlockPrefix 返回将图片渲染为独立块所需的换行前缀。
function markdownImageLeadingBlockPrefix(content: string, offset: number): string {
  const previousContent = content.slice(0, Math.max(0, offset));
  if (previousContent.trim().length === 0) {
    return "";
  }
  if (/(?:\r?\n)[ \t]*(?:\r?\n)[ \t]*$/u.test(previousContent)) {
    return "";
  }
  const lineStart = Math.max(content.lastIndexOf("\n", offset - 1), content.lastIndexOf("\r", offset - 1)) + 1;
  return content.slice(lineStart, offset).trim().length > 0 ? "\n\n" : "\n";
}

// normalizeMarkdownImageDestination extracts and rewrites one Markdown image destination.
// normalizeMarkdownImageDestination 提取并重写单个 Markdown 图片目标。
function normalizeMarkdownImageDestination(destination: string, workspacePath?: string | null): string | null {
  const trimmed = destination.trim();
  if (!trimmed) {
    return null;
  }
  const sizedDestination = markdownImageSizedDestination(trimmed);
  if (sizedDestination) {
    const normalized = normalizeMarkdownImageDestination(sizedDestination.path, workspacePath);
    const nextPath = normalized ?? sizedDestination.path.trim();
    return `${nextPath} "width=${sizedDestination.width}"`;
  }
  const titleMatch = trimmed.match(/^(.+?)(\s+["'][^"']*["'])$/);
  const rawPath = (titleMatch ? titleMatch[1] : trimmed).trim();
  const title = titleMatch?.[2] ?? "";
  const unwrapped = rawPath.startsWith("<") && rawPath.endsWith(">") ? rawPath.slice(1, -1).trim() : rawPath;
  const normalized = normalizeMarkdownImageSource(unwrapped, workspacePath);
  if (!normalized || normalized === unwrapped) {
    return null;
  }
  return `${normalized}${title}`;
}

// markdownImageSizedDestination parses Markdown image destinations that carry an explicit width suffix.
// markdownImageSizedDestination 解析携带显式宽度后缀的 Markdown 图片目标。
function markdownImageSizedDestination(destination: string): { path: string; width: number } | null {
  const match = destination.match(/^(.+?)\s+=([1-9]\d{0,4})(?:x\d*)?$/u);
  if (!match) {
    return null;
  }
  return { path: match[1].trim(), width: clampMarkdownImageWidth(Number(match[2])) };
}

// markdownImageWidthFromAlt parses Obsidian-style image alt width hints.
// markdownImageWidthFromAlt 解析 Obsidian 风格的图片 alt 宽度提示。
function markdownImageWidthFromAlt(alt: string): number | null {
  const match = alt.match(/\|\s*([1-9]\d{0,4})\s*$/u);
  return match ? clampMarkdownImageWidth(Number(match[1])) : null;
}

// markdownImageWidthFromTitle parses explicit width hints carried by Markdown image title text.
// markdownImageWidthFromTitle 解析 Markdown 图片 title 文本携带的显式宽度提示。
function markdownImageWidthFromTitle(title: string | undefined): number | null {
  if (!title) {
    return null;
  }
  const match = title.match(/(?:^|\s)(?:width|w)\s*=\s*([1-9]\d{0,4})(?:px)?(?:\s|$)/iu);
  return match ? clampMarkdownImageWidth(Number(match[1])) : null;
}

// clampMarkdownImageWidth caps explicit Markdown image widths to avoid oversized transcript layouts.
// clampMarkdownImageWidth 限制显式 Markdown 图片宽度，避免转录布局被过大图片撑开。
function clampMarkdownImageWidth(width: number): number {
  return Math.min(Math.max(Math.round(width), 1), 800);
}

// looksLikeLocalFilesystemPath detects Windows and Unix filesystem paths carried in Markdown.
// looksLikeLocalFilesystemPath 检测 Markdown 中携带的 Windows 与 Unix 文件系统路径。
function looksLikeLocalFilesystemPath(value: string): boolean {
  const normalized = value.replace(/\\/g, "/");
  return (
    /^[a-zA-Z]:[\\/]/.test(value)
    || value.startsWith("\\\\")
    || value.startsWith("/")
    || normalized.startsWith(".vulcan/code/attachments/")
    || normalized.startsWith("./.vulcan/code/attachments/")
  );
}

// resolveWorkspaceRelativeMarkdownImageSource converts a relative Markdown image path into a project-root media URL.
// resolveWorkspaceRelativeMarkdownImageSource 将 Markdown 相对图片路径转换为项目根目录下的媒体 URL。
function resolveWorkspaceRelativeMarkdownImageSource(value: string, workspacePath?: string | null): string | null {
  const workspaceRoot = workspacePath?.trim();
  if (!workspaceRoot || !looksLikeRelativeMarkdownImagePath(value)) {
    return null;
  }
  const { path, suffix } = splitMarkdownImagePathSuffix(value);
  const relativePath = normalizeRelativeMarkdownImagePath(path);
  if (!relativePath) {
    return null;
  }
  return `${mediaSourceUrl(joinWorkspaceRelativePath(workspaceRoot, relativePath))}${suffix}`;
}

// looksLikeRelativeMarkdownImagePath detects image-only relative links that can be resolved from the workspace root.
// looksLikeRelativeMarkdownImagePath 检测可从工作区根目录解析的图片型相对链接。
function looksLikeRelativeMarkdownImagePath(value: string): boolean {
  const { path } = splitMarkdownImagePathSuffix(value.trim());
  if (!path || looksLikeLocalFilesystemPath(path) || path.startsWith("//")) {
    return false;
  }
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/u.test(path)) {
    return false;
  }
  const normalized = path.replace(/\\/g, "/");
  if (normalized.split("/").some((segment) => segment === "..")) {
    return false;
  }
  return /\.(?:png|jpe?g|webp|gif|bmp|svg)$/iu.test(normalized);
}

// splitMarkdownImagePathSuffix separates a path from query and hash suffixes.
// splitMarkdownImagePathSuffix 将路径与查询、哈希后缀拆分。
function splitMarkdownImagePathSuffix(value: string): { path: string; suffix: string } {
  const match = value.match(/^([^?#]*)([?#].*)?$/u);
  return {
    path: match?.[1]?.trim() ?? value.trim(),
    suffix: match?.[2] ?? "",
  };
}

// conversationBlockHasToolEvidence separates real tool metadata from prose-derived display text.
// conversationBlockHasToolEvidence 将真实工具元数据与正文推断出的展示文本分离。
function conversationBlockHasToolEvidence(
  kind: ConversationBlockKind,
  attributes: Record<string, string>,
  rawMetadata?: unknown,
): boolean {
  if (!structuredKindLooksLikeToolBlock(kind)) {
    return false;
  }
  if (attributes.tool_evidence === "true" || attributes.tool_call_id || attributes.tool_result_id || attributes.command_session_id || attributes.task_session_id) {
    return true;
  }
  if (attributes.tool_name || attributes.name) {
    return true;
  }
  const hostKind = metadataString(rawMetadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ]);
  if (hostKind && ["tool_error", "tool_result", "tool_call", "task_session", "command_session", "command_run", "change_set"].includes(hostKind)) {
    return true;
  }
  return Boolean(metadataString(rawMetadata, [
    "payload.tool",
    "host_result.payload.tool",
    "hostResult.payload.tool",
    "payload.tool_name",
    "host_result.payload.tool_name",
    "hostResult.payload.toolName",
  ]));
}

// normalizeRelativeMarkdownImagePath removes harmless relative prefixes without allowing root escapes.
// normalizeRelativeMarkdownImagePath 移除无害的相对路径前缀，同时避免跳出项目根目录。
function normalizeRelativeMarkdownImagePath(value: string): string | null {
  const normalized = value.replace(/\\/g, "/").replace(/^\.\/+/u, "").replace(/^\/+/u, "").trim();
  if (!normalized || normalized.split("/").some((segment) => segment === "..")) {
    return null;
  }
  return normalized;
}

// joinWorkspaceRelativePath joins one project root and one already-safe relative media path.
// joinWorkspaceRelativePath 拼接项目根目录与已确认安全的相对媒体路径。
function joinWorkspaceRelativePath(workspaceRoot: string, relativePath: string): string {
  const separator = workspaceRoot.includes("\\") ? "\\" : "/";
  const cleanRoot = workspaceRoot.replace(/[\\/]+$/u, "");
  return `${cleanRoot}${separator}${relativePath.replace(/[\\/]+/gu, separator)}`;
}

// useGeneratedMediaSource resolves a generated local media path into a WebUI-renderable source URL.
// useGeneratedMediaSource 将生成的本地媒体路径解析为 WebUI 可渲染的源 URL。
function useGeneratedMediaSource(path: string, _mimeType?: string): string {
  return React.useMemo(() => (path ? mediaSourceUrl(path) : ""), [path]);
}

// toolMetadataContainsReturnPayload returns whether a tool result carries an API-level returned payload.
// toolMetadataContainsReturnPayload 返回工具结果是否携带 API 层已返回载荷。
function toolMetadataContainsReturnPayload(metadata: unknown, body?: string): boolean {
  if (typeof body === "string" && body.trim()) {
    return true;
  }
  return Boolean(
    metadataString(metadata, ["tool_result_id", "toolResultId", "payload.tool_result_id", "payload.toolResultId"])
      ?? metadataValue(metadata, ["host_result", "hostResult", "payload"]),
  );
}

// ConsoleShellContent renders the workspace shell after sidebar context is available.
// ConsoleShellContent 在侧栏上下文可用后渲染工作台外壳。
function ConsoleShellContent(props: {
  startup: StartupSnapshot;
  workbench: WorkbenchSnapshot;
  selectedDraftConversation: DraftConversationSelection | null;
  selectedConversation?: ConversationSummary;
  selectedConversationId: string | null;
  loadingConversationId: string | null;
  conversationLoadError: string | null;
  pinnedConversationIds: Set<string>;
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  onCreateConversation: (project: ProjectSummary) => void;
  onCreateProject: () => void;
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onReorderProjects: (projectIds: string[]) => void;
  onSelectConversation: (conversationId: string) => void;
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  onStopConversationRun: () => Promise<boolean>;
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  onCloseSubagentPanel: () => Promise<void>;
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
}) {
  // sidebar controls the left project tree that can collapse automatically.
  // sidebar 控制可自动折叠的左侧项目树。
  const sidebar = useSidebar();
  // shellRef measures the available workspace width for automatic panel collapse.
  // shellRef 测量工作台可用宽度，用于自动折叠面板。
  const shellRef = React.useRef<HTMLDivElement | null>(null);
  // previousShellWidthRef tracks resize direction so auto-restore only runs while width grows.
  // previousShellWidthRef 跟踪尺寸变化方向，确保自动恢复只在宽度增长时执行。
  const previousShellWidthRef = React.useRef<number | null>(null);
  // rightPanelOpen stores whether the review and memory panel is visible.
  // rightPanelOpen 保存审核与记忆面板是否可见。
  const [rightPanelOpen, setRightPanelOpen] = React.useState(false);
  // rightPanelFloating stores whether the right panel overlays instead of resizing the chat area.
  // rightPanelFloating 保存右侧面板是否以浮动方式覆盖而不是压缩会话区。
  const [rightPanelFloating, setRightPanelFloating] = React.useState(false);
  // leftAutoCollapsed records automatic left-sidebar collapse caused by width pressure.
  // leftAutoCollapsed 记录宽度压力导致的左侧栏自动折叠。
  const [leftAutoCollapsed, setLeftAutoCollapsed] = React.useState(false);
  // handleLeftPanelToggle clears automatic restore intent when the user toggles the left sidebar manually.
  // handleLeftPanelToggle 在用户手动切换左侧栏时清除自动恢复意图。
  const handleLeftPanelToggle = React.useCallback(() => {
    setLeftAutoCollapsed(false);
  }, []);
  // handleRightPanelToggle clears automatic restore intent when the user toggles the right panel manually.
  // handleRightPanelToggle 在用户手动切换右侧面板时清除自动恢复意图。
  const handleRightPanelToggle = React.useCallback(() => {
    const nextOpen = !rightPanelOpen;
    if (!nextOpen) {
      setRightPanelFloating(false);
      setRightPanelOpen(false);
      return;
    }
    const width = shellRef.current?.getBoundingClientRect().width ?? 0;
    const expandedLeftWidth = 288;
    const collapsedLeftWidth = 46;
    const preferredRightWidth = 480;
    const handleWidth = 9;
    const leftWidth = sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
    setRightPanelFloating(width > 0 && width - leftWidth - preferredRightWidth - handleWidth < 800);
    setRightPanelOpen(true);
  }, [rightPanelOpen, sidebar.open]);

  React.useEffect(() => {
    const element = shellRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      const width = entry.contentRect.width;
      const previousWidth = previousShellWidthRef.current;
      const isGrowing = previousWidth !== null && width > previousWidth + 1;
      previousShellWidthRef.current = width;
      const expandedLeftWidth = 288;
      const collapsedLeftWidth = 46;
      const preferredRightWidth = 480;
      const handleWidth = 9;
      const leftWidth = sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
      const dockedPanelFits = width - leftWidth - preferredRightWidth - handleWidth >= 800;
      const shouldFloatRightPanel = rightPanelOpen && !dockedPanelFits;
      const dockedRightWidth = rightPanelOpen && !shouldFloatRightPanel ? preferredRightWidth : 0;
      const shellGaps = rightPanelOpen && !shouldFloatRightPanel ? handleWidth : 0;
      const mainWidth = width - leftWidth - dockedRightWidth - shellGaps;
      if (rightPanelOpen) {
        setRightPanelFloating(shouldFloatRightPanel);
      }
      if (mainWidth < 800 && sidebar.open) {
        sidebar.setOpen(false);
        setLeftAutoCollapsed(true);
        return;
      }
      if (!isGrowing) {
        return;
      }
      if (leftAutoCollapsed && !sidebar.open) {
        const restoredRightWidth = rightPanelOpen && !shouldFloatRightPanel ? preferredRightWidth : 0;
        const restoredShellGaps = rightPanelOpen && !shouldFloatRightPanel ? handleWidth : 0;
        const restoredMainWidth = width - expandedLeftWidth - restoredRightWidth - restoredShellGaps;
        if (restoredMainWidth >= 800) {
          sidebar.setOpen(true);
          setLeftAutoCollapsed(false);
          return;
        }
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [leftAutoCollapsed, rightPanelFloating, rightPanelOpen, sidebar]);

  return (
    <div className="workspace-panel-shell" ref={shellRef}>
      <ProjectSidebar
        projects={props.workbench.projects}
        conversations={props.workbench.conversations}
        expandedProjectIds={props.workbench.expandedProjectIds}
        pinnedConversationIds={props.pinnedConversationIds}
        selectedConversationId={props.selectedConversationId}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onCreateProject={props.onCreateProject}
        onExpandedProjectIdsChange={props.onExpandedProjectIdsChange}
        onProjectMenuAction={props.onProjectMenuAction}
        onReorderProjects={props.onReorderProjects}
        onSelectConversation={props.onSelectConversation}
      />
      <SidebarInset className="workspace-inset">
        <WorkspaceResizableBody
          rightPanelFloating={rightPanelFloating}
          rightPanelOpen={rightPanelOpen}
          startup={props.startup}
          workbench={props.workbench}
          selectedDraftConversation={props.selectedDraftConversation}
          selectedConversation={props.selectedConversation}
          selectedConversationId={props.selectedConversationId}
          loadingConversationId={props.loadingConversationId}
          conversationLoadError={props.conversationLoadError}
          onToggleLeftPanel={handleLeftPanelToggle}
          onToggleRightPanel={handleRightPanelToggle}
          onSelectWorkbenchModel={props.onSelectWorkbenchModel}
          onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
          onSlashCommand={props.onSlashCommand}
          onSubmitPrompt={props.onSubmitPrompt}
          onStopConversationRun={props.onStopConversationRun}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
          onCloseSubagentPanel={props.onCloseSubagentPanel}
          onStopSubagentTask={props.onStopSubagentTask}
          onEditPendingPrompt={props.onEditPendingPrompt}
          onInsertPendingPrompt={props.onInsertPendingPrompt}
          onRemovePendingPrompt={props.onRemovePendingPrompt}
          onReorderPendingPrompts={props.onReorderPendingPrompts}
        />
      </SidebarInset>
    </div>
  );
}

// ConsoleHeader renders the inset title bar for the current conversation.
// ConsoleHeader 渲染当前会话的 inset 标题栏。
function ConsoleHeader(props: {
  conversation?: ConversationSummary;
  draftConversation: DraftConversationSelection | null;
  rightPanelOpen: boolean;
  subagentNavigation?: WorkbenchSnapshot["subagentNavigation"];
  onToggleLeftPanel: () => void;
  onToggleRightPanel: () => void;
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  onCloseSubagentPanel: () => Promise<void>;
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
}) {
  // t translates stable console chrome labels.
  // t 翻译稳定的控制台外壳标签。
  const { t } = useI18n();
  // rightPanelLabel describes the current right panel toggle action.
  // rightPanelLabel 描述当前右侧面板切换动作。
  const rightPanelLabel = props.rightPanelOpen ? t("contextPanel.close") : t("contextPanel.open");

  return (
    <header className="console-header">
      <div className="console-header-main">
        <SidebarTrigger onClick={props.onToggleLeftPanel} />
        <span className="console-header-separator" aria-hidden="true" />
        <h1>{props.draftConversation ? t("console.newConversation") : props.conversation?.title ?? t("console.title")}</h1>
        <SubagentNavigationMenu
          navigation={props.subagentNavigation}
          onCloseSubagentPanel={props.onCloseSubagentPanel}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
          onStopSubagentTask={props.onStopSubagentTask}
        />
      </div>
      <div className="console-header-actions">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              aria-label={rightPanelLabel}
              aria-pressed={props.rightPanelOpen}
              className="icon-button"
              onClick={props.onToggleRightPanel}
              size="icon-sm"
              type="button"
              variant={props.rightPanelOpen ? "secondary" : "ghost"}
            >
              {props.rightPanelOpen ? <PanelRightClose size={16} /> : <PanelRightOpen size={16} />}
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">{rightPanelLabel}</TooltipContent>
        </Tooltip>
      </div>
    </header>
  );
}

// WorkspaceResizableBody renders the main chat area and optional right context panel.
// WorkspaceResizableBody 渲染主会话区域与可选右侧上下文面板。
function WorkspaceResizableBody(props: {
  // Whether the right context panel is visible.
  // 右侧上下文面板是否可见。
  rightPanelOpen: boolean;
  // Whether the right context panel floats over the main area.
  // 右侧上下文面板是否浮动覆盖主区域。
  rightPanelFloating: boolean;
  // Startup snapshot forwarded to the chat area.
  // 转发给会话区域的启动快照。
  startup: StartupSnapshot;
  // Workbench snapshot forwarded to the chat area.
  // 转发给会话区域的工作台快照。
  workbench: WorkbenchSnapshot;
  // Current draft conversation selection.
  // 当前草稿会话选择。
  selectedDraftConversation: DraftConversationSelection | null;
  // Current selected conversation summary.
  // 当前选中会话摘要。
  selectedConversation?: ConversationSummary;
  // Current selected conversation id.
  // 当前选中会话标识。
  selectedConversationId: string | null;
  // Conversation id currently loading from service.
  // 当前正在从服务载入的会话标识。
  loadingConversationId: string | null;
  // Conversation load error shown by the chat area.
  // 会话区域展示的载入错误。
  conversationLoadError: string | null;
  // Toggles the right context panel.
  // 切换右侧上下文面板。
  onToggleRightPanel: () => void;
  // Records a manual left sidebar toggle.
  // 记录一次手动左侧栏切换。
  onToggleLeftPanel: () => void;
  // Persists one workbench model selection through the backend runtime.
  // 通过后端运行时持久化单个工作台模型选择。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Persists one workbench reasoning-effort selection through the backend runtime.
  // 通过后端运行时持久化单个工作台思考档位选择。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Routes one projected composer slash command through the workbench shell.
  // 通过工作台外壳路由单个投影输入区 slash 指令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Sends one composer prompt through the backend runtime.
  // 通过后端运行时发送一条输入区提示词。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Stops the selected backend conversation run.
  // 停止当前选中的后端会话运行。
  onStopConversationRun: () => Promise<boolean>;
  // Opens one backend-routed subagent surface.
  // 打开单个由后端路由的子代理界面。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Closes the backend-routed subagent surface.
  // 关闭由后端路由的子代理界面。
  onCloseSubagentPanel: () => Promise<void>;
  // Stops one backend-routed subagent task.
  // 停止单个由后端路由的子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Restores one pending prompt into the composer.
  // 将单条待发送提示词恢复到输入区。
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  // Routes one pending prompt into the shared insertion queue.
  // 将单条待发送提示词路由进共享引导队列。
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  // Removes one pending prompt from the queue.
  // 从队列中移除单条待发送提示词。
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  // Persists the visible pending prompt order.
  // 持久化可见待发送提示词顺序。
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
}) {
  return (
    <div className={cn("workspace-resizable-body-shell", props.rightPanelFloating && "has-floating-context")}>
      <ResizablePanelGroup className="workspace-resizable-body" defaultLayout={{ workspaceMain: 68, workspaceContext: 32 }} orientation="horizontal">
        <ResizablePanel
          className="workspace-main-panel"
          defaultSize={props.rightPanelOpen && !props.rightPanelFloating ? 68 : 100}
          id="workspaceMain"
          minSize="800px"
        >
          <section className="workspace-main-shell">
            <ConsoleHeader
              draftConversation={props.selectedDraftConversation}
              conversation={props.selectedConversation}
              rightPanelOpen={props.rightPanelOpen}
              subagentNavigation={props.workbench.subagentNavigation}
              onToggleLeftPanel={props.onToggleLeftPanel}
              onToggleRightPanel={props.onToggleRightPanel}
              onOpenSubagentPanel={props.onOpenSubagentPanel}
              onCloseSubagentPanel={props.onCloseSubagentPanel}
              onStopSubagentTask={props.onStopSubagentTask}
            />
            <ChatArea
              startup={props.startup}
              workbench={props.workbench}
              selectedDraftConversation={props.selectedDraftConversation}
              selectedConversationId={props.selectedConversationId}
              loadingConversationId={props.loadingConversationId}
              conversationLoadError={props.conversationLoadError}
              onSelectWorkbenchModel={props.onSelectWorkbenchModel}
              onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
              onSlashCommand={props.onSlashCommand}
              onSubmitPrompt={props.onSubmitPrompt}
              onStopConversationRun={props.onStopConversationRun}
              onOpenSubagentPanel={props.onOpenSubagentPanel}
              onCloseSubagentPanel={props.onCloseSubagentPanel}
              onStopSubagentTask={props.onStopSubagentTask}
              onEditPendingPrompt={props.onEditPendingPrompt}
              onInsertPendingPrompt={props.onInsertPendingPrompt}
              onRemovePendingPrompt={props.onRemovePendingPrompt}
              onReorderPendingPrompts={props.onReorderPendingPrompts}
            />
          </section>
        </ResizablePanel>
        {props.rightPanelOpen && !props.rightPanelFloating ? (
          <>
            <ResizableHandle className="workspace-context-resize-handle" withHandle />
            <ResizablePanel
              className="workspace-context-panel"
              defaultSize={32}
              groupResizeBehavior="preserve-pixel-size"
              id="workspaceContext"
              minSize="320px"
            >
              <WorkspaceContextPanel
                selectedConversationId={props.selectedConversationId}
                workbench={props.workbench}
                onClose={props.onToggleRightPanel}
              />
            </ResizablePanel>
          </>
        ) : null}
      </ResizablePanelGroup>
      {props.rightPanelOpen && props.rightPanelFloating ? (
        <div className="workspace-context-floating-panel">
          <WorkspaceContextPanel
            floating
            selectedConversationId={props.selectedConversationId}
            workbench={props.workbench}
            onClose={props.onToggleRightPanel}
          />
        </div>
      ) : null}
    </div>
  );
}

// WorkspaceReviewPanelCacheEntry stores one mounted review panel state across close and reopen cycles.
// WorkspaceReviewPanelCacheEntry 保存一次右侧审核面板在关闭与重开之间需要复用的状态。
interface WorkspaceReviewPanelCacheEntry {
  // Latest workspace diff snapshot rendered by the review panel.
  // 审核面板最近渲染的工作区 diff 快照。
  snapshot: DiffPanelSnapshot | null;
  // Git review source selection used by the cached snapshot.
  // 缓存快照对应的 Git 审核来源选择。
  selection: DiffReviewSelection;
  // Conversation or subagent surface that produced the cached snapshot.
  // 生成缓存快照的会话或子代理界面标识。
  surfaceId: string | null;
  // Workspace path that produced the cached snapshot.
  // 生成缓存快照的工作区路径。
  workspacePath: string | null;
  // Stable Git review source key that produced the cached snapshot.
  // 生成缓存快照的稳定 Git 审核来源键。
  reviewKey: string | null;
  // Last refresh error shown by the panel.
  // 面板最近展示的刷新错误。
  error: string | null;
}

// workspaceReviewPanelCache preserves review content while the right rail is temporarily collapsed.
// workspaceReviewPanelCache 在右侧栏临时折叠时保留审核内容。
const workspaceReviewPanelCache = new Map<string, WorkspaceReviewPanelCacheEntry>();

// lastWorkspaceReviewVisibleSurfaceId tracks the latest conversation surface that the review panel represented.
// lastWorkspaceReviewVisibleSurfaceId 跟踪审核面板最近代表的会话界面。
let lastWorkspaceReviewVisibleSurfaceId: string | null = null;

// workspaceReviewPanelCacheLimit caps cached review states to avoid retaining old diff trees forever.
// workspaceReviewPanelCacheLimit 限制缓存的审核状态数量，避免长期保留旧 diff 树。
const workspaceReviewPanelCacheLimit = 12;

// workspaceReviewPanelCacheKey builds a per-surface cache key for right-panel review state.
// workspaceReviewPanelCacheKey 为右侧审核状态构建按界面隔离的缓存键。
function workspaceReviewPanelCacheKey(surfaceId: string | null, workspacePath: string | null): string {
  return [surfaceId ?? "no-surface", workspacePath ?? "no-workspace"].join("\u001f");
}

// rememberWorkspaceReviewPanelCache writes one cache entry and trims the oldest entries.
// rememberWorkspaceReviewPanelCache 写入单个缓存条目并裁剪最旧条目。
function rememberWorkspaceReviewPanelCache(key: string, entry: WorkspaceReviewPanelCacheEntry): void {
  workspaceReviewPanelCache.set(key, entry);
  while (workspaceReviewPanelCache.size > workspaceReviewPanelCacheLimit) {
    const oldestKey = workspaceReviewPanelCache.keys().next().value;
    if (!oldestKey) {
      break;
    }
    workspaceReviewPanelCache.delete(oldestKey);
  }
}

// WorkspaceContextPanel renders review and memory tabs for the right context rail.
// WorkspaceContextPanel 渲染右侧上下文栏中的审核与记忆标签页。
function WorkspaceContextPanel(props: {
  // Current selected conversation id from the local workbench route.
  // 来自本地工作台路由的当前选中会话标识。
  selectedConversationId: string | null;
  // Workbench snapshot carrying selected-session side-panel projections.
  // 携带当前选中会话侧面板投影的工作台快照。
  workbench: WorkbenchSnapshot;
  // Whether the panel is rendered as a floating overlay.
  // 面板是否以浮动覆盖层方式渲染。
  floating?: boolean;
  // Closes the right context panel.
  // 关闭右侧上下文面板。
  onClose?: () => void;
}) {
  // t translates the right context panel shell labels.
  // t 翻译右侧上下文面板外壳标签。
  const { t } = useI18n();
  // reviewSnapshot stores the newest renderable diff for the selected conversation.
  // reviewSnapshot 保存当前选中会话中最新的可渲染 diff。
  const reviewSnapshot = React.useMemo(
    () => latestConversationDiffSnapshot(props.workbench, props.selectedConversationId),
    [props.selectedConversationId, props.workbench],
  );
  // activeSurfaceId stores the conversation or subagent surface currently represented by the panel.
  // activeSurfaceId 保存当前面板代表的会话或子代理界面标识。
  const activeSurfaceId = activeWorkbenchSurfaceId(props.workbench, props.selectedConversationId);
  // workspaceProjectId stores the backend project id used to resolve the Git diff path.
  // workspaceProjectId 保存用于解析 Git diff 路径的后端项目标识。
  const workspaceProjectId = React.useMemo(
    () => selectedConversationProjectId(props.workbench, props.selectedConversationId),
    [props.selectedConversationId, props.workbench],
  );
  // workspacePath stores the selected conversation owner project path for live diff refresh.
  // workspacePath 保存当前选中会话所属项目路径，用于刷新实时 diff。
  const workspacePath = React.useMemo(
    () => selectedConversationWorkspacePath(props.workbench, props.selectedConversationId),
    [props.selectedConversationId, props.workbench],
  );
  // reviewPanelCacheKey stores the cache bucket for this visible conversation surface.
  // reviewPanelCacheKey 保存当前可见会话界面的审核缓存桶。
  const reviewPanelCacheKey = React.useMemo(
    () => workspaceReviewPanelCacheKey(activeSurfaceId, workspacePath),
    [activeSurfaceId, workspacePath],
  );
  // initialReviewPanelCache restores a prior collapsed-panel state for this surface.
  // initialReviewPanelCache 恢复该界面先前折叠面板时保留的状态。
  const initialReviewPanelCache = React.useMemo(
    () => workspaceReviewPanelCache.get(reviewPanelCacheKey) ?? null,
    [reviewPanelCacheKey],
  );
  // workspaceReviewSnapshot stores the latest user-refreshed workspace Git diff.
  // workspaceReviewSnapshot 保存最近一次用户刷新得到的工作区 Git diff。
  const [workspaceReviewSnapshot, setWorkspaceReviewSnapshot] = React.useState<DiffPanelSnapshot | null>(
    () => initialReviewPanelCache?.snapshot ?? null,
  );
  // diffReviewSelection stores the current Git review source selection.
  // diffReviewSelection 保存当前 Git 审核来源选择。
  const [diffReviewSelection, setDiffReviewSelection] = React.useState<DiffReviewSelection>(
    () => initialReviewPanelCache?.selection ?? { mode: "branch" },
  );
  // diffReviewSelectionKey stores a stable key for the current Git review source.
  // diffReviewSelectionKey 保存当前 Git 审核来源的稳定键。
  const diffReviewSelectionKey = React.useMemo(
    () => diffReviewSelectionStableKey(diffReviewSelection),
    [diffReviewSelection],
  );
  // workspaceReviewSurfaceId stores the surface that produced the current workspace diff snapshot.
  // workspaceReviewSurfaceId 保存生成当前工作区 diff 快照的界面标识。
  const [workspaceReviewSurfaceId, setWorkspaceReviewSurfaceId] = React.useState<string | null>(
    () => initialReviewPanelCache?.surfaceId ?? null,
  );
  // workspaceReviewPath stores the project path that produced the current workspace diff snapshot.
  // workspaceReviewPath 保存生成当前工作区 diff 快照的项目路径。
  const [workspaceReviewPath, setWorkspaceReviewPath] = React.useState<string | null>(
    () => initialReviewPanelCache?.workspacePath ?? null,
  );
  // workspaceReviewKey stores the Git review source that produced the current snapshot.
  // workspaceReviewKey 保存生成当前快照的 Git 审核来源。
  const [workspaceReviewKey, setWorkspaceReviewKey] = React.useState<string | null>(
    () => initialReviewPanelCache?.reviewKey ?? null,
  );
  // workspaceReviewLoading tracks the live diff refresh request.
  // workspaceReviewLoading 跟踪实时 diff 刷新请求状态。
  const [workspaceReviewLoading, setWorkspaceReviewLoading] = React.useState(false);
  // workspaceReviewError stores the latest live diff refresh error.
  // workspaceReviewError 保存最近一次实时 diff 刷新错误。
  const [workspaceReviewError, setWorkspaceReviewError] = React.useState<string | null>(
    () => initialReviewPanelCache?.error ?? null,
  );
  // workspaceReviewRequestSeqRef prevents stale async refresh results from replacing the current project diff.
  // workspaceReviewRequestSeqRef 防止过期异步刷新结果覆盖当前项目 diff。
  const workspaceReviewRequestSeqRef = React.useRef(0);
  // hydratedReviewPanelCacheKeyRef prevents stale state from being saved under a newly selected surface.
  // hydratedReviewPanelCacheKeyRef 防止旧状态被写入新选中界面的缓存桶。
  const hydratedReviewPanelCacheKeyRef = React.useRef(reviewPanelCacheKey);
  // forceWorkspaceReviewRefreshRef marks real conversation switches while ignoring simple collapse/reopen cycles.
  // forceWorkspaceReviewRefreshRef 标记真实会话切换，同时忽略单纯折叠再展开。
  const forceWorkspaceReviewRefreshRef = React.useRef(false);
  const activeWorkspaceReviewSnapshot = workspaceReviewSurfaceId === activeSurfaceId
    && workspaceReviewPath === workspacePath
    && workspaceReviewKey === diffReviewSelectionKey
      ? workspaceReviewSnapshot
      : null;
  const activeReviewSnapshot = activeWorkspaceReviewSnapshot ?? reviewSnapshot;

  // handleRefreshWorkspaceDiff refreshes the current project Git diff through the local service.
  // handleRefreshWorkspaceDiff 通过本地服务刷新当前项目 Git diff。
  const handleRefreshWorkspaceDiff = React.useCallback(async (selectionOverride?: DiffReviewSelection) => {
    if (!workspacePath) {
      setWorkspaceReviewError("当前会话没有关联项目路径，无法刷新 Git diff。");
      return;
    }
    const requestSelection = selectionOverride ?? diffReviewSelection;
    const requestKey = diffReviewSelectionStableKey(requestSelection);
    const requestSeq = workspaceReviewRequestSeqRef.current + 1;
    workspaceReviewRequestSeqRef.current = requestSeq;
    setWorkspaceReviewLoading(true);
    setWorkspaceReviewError(null);
    try {
      const response = await loadWorkspaceDiffSnapshot(workspacePath, workspaceProjectId, activeSurfaceId, requestSelection);
      if (workspaceReviewRequestSeqRef.current !== requestSeq) {
        return;
      }
      setWorkspaceReviewSnapshot(diffSnapshotFromWorkspaceResponse(response));
      setWorkspaceReviewSurfaceId(activeSurfaceId);
      setWorkspaceReviewPath(workspacePath);
      setWorkspaceReviewKey(requestKey);
    } catch (error) {
      if (workspaceReviewRequestSeqRef.current !== requestSeq) {
        return;
      }
      const message = error instanceof Error ? error.message : String(error);
      setWorkspaceReviewError(message);
      toast.error(`刷新 Git diff 失败：${message}`);
    } finally {
      if (workspaceReviewRequestSeqRef.current === requestSeq) {
        setWorkspaceReviewLoading(false);
      }
    }
  }, [activeSurfaceId, diffReviewSelection, workspacePath, workspaceProjectId]);

  // handleDiffReviewSelectionChange applies a new Git review source and refreshes it.
  // handleDiffReviewSelectionChange 应用新的 Git 审核来源并刷新。
  const handleDiffReviewSelectionChange = React.useCallback((selection: DiffReviewSelection) => {
    setDiffReviewSelection(selection);
    setWorkspaceReviewSnapshot(null);
    setWorkspaceReviewSurfaceId(activeSurfaceId);
    setWorkspaceReviewPath(workspacePath);
    setWorkspaceReviewKey(null);
    void handleRefreshWorkspaceDiff(selection);
  }, [activeSurfaceId, handleRefreshWorkspaceDiff, workspacePath]);

  // Tracks real surface changes so cached content is reused only for panel collapse, not conversation switching.
  // 跟踪真实界面变化，使缓存只服务于面板折叠，而不会吞掉会话切换刷新。
  React.useEffect(() => {
    if (lastWorkspaceReviewVisibleSurfaceId !== activeSurfaceId) {
      forceWorkspaceReviewRefreshRef.current = true;
      lastWorkspaceReviewVisibleSurfaceId = activeSurfaceId;
    }
  }, [activeSurfaceId]);

  // Rehydrates cached review state when the panel remounts for a different visible surface.
  // 当面板为不同可见界面重新挂载时恢复缓存的审核状态。
  React.useEffect(() => {
    if (hydratedReviewPanelCacheKeyRef.current === reviewPanelCacheKey) {
      return;
    }
    hydratedReviewPanelCacheKeyRef.current = reviewPanelCacheKey;
    workspaceReviewRequestSeqRef.current += 1;
    const cachedEntry = workspaceReviewPanelCache.get(reviewPanelCacheKey) ?? null;
    setDiffReviewSelection(cachedEntry?.selection ?? { mode: "branch" });
    setWorkspaceReviewSnapshot(cachedEntry?.snapshot ?? null);
    setWorkspaceReviewSurfaceId(cachedEntry?.surfaceId ?? activeSurfaceId);
    setWorkspaceReviewPath(cachedEntry?.workspacePath ?? workspacePath);
    setWorkspaceReviewKey(cachedEntry?.reviewKey ?? null);
    setWorkspaceReviewError(cachedEntry?.error ?? null);
    setWorkspaceReviewLoading(false);
  }, [activeSurfaceId, reviewPanelCacheKey, workspacePath]);

  // Persists the current review state so collapsing the panel does not force a fresh diff render.
  // 持久化当前审核状态，使折叠面板不会强制重新生成 diff。
  React.useEffect(() => {
    if (hydratedReviewPanelCacheKeyRef.current !== reviewPanelCacheKey) {
      return;
    }
    rememberWorkspaceReviewPanelCache(reviewPanelCacheKey, {
      snapshot: workspaceReviewSnapshot,
      selection: diffReviewSelection,
      surfaceId: workspaceReviewSurfaceId,
      workspacePath: workspaceReviewPath,
      reviewKey: workspaceReviewKey,
      error: workspaceReviewError,
    });
  }, [
    diffReviewSelection,
    reviewPanelCacheKey,
    workspaceReviewError,
    workspaceReviewKey,
    workspaceReviewPath,
    workspaceReviewSnapshot,
    workspaceReviewSurfaceId,
  ]);

  // Auto-refreshes when the visible conversation moves to a different project.
  // 当可见会话切换到不同项目时自动刷新。
  React.useEffect(() => {
    if (hydratedReviewPanelCacheKeyRef.current !== reviewPanelCacheKey) {
      return;
    }
    const forceRefreshForSurface = forceWorkspaceReviewRefreshRef.current;
    if (!workspacePath) {
      forceWorkspaceReviewRefreshRef.current = false;
      setWorkspaceReviewSnapshot(null);
      setWorkspaceReviewSurfaceId(activeSurfaceId);
      setWorkspaceReviewPath(null);
      setWorkspaceReviewKey(null);
      setWorkspaceReviewError(null);
      setWorkspaceReviewLoading(false);
      return;
    }
    if (
      workspaceReviewSurfaceId === activeSurfaceId
      && workspaceReviewPath === workspacePath
      && workspaceReviewKey === diffReviewSelectionKey
      && workspaceReviewSnapshot
      && !forceRefreshForSurface
    ) {
      setWorkspaceReviewError(null);
      return;
    }
    forceWorkspaceReviewRefreshRef.current = false;
    setWorkspaceReviewSnapshot(null);
    setWorkspaceReviewSurfaceId(activeSurfaceId);
    setWorkspaceReviewPath(workspacePath);
    setWorkspaceReviewKey(diffReviewSelectionKey);
    void handleRefreshWorkspaceDiff();
  }, [
    activeSurfaceId,
    diffReviewSelectionKey,
    handleRefreshWorkspaceDiff,
    reviewPanelCacheKey,
    workspacePath,
    workspaceReviewKey,
    workspaceReviewPath,
    workspaceReviewSnapshot,
    workspaceReviewSurfaceId,
  ]);

  return (
    <aside className={cn("context-panel-shell", props.floating && "floating")}>
      <Tabs className="context-tabs" defaultValue="review">
        <div className="context-panel-header">
          <TabsList className="context-tabs-list">
            <TabsTrigger value="review">{t("contextPanel.review")}</TabsTrigger>
            <TabsTrigger value="memory">{t("contextPanel.memory")}</TabsTrigger>
          </TabsList>
          {props.floating && props.onClose ? (
            <Button
              aria-label={t("contextPanel.close")}
              className="context-panel-close-button"
              onClick={props.onClose}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <PanelRightClose size={16} />
            </Button>
          ) : null}
        </div>
        <TabsContent className="context-tab-content review-tab-content" value="review">
          <DiffReviewPanel
            emptyBody={workspaceReviewError ?? t("contextPanel.reviewEmpty")}
            emptyTitle={t("contextPanel.review")}
            loading={workspaceReviewLoading}
            selection={diffReviewSelection}
            snapshot={activeReviewSnapshot}
            onCopyRaw={(rawDiff) => void copyText(rawDiff)}
            onRefresh={handleRefreshWorkspaceDiff}
            onSelectionChange={handleDiffReviewSelectionChange}
          />
        </TabsContent>
        <TabsContent className="context-tab-content" value="memory">
          <ContextInjectionTabs snapshot={props.workbench.contextInjection ?? null} />
        </TabsContent>
      </Tabs>
    </aside>
  );
}

// activeWorkbenchSurfaceId returns the currently visible parent or subagent conversation id.
// activeWorkbenchSurfaceId 返回当前可见的父会话或子代理会话标识。
function activeWorkbenchSurfaceId(workbench: WorkbenchSnapshot, selectedConversationId: string | null | undefined): string | null {
  const parentConversationId = selectedConversationId ?? workbench.selectedConversationId ?? null;
  if (workbench.subagentNavigation?.parentConversationId === parentConversationId) {
    return workbench.subagentNavigation.activeConversationId ?? parentConversationId;
  }
  return parentConversationId;
}

// selectedConversationProjectId returns the project id for the currently visible conversation.
// selectedConversationProjectId 返回当前可见会话所属的项目标识。
function selectedConversationProjectId(workbench: WorkbenchSnapshot, selectedConversationId: string | null | undefined): string | null {
  const activeSurfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  const parentConversationId = selectedConversationId ?? workbench.selectedConversationId ?? null;
  const conversation = activeSurfaceId
    ? workbench.conversations.find((item) => item.id === activeSurfaceId)
      ?? workbench.conversations.find((item) => item.id === parentConversationId)
      : null;
  if (conversation?.projectId) {
    return conversation.projectId;
  }
  return conversation ? projectForConversation(workbench, conversation)?.id ?? null : workbench.projects[0]?.id ?? null;
}

// selectedConversationWorkspacePath returns the project path for the currently visible conversation.
// selectedConversationWorkspacePath 返回当前可见会话所属的项目路径。
function selectedConversationWorkspacePath(workbench: WorkbenchSnapshot, selectedConversationId: string | null | undefined): string | null {
  const activeSurfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  const parentConversationId = selectedConversationId ?? workbench.selectedConversationId ?? null;
  if (!activeSurfaceId) {
    return workbench.projects[0]?.path ?? null;
  }
  const conversation = workbench.conversations.find((item) => item.id === activeSurfaceId)
    ?? workbench.conversations.find((item) => item.id === parentConversationId);
  if (!conversation) {
    return workbench.projects[0]?.path ?? null;
  }
  return projectForConversation(workbench, conversation)?.path ?? null;
}

// diffSnapshotFromWorkspaceResponse converts the backend Git response into renderer-ready state.
// diffSnapshotFromWorkspaceResponse 将后端 Git 响应转换为可渲染状态。
function diffSnapshotFromWorkspaceResponse(response: WorkspaceDiffSnapshotResponse): DiffPanelSnapshot {
  const snapshot = parseUnifiedDiff(response.unifiedDiff, "workspace", response.repositoryRoot);
  return {
    ...snapshot,
    notice: response.notice ?? snapshot.notice,
    stats: {
      files: response.changedFiles || snapshot.stats.files,
      additions: response.additions || snapshot.stats.additions,
      deletions: response.deletions || snapshot.stats.deletions,
    },
    currentBranch: response.currentBranch ?? null,
    defaultBranch: response.defaultBranch ?? null,
    selectedBaseBranch: response.selectedBaseBranch ?? null,
    selectedCompareBranch: response.selectedCompareBranch ?? null,
    selectedCommitHash: response.selectedCommitHash ?? null,
    branches: response.branches ?? [],
    recentCommits: response.recentCommits ?? [],
  };
}

// diffReviewSelectionStableKey builds a compact stable key for one Git review selection.
// diffReviewSelectionStableKey 为单个 Git 审核选择构建紧凑稳定键。
function diffReviewSelectionStableKey(selection: DiffReviewSelection): string {
  return [
    selection.mode,
    selection.baseBranch?.trim() ?? "",
    selection.compareBranch?.trim() ?? "",
    selection.commitHash?.trim() ?? "",
  ].join("\u001f");
}

// latestConversationDiffSnapshot resolves the newest structured diff from the selected conversation cache.
// latestConversationDiffSnapshot 从当前选中会话缓存中解析最新的结构化 diff。
function latestConversationDiffSnapshot(workbench: WorkbenchSnapshot, selectedConversationId: string | null | undefined): DiffPanelSnapshot | null {
  const activeSurfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  if (!activeSurfaceId) {
    return null;
  }
  const messages = workbench.messages.filter((message) => message.conversationId === activeSurfaceId);
  for (const message of [...messages].reverse()) {
    const blocks = parseConversationBlocks(message);
    const diffBlock = [...blocks].reverse().find((block) => block.kind === "diff" && hasRenderableDiff(block.body));
    if (diffBlock) {
      return parseUnifiedDiff(diffBlock.body, diffPanelSourceForMessage(message), "");
    }
  }
  return null;
}

// diffPanelSourceForMessage derives a display source from structured message metadata.
// diffPanelSourceForMessage 从结构化消息元数据中推导展示来源。
function diffPanelSourceForMessage(message: ConversationMessage): DiffPanelSnapshot["source"] {
  const hostKind = metadataString(message.metadata, ["kind", "host_result.kind", "hostResult.kind"]);
  if (hostKind === "change_set") {
    return "change_set";
  }
  return "message";
}

// ContextInjectionTabs renders iced-aligned dynamic and static injection lists.
// ContextInjectionTabs 渲染对齐 iced 的动态与静态注入列表。
function ContextInjectionTabs(props: {
  // Selected-session context injection snapshot from the service.
  // 来自服务的当前选中会话上下文注入快照。
  snapshot: ContextInjectionSnapshot | null;
}) {
  // t translates stable side-panel labels.
  // t 翻译稳定侧面板标签。
  const { t } = useI18n();
  const dynamicCards = props.snapshot?.dynamicCards ?? [];
  const staticCards = props.snapshot?.staticCards ?? [];

  return (
    <Tabs className="context-injection-tabs" defaultValue="dynamic">
      <TabsList className="context-injection-tabs-list">
        <TabsTrigger value="dynamic">{t("contextPanel.dynamic")}</TabsTrigger>
        <TabsTrigger value="static">{t("contextPanel.static")}</TabsTrigger>
      </TabsList>
      <TabsContent className="context-injection-tab-content" value="dynamic">
        <ContextInjectionCardList
          cards={dynamicCards}
          emptyIcon={<BrainCircuit size={20} />}
          emptyText={t("contextPanel.dynamicEmpty")}
          title={t("contextPanel.dynamic")}
        />
      </TabsContent>
      <TabsContent className="context-injection-tab-content" value="static">
        <ContextInjectionCardList
          cards={staticCards}
          emptyIcon={<Database size={20} />}
          emptyText={t("contextPanel.staticEmpty")}
          title={t("contextPanel.static")}
        />
      </TabsContent>
    </Tabs>
  );
}

// ContextInjectionCardList renders one injection card collection or its empty state.
// ContextInjectionCardList 渲染一组注入卡片或其空状态。
function ContextInjectionCardList(props: {
  // Injection cards to display.
  // 需要展示的注入卡片。
  cards: ContextInjectionCardSummary[];
  // Empty state icon.
  // 空状态图标。
  emptyIcon: React.ReactNode;
  // Empty state title.
  // 空状态标题。
  title: string;
  // Empty state message.
  // 空状态消息。
  emptyText: string;
}) {
  if (props.cards.length === 0) {
    return (
      <div className="context-empty-state">
        {props.emptyIcon}
        <strong>{props.title}</strong>
        <span>{props.emptyText}</span>
      </div>
    );
  }

  return (
    <div className="context-injection-card-list">
      {props.cards.map((card) => (
        <ContextInjectionCardView card={card} key={card.tag} />
      ))}
    </div>
  );
}

// ContextInjectionCardView renders one injection source group and its items.
// ContextInjectionCardView 渲染单个注入来源分组及其条目。
function ContextInjectionCardView(props: {
  // Injection source card to render.
  // 要渲染的注入来源卡片。
  card: ContextInjectionCardSummary;
}) {
  return (
    <article className="context-injection-card">
      <header className="context-injection-card-header">
        <span className="context-injection-tag">{props.card.tag}</span>
        <p>{props.card.description}</p>
      </header>
      <div className="context-injection-item-list">
        {props.card.items.map((item, index) => (
          <details className="context-injection-item" key={`${props.card.tag}-${index}`}>
            <summary>
              <span className="context-injection-item-content">{item.content}</span>
              <span className="context-injection-item-meta">
                {item.turnLabel ? <span>{item.turnLabel}</span> : null}
                <span>{item.ttlLabel}</span>
                {item.sourceDisplay ? <span>{item.sourceDisplay}</span> : null}
              </span>
            </summary>
            <pre>{item.detail}</pre>
          </details>
        ))}
      </div>
    </article>
  );
}

// ProjectSidebar renders projects and conversations.
// ProjectSidebar 渲染项目与会话。
function ProjectSidebar(props: {
  projects: ProjectSummary[];
  conversations: ConversationSummary[];
  expandedProjectIds?: string[];
  pinnedConversationIds: Set<string>;
  selectedConversationId: string | null;
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  onCreateConversation: (project: ProjectSummary) => void;
  onCreateProject: () => void;
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onReorderProjects: (projectIds: string[]) => void;
  onSelectConversation: (conversationId: string) => void;
}) {
  // t translates stable sidebar labels while project names remain user data.
  // t 翻译稳定侧栏标签，项目名称仍保留为用户数据。
  const { t, locale } = useI18n();
  // sidebarState selects the expanded tree interaction or the collapsed project pop menu.
  // sidebarState 选择展开树交互或折叠项目弹出菜单交互。
  const { state: sidebarState } = useSidebar();
  // openProjectIds stores the project folders currently expanded in the tree.
  // openProjectIds 保存当前在树中展开的项目文件夹。
  const [openProjectIds, setOpenProjectIds] = React.useState<Set<string>>(
    () => initialExpandedProjectIdSet(props.projects, props.expandedProjectIds),
  );
  // activeProjectDragId records when project sorting is active so hover-only UI can pause.
  // activeProjectDragId 记录项目排序激活状态，以便暂停仅悬停显示的界面。
  const [activeProjectDragId, setActiveProjectDragId] = React.useState<string | null>(null);
  // expandedConversationProjectIds stores project ids whose conversation list is fully expanded.
  // expandedConversationProjectIds 保存会话列表已完整展开的项目标识。
  const [expandedConversationProjectIds, setExpandedConversationProjectIds] = React.useState<Set<string>>(() => new Set());
  // projectDragSensors enables pointer dragging only after a small movement threshold.
  // projectDragSensors 仅在达到小幅移动阈值后启用指针拖拽。
  const projectDragSensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
  // conversationsByProject groups conversations beneath their owning project folder.
  // conversationsByProject 将会话归组到所属项目文件夹下。
  const conversationsByProject = React.useMemo(() => {
    const fallbackProjectId = props.projects[0]?.id ?? "default-project";
    return props.conversations.reduce<Record<string, ConversationSummary[]>>((groups, conversation) => {
      const projectId = conversation.projectId ?? fallbackProjectId;
      groups[projectId] = groups[projectId] ?? [];
      groups[projectId].push(conversation);
      return groups;
    }, {});
  }, [props.conversations, props.projects]);
  // sortedConversationsByProject keeps the service-provided recency order for each project.
  // sortedConversationsByProject 保留服务端提供的项目内最近活动顺序。
  const sortedConversationsByProject = React.useMemo(() => {
    return conversationsByProject;
  }, [conversationsByProject]);
  // pinnedConversations renders conversation pins in an independent section above pinned projects.
  // pinnedConversations 将置顶会话渲染到置顶项目上方的独立分区。
  const pinnedConversations = React.useMemo(() => {
    return props.conversations.filter((conversation) => props.pinnedConversationIds.has(conversation.id));
  }, [props.conversations, props.pinnedConversationIds]);
  // projectsById resolves pinned conversation project icons without changing conversation data.
  // projectsById 在不改变会话数据的情况下解析置顶会话所属项目图标。
  const projectsById = React.useMemo(() => {
    return new Map(props.projects.map((project) => [project.id, project]));
  }, [props.projects]);
  // pinnedProjects mirrors the iced task tree section split.
  // pinnedProjects 对齐 iced 任务树中的置顶分区拆分。
  const pinnedProjects = React.useMemo(() => props.projects.filter((project) => project.pinned), [props.projects]);
  // regularProjects keeps non-pinned projects in their service-provided order.
  // regularProjects 保留普通项目在服务端快照中的原始顺序。
  const regularProjects = React.useMemo(() => props.projects.filter((project) => !project.pinned), [props.projects]);
  // nowMs refreshes relative time labels without touching hover or confirmation state.
  // nowMs 刷新相对时间标签且不触碰悬停或确认状态。
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  React.useEffect(() => {
    const intervalId = window.setInterval(() => setNowMs(Date.now()), 60_000);
    return () => window.clearInterval(intervalId);
  }, []);
  React.useEffect(() => {
    setOpenProjectIds(initialExpandedProjectIdSet(props.projects, props.expandedProjectIds));
  }, [props.expandedProjectIds, props.projects]);
  React.useEffect(() => {
    const visibleProjectIds = new Set(props.projects.map((project) => project.id));
    setExpandedConversationProjectIds((current) => {
      const next = new Set([...current].filter((projectId) => visibleProjectIds.has(projectId)));
      return next.size === current.size ? current : next;
    });
  }, [props.projects]);
  // handleProjectDragEnd persists one dnd-kit same-section reorder result.
  // handleProjectDragEnd 持久化一次 dnd-kit 同分区项目排序结果。
  const handleProjectDragEnd = React.useCallback((sectionProjects: ProjectSummary[], event: DragEndEvent) => {
    setActiveProjectDragId(null);
    if (!event.over || event.active.id === event.over.id) {
      return;
    }
    const sectionIds = sectionProjects.map((project) => project.id);
    const activeIndex = sectionIds.indexOf(String(event.active.id));
    const overIndex = sectionIds.indexOf(String(event.over.id));
    if (activeIndex < 0 || overIndex < 0) {
      return;
    }
    props.onReorderProjects(arrayMove(sectionIds, activeIndex, overIndex));
  }, [props]);
  // toggleProjectOpen toggles one project folder without changing the selected conversation.
  // toggleProjectOpen 切换单个项目文件夹展开状态且不改变当前会话选择。
  const toggleProjectOpen = React.useCallback((projectId: string) => {
    setOpenProjectIds((current) => {
      const next = new Set(current);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      props.onExpandedProjectIdsChange(expandedProjectIdsInProjectOrder(props.projects, next));
      return next;
    });
  }, [props]);
  // toggleProjectConversations toggles whether one project shows every conversation row.
  // toggleProjectConversations 切换单个项目是否显示全部会话行。
  const toggleProjectConversations = React.useCallback((projectId: string) => {
    setExpandedConversationProjectIds((current) => {
      const next = new Set(current);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      return next;
    });
  }, []);
  // renderProjectGroupLabel renders the pinned/project heading with optional actions.
  // renderProjectGroupLabel 渲染置顶/项目标题，并按需显示操作入口。
  const renderProjectGroupLabel = (label: string, icon: React.ReactNode, action?: React.ReactNode) => (
    <SidebarGroupLabel className="project-group-label">
      <span className="project-group-label-main">{icon}<span className="project-group-label-text">{label}</span></span>
      {action}
    </SidebarGroupLabel>
  );
  // renderProjectRows renders one project section while preserving the shared row behavior.
  // renderProjectRows 渲染单个项目分区并保留共享项目行行为。
  const renderProjectRows = (projects: ProjectSummary[]) => projects.map((project) => {
    const projectConversations = (sortedConversationsByProject[project.id] ?? [])
      .filter((conversation) => !props.pinnedConversationIds.has(conversation.id));
    const projectConversationIds = new Set((sortedConversationsByProject[project.id] ?? []).map((conversation) => conversation.id));
    return (
      <SortableProjectRow
        conversations={projectConversations}
        hasSelectedConversation={props.selectedConversationId ? projectConversationIds.has(props.selectedConversationId) : false}
        isOpen={openProjectIds.has(project.id)}
        isConversationListExpanded={expandedConversationProjectIds.has(project.id)}
        isPinnedConversation={(conversationId) => props.pinnedConversationIds.has(conversationId)}
        key={project.id}
        locale={locale}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onProjectMenuAction={props.onProjectMenuAction}
        renderConversationRow={renderConversationRow}
        onSelectConversation={props.onSelectConversation}
        onToggleConversationList={toggleProjectConversations}
        onToggleOpen={toggleProjectOpen}
        project={project}
        selectedConversationId={props.selectedConversationId}
        sidebarState={sidebarState}
      />
    );
  });
  // renderSortableProjectSection renders one dnd-kit sortable project section.
  // renderSortableProjectSection 渲染一个 dnd-kit 可排序项目分区。
  const renderSortableProjectSection = (projects: ProjectSummary[]) => (
    <DndContext
      collisionDetection={closestCenter}
      onDragCancel={() => setActiveProjectDragId(null)}
      onDragEnd={(event) => handleProjectDragEnd(projects, event)}
      onDragStart={(event) => setActiveProjectDragId(String(event.active.id))}
      sensors={projectDragSensors}
    >
      <SortableContext items={projects.map((project) => project.id)} strategy={verticalListSortingStrategy}>
        {renderProjectRows(projects)}
      </SortableContext>
    </DndContext>
  );
  // renderConversationRow renders a sidebar conversation row outside or inside one project folder.
  // renderConversationRow 渲染项目内外复用的侧栏会话行。
  const renderConversationRow = (conversation: ConversationSummary, compactPinned = false) => (
    <ConversationTreeRow
      compactPinned={compactPinned}
      conversation={conversation}
      isPinned={props.pinnedConversationIds.has(conversation.id)}
      isSelected={props.selectedConversationId === conversation.id}
      key={conversation.id}
      locale={locale}
      nowMs={nowMs}
      onAction={props.onConversationMenuAction}
      onSelectConversation={props.onSelectConversation}
      project={conversation.projectId ? projectsById.get(conversation.projectId) : undefined}
    />
  );

  return (
    <Sidebar className={cn(activeProjectDragId && "project-drag-active")} collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <div aria-current="page" className="vcn-sidebar-static-item">
              <LayoutDashboard size={16} />
              <span className="vcn-sidebar-label strong">{t("sidebar.console")}</span>
            </div>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        {pinnedConversations.length > 0 || pinnedProjects.length > 0 ? (
          <SidebarGroup className="project-section pinned-project-section">
            {renderProjectGroupLabel(t("sidebar.pinned"), <Pin size={15} />)}
            <SidebarMenu className="pinned-tree-list">
              {pinnedConversations.map((conversation) => renderConversationRow(conversation, true))}
              {pinnedProjects.length > 0 ? renderSortableProjectSection(pinnedProjects) : null}
            </SidebarMenu>
          </SidebarGroup>
        ) : null}
        <SidebarGroup>
            {renderProjectGroupLabel(t("sidebar.projects"), <Boxes size={15} />, (
              <Button
                aria-label={t("sidebar.addProject")}
                className="project-add-button"
                onClick={props.onCreateProject}
                size="icon-sm"
                title={t("sidebar.addProject")}
                type="button"
                variant="ghost"
              >
                <FolderPlus size={15} />
              </Button>
            ))}
            <SidebarMenu>
              {renderSortableProjectSection(regularProjects)}
              {props.projects.length === 0 ? (
                <SidebarMenuItem>
                  <SidebarMenuButton className="muted" tooltip={t("sidebar.noProjects")}>
                    <Folder size={16} />
                    <span className="vcn-sidebar-label">
                      <strong>{t("sidebar.noProjects")}</strong>
                      <small>{t("sidebar.waitingWorkspace")}</small>
                    </span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ) : null}
            </SidebarMenu>
          </SidebarGroup>
      </SidebarContent>
    </Sidebar>
  );
}

// initialExpandedProjectIdSet restores project expansion, defaulting missing state to fully expanded.
// initialExpandedProjectIdSet 恢复项目展开状态，并在缺少状态时默认全部展开。
function initialExpandedProjectIdSet(
  projects: ProjectSummary[],
  expandedProjectIds: string[] | undefined,
): Set<string> {
  if (!expandedProjectIds) {
    return new Set(projects.map((project) => project.id));
  }
  const projectIds = new Set(projects.map((project) => project.id));
  return new Set(expandedProjectIds.filter((projectId) => projectIds.has(projectId)));
}

// expandedProjectIdsInProjectOrder serializes expanded project ids in the visible project order.
// expandedProjectIdsInProjectOrder 按可见项目顺序序列化已展开项目标识。
function expandedProjectIdsInProjectOrder(
  projects: ProjectSummary[],
  openProjectIds: Set<string>,
): string[] {
  return projects
    .filter((project) => openProjectIds.has(project.id))
    .map((project) => project.id);
}

// ConversationTreeRow renders one interactive conversation row with inline quick actions.
// ConversationTreeRow 渲染带行内快捷操作的单个交互式会话行。
function ConversationTreeRow(props: {
  // Conversation summary shown by the row.
  // 当前行展示的会话摘要。
  conversation: ConversationSummary;
  // Whether the row belongs to the pinned section.
  // 当前行是否属于置顶分区。
  compactPinned: boolean;
  // Whether the conversation is pinned.
  // 当前会话是否已置顶。
  isPinned: boolean;
  // Whether the conversation is currently selected.
  // 当前会话是否被选中。
  isSelected: boolean;
  // Active locale used for compact time labels.
  // 用于紧凑时间标签的当前语言。
  locale: LocaleCode;
  // Current timestamp used for relative time rendering.
  // 用于渲染相对时间的当前时间戳。
  nowMs: number;
  // Handles row-level menu and quick actions.
  // 处理行级菜单和快捷操作。
  onAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Selects this conversation.
  // 选择当前会话。
  onSelectConversation: (conversationId: string) => void;
  // Optional owning project used by pinned conversation rows.
  // 置顶会话行使用的可选所属项目。
  project?: ProjectSummary;
}) {
  // archiveConfirming tracks the inline archive confirmation state.
  // archiveConfirming 跟踪行内归档确认状态。
  const [archiveConfirming, setArchiveConfirming] = React.useState(false);
  const resetArchiveConfirming = React.useCallback(() => setArchiveConfirming(false), []);
  return (
    <SidebarMenuItem key={props.conversation.id} onMouseLeave={resetArchiveConfirming}>
      <ConversationContextMenu
        conversation={props.conversation}
        isPinned={props.isPinned}
        onAction={props.onAction}
      >
        <SidebarMenuButton
          active={props.isSelected}
          className={cn(
            "conversation-tree-button",
            props.compactPinned && "pinned-conversation-button",
            props.compactPinned && "with-pinned-icon",
          )}
          onClick={() => props.onSelectConversation(props.conversation.id)}
          tooltip={`${props.conversation.title}\n${props.conversation.status}`}
        >
          {props.compactPinned ? (
            <span className="conversation-tree-project-icon">
              <MessageSquareText size={14} />
            </span>
          ) : null}
          <span className="vcn-sidebar-label conversation-tree-label">
            <strong>{props.conversation.title}</strong>
          </span>
          <ConversationRowStatus
            archiveConfirming={archiveConfirming}
            conversation={props.conversation}
            isPinned={props.isPinned}
            isSelected={props.isSelected}
            locale={props.locale}
            nowMs={props.nowMs}
            onAction={props.onAction}
            onRequestArchiveConfirm={() => setArchiveConfirming(true)}
          />
        </SidebarMenuButton>
      </ConversationContextMenu>
    </SidebarMenuItem>
  );
}

// ConversationRowStatus renders time, unread, running state, and hover quick actions.
// ConversationRowStatus 渲染时间、未读、运行状态与悬停快捷操作。
function ConversationRowStatus(props: {
  // Whether inline archive confirmation is active.
  // 行内归档确认是否激活。
  archiveConfirming: boolean;
  // Conversation summary used to decide the row status indicator.
  // 用于决定行状态指示器的会话摘要。
  conversation: ConversationSummary;
  // Whether the conversation is pinned.
  // 当前会话是否置顶。
  isPinned: boolean;
  // Whether this conversation is currently selected.
  // 当前会话是否正在被选中。
  isSelected: boolean;
  // Active locale used for compact time labels.
  // 用于紧凑时间标签的当前语言。
  locale: LocaleCode;
  // Current timestamp used for relative time rendering.
  // 用于渲染相对时间的当前时间戳。
  nowMs: number;
  // Handles row-level quick actions.
  // 处理行级快捷操作。
  onAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Enters inline archive confirmation mode.
  // 进入行内归档确认模式。
  onRequestArchiveConfirm: () => void;
}) {
  const isRunning = props.conversation.status.toLowerCase() === "running";
  if (isRunning) {
    return (
      <span className="conversation-tree-trailing running">
        <span className="conversation-tree-status running">
          <span className="conversation-tree-status-mark" aria-label="会话正在运行">
            <Spinner className="size-3" />
          </span>
        </span>
        <span className="conversation-tree-quick-actions running" aria-hidden="false">
          <span
            aria-label={props.isPinned ? "取消置顶对话" : "置顶对话"}
            className="conversation-tree-quick-action"
            onClick={(event) => {
              event.stopPropagation();
              props.onAction("pin", props.conversation);
            }}
            onMouseDown={(event) => event.preventDefault()}
            role="button"
          >
            {props.isPinned ? <PinOff size={13} /> : <Pin size={13} />}
          </span>
        </span>
      </span>
    );
  }
  if (props.archiveConfirming) {
    return (
      <span
        className="conversation-tree-trailing confirming"
        onClick={(event) => event.stopPropagation()}
        onMouseDown={(event) => event.preventDefault()}
      >
        <span
          aria-label="确认归档对话"
          className="conversation-tree-confirm-action"
          onClick={(event) => {
            event.stopPropagation();
            props.onAction("archive", props.conversation);
          }}
          role="button"
        >
          确认
        </span>
      </span>
    );
  }
  return (
    <span className="conversation-tree-trailing">
      <span className="conversation-tree-status">
        {props.conversation.unread && !props.isSelected ? (
          <span className="conversation-tree-status-mark">
            <span className="conversation-tree-unread-dot" aria-label="未读" />
          </span>
        ) : (
          formatConversationRelativeTime(props.conversation, props.nowMs, props.locale)
        )}
      </span>
      <span className="conversation-tree-quick-actions" aria-hidden="false">
        <span
          aria-label={props.isPinned ? "取消置顶对话" : "置顶对话"}
          className="conversation-tree-quick-action"
          onClick={(event) => {
            event.stopPropagation();
            props.onAction("pin", props.conversation);
          }}
          onMouseDown={(event) => event.preventDefault()}
          role="button"
        >
          {props.isPinned ? <PinOff size={13} /> : <Pin size={13} />}
        </span>
        <span
          aria-label="归档对话"
          className="conversation-tree-quick-action"
          onClick={(event) => {
            event.stopPropagation();
            props.onRequestArchiveConfirm();
          }}
          onMouseDown={(event) => event.preventDefault()}
          role="button"
        >
          <Archive size={13} />
        </span>
      </span>
    </span>
  );
}

// formatConversationRelativeTime returns compact dynamic time text for sidebar rows.
// formatConversationRelativeTime 为侧栏行返回紧凑的动态时间文本。
function formatConversationRelativeTime(
  conversation: ConversationSummary,
  nowMs: number,
  locale: LocaleCode,
): string {
  const updatedAtMs = Number.parseInt(conversation.lastUpdatedAt ?? "", 10);
  if (Number.isFinite(updatedAtMs) && updatedAtMs > 0) {
    return formatCompactRelativeTime(nowMs - updatedAtMs, locale);
  }
  return compactPersistedRelativeLabel(conversation.lastUpdatedLabel || conversation.status, locale);
}

// formatCompactRelativeTime converts one elapsed duration into the compact tree label.
// formatCompactRelativeTime 将经过时长转换为紧凑树标签。
function formatCompactRelativeTime(elapsedMs: number, locale: LocaleCode): string {
  const safeElapsedMs = Math.max(0, elapsedMs);
  const minuteMs = 60_000;
  const hourMs = 60 * minuteMs;
  const dayMs = 24 * hourMs;
  const weekMs = 7 * dayMs;
  const monthMs = 30 * dayMs;
  const yearMs = 365 * dayMs;
  const chinese = locale === "zh-CN";
  if (safeElapsedMs < minuteMs) {
    return chinese ? "刚刚" : "now";
  }
  if (safeElapsedMs < hourMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / minuteMs));
    return chinese ? `${value}分` : `${value}m`;
  }
  if (safeElapsedMs < dayMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / hourMs));
    return chinese ? `${value}时` : `${value}h`;
  }
  if (safeElapsedMs < weekMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / dayMs));
    return chinese ? `${value}天` : `${value}d`;
  }
  if (safeElapsedMs < monthMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / weekMs));
    return chinese ? `${value}周` : `${value}w`;
  }
  if (safeElapsedMs < yearMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / monthMs));
    return `${value}M`;
  }
  const value = Math.max(1, Math.floor(safeElapsedMs / yearMs));
  return `${value}Y`;
}

// compactPersistedRelativeLabel compresses older static labels when no raw timestamp exists.
// compactPersistedRelativeLabel 在没有原始时间戳时压缩旧的静态标签。
function compactPersistedRelativeLabel(label: string, locale: LocaleCode): string {
  const trimmed = label.trim();
  if (!trimmed) {
    return "";
  }
  if (/^(刚刚|now)$/i.test(trimmed)) {
    return locale === "zh-CN" ? "刚刚" : "now";
  }
  const match = trimmed.match(/(\d+)\s*(分|分钟|minute|minutes|min|m|时|小时|hour|hours|h|天|日|day|days|d|周|星期|week|weeks|w|月|month|months|M|年|year|years|Y)/i);
  if (!match) {
    return trimmed;
  }
  const value = Number.parseInt(match[1], 10);
  const rawUnit = match[2];
  const unit = rawUnit.toLowerCase();
  const chinese = locale === "zh-CN";
  if (rawUnit === "M" || unit === "月" || unit === "month" || unit === "months") {
    return `${value}M`;
  }
  if (rawUnit === "Y" || unit === "年" || unit === "year" || unit === "years" || unit === "y") {
    return `${value}Y`;
  }
  if (unit === "分" || unit === "分钟" || unit === "minute" || unit === "minutes" || unit === "min" || unit === "m") {
    return chinese ? `${value}分` : `${value}m`;
  }
  if (unit === "时" || unit === "小时" || unit === "hour" || unit === "hours" || unit === "h") {
    return chinese ? `${value}时` : `${value}h`;
  }
  if (unit === "天" || unit === "日" || unit === "day" || unit === "days" || unit === "d") {
    return chinese ? `${value}天` : `${value}d`;
  }
  if (unit === "周" || unit === "星期" || unit === "week" || unit === "weeks" || unit === "w") {
    return chinese ? `${value}周` : `${value}w`;
  }
  return trimmed;
}

// SortableProjectRow renders one dnd-kit sortable project folder and its conversations.
// SortableProjectRow 渲染一个 dnd-kit 可排序项目文件夹及其会话。
function SortableProjectRow(props: {
  // Project represented by this sortable row.
  // 当前可排序行代表的项目。
  project: ProjectSummary;
  // Conversations that belong to the project.
  // 归属于当前项目的会话列表。
  conversations: ConversationSummary[];
  // Whether the project folder is expanded.
  // 当前项目文件夹是否展开。
  isOpen: boolean;
  // Whether every conversation under this project should be displayed.
  // 当前项目下是否显示全部会话。
  isConversationListExpanded: boolean;
  // Whether the selected conversation belongs to this project.
  // 当前选中会话是否归属于该项目。
  hasSelectedConversation: boolean;
  // Current sidebar collapsed or expanded state.
  // 当前侧栏的折叠或展开状态。
  sidebarState: ReturnType<typeof useSidebar>["state"];
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Active locale used for empty labels.
  // 用于空状态标签的当前语言。
  locale: LocaleCode;
  // Checks whether a conversation is pinned.
  // 检查某个会话是否置顶。
  isPinnedConversation: (conversationId: string) => boolean;
  // Toggles the project folder expansion.
  // 切换项目文件夹展开状态。
  onToggleOpen: (projectId: string) => void;
  // Toggles the conversation list overflow rows for this project.
  // 切换当前项目会话列表的溢出行。
  onToggleConversationList: (projectId: string) => void;
  // Opens a new draft conversation for the project.
  // 为项目打开一个新的草稿会话。
  onCreateConversation: (project: ProjectSummary) => void;
  // Handles project context menu actions.
  // 处理项目右键菜单动作。
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  // Handles conversation context menu actions.
  // 处理会话右键菜单动作。
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Selects one conversation.
  // 选择一个会话。
  onSelectConversation: (conversationId: string) => void;
  // Renders one conversation row with the shared sidebar style.
  // 使用共享侧栏样式渲染单个会话行。
  renderConversationRow: (conversation: ConversationSummary, compactPinned?: boolean) => React.ReactNode;
}) {
  const sortable = useVerticalSortableItem(props.project.id);
  const folderName = projectFolderName(props.project.path);
  const showOriginalFolderName = folderName.length > 0 && folderName !== props.project.name;
  const conversationPreviewLimit = 5;
  const hasConversationOverflow = props.conversations.length > conversationPreviewLimit;
  const visibleConversations = hasConversationOverflow && !props.isConversationListExpanded
    ? props.conversations.slice(0, conversationPreviewLimit)
    : props.conversations;
  const conversationOverflowLabel = props.isConversationListExpanded
    ? props.locale === "zh-CN" ? "折叠显示" : "Collapse"
    : props.locale === "zh-CN" ? "展开显示" : "Show more";
  return (
    <SidebarMenuItem
      className={cn("project-tree-item", sortable.isDragging && "drag-source")}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
    >
      {props.sidebarState === "collapsed" ? (
        <CollapsedProjectMenu
          conversations={props.conversations}
          onCreateConversation={props.onCreateConversation}
          onSelectConversation={props.onSelectConversation}
          project={props.project}
          hasSelectedConversation={props.hasSelectedConversation}
          selectedConversationId={props.selectedConversationId}
        />
      ) : (
        <div className={cn("project-folder-row", sortable.isDragging && "drag-source-row")} {...sortable.attributes} {...sortable.listeners}>
          <ProjectContextMenu project={props.project} onAction={props.onProjectMenuAction}>
            <SidebarMenuButton
              aria-expanded={props.isOpen}
              className={cn("project-folder-button", props.hasSelectedConversation && "project-folder-active")}
              onClick={() => props.onToggleOpen(props.project.id)}
              tooltip={`${props.project.name}\n${props.project.path}`}
            >
              <ProjectIcon isOpen={props.isOpen} project={props.project} size={16} />
              <span className="vcn-sidebar-label project-folder-label">
                <strong>{props.project.name}</strong>
                {showOriginalFolderName ? <small>{folderName}</small> : null}
              </span>
              <span className="project-folder-arrow" aria-hidden="true">
                {props.isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
              </span>
            </SidebarMenuButton>
          </ProjectContextMenu>
          <SidebarMenuAction
            aria-label={`在 ${props.project.name} 中创建新会话`}
            className="project-create-conversation"
            onClick={(event) => {
              event.stopPropagation();
              props.onCreateConversation(props.project);
            }}
            data-project-row-action
            showOnHover
            title="新建会话"
            type="button"
          >
            <SquarePen size={13} />
          </SidebarMenuAction>
        </div>
      )}
      {props.isOpen && props.sidebarState !== "collapsed" ? (
        <SidebarMenu className="project-conversation-list">
          {props.conversations.length > 0 ? (
            <>
              {visibleConversations.map((conversation) => props.renderConversationRow(conversation))}
              {hasConversationOverflow ? (
                <SidebarMenuItem>
                  <button
                    className="conversation-list-toggle"
                    onClick={(event) => {
                      event.stopPropagation();
                      props.onToggleConversationList(props.project.id);
                    }}
                    type="button"
                  >
                    {conversationOverflowLabel}
                  </button>
                </SidebarMenuItem>
              ) : null}
            </>
          ) : (
            <SidebarMenuItem>
              <div className="conversation-empty-row">{props.locale === "zh-CN" ? "暂无对话" : "No conversations"}</div>
            </SidebarMenuItem>
          )}
        </SidebarMenu>
      ) : null}
    </SidebarMenuItem>
  );
}

// CollapsedProjectMenu renders project actions as a hover flyout while the sidebar is collapsed.
// CollapsedProjectMenu 在侧栏折叠时以悬停浮层展示项目操作。
function CollapsedProjectMenu(props: {
  // Project that owns the collapsed entry.
  // 折叠入口所属项目。
  project: ProjectSummary;
  // Whether the selected conversation belongs to this project.
  // 当前选中会话是否归属于该项目。
  hasSelectedConversation: boolean;
  // Conversations shown below the create command.
  // 显示在新建命令下方的会话列表。
  conversations: ConversationSummary[];
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Opens a new draft conversation for the project.
  // 为项目打开一个新的草稿会话。
  onCreateConversation: (project: ProjectSummary) => void;
  // Selects one existing conversation.
  // 选择一个已有会话。
  onSelectConversation: (conversationId: string) => void;
}) {
  // anchorRef points at the collapsed folder row used to place the hover flyout.
  // anchorRef 指向折叠后的文件夹行，用于定位悬停浮层。
  const anchorRef = React.useRef<HTMLDivElement | null>(null);
  // closeTimerRef keeps the flyout alive while the pointer moves from folder to menu.
  // closeTimerRef 在指针从文件夹移动到菜单时保持浮层不被立刻关闭。
  const closeTimerRef = React.useRef<number | null>(null);
  // flyoutOpen tracks whether the hover flyout is currently visible.
  // flyoutOpen 跟踪悬停浮层当前是否可见。
  const [flyoutOpen, setFlyoutOpen] = React.useState(false);
  // flyoutPosition stores viewport coordinates so the menu is not clipped by sidebar scrolling.
  // flyoutPosition 保存视口坐标，避免菜单被侧栏滚动容器裁剪。
  const [flyoutPosition, setFlyoutPosition] = React.useState<{ left: number; top: number } | null>(null);

  // clearCloseTimer cancels a pending delayed close when the pointer returns.
  // clearCloseTimer 在指针返回时取消等待中的延迟关闭。
  const clearCloseTimer = React.useCallback(() => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  // openFlyout positions and shows the collapsed project flyout.
  // openFlyout 定位并显示折叠项目浮层。
  const openFlyout = React.useCallback(() => {
    clearCloseTimer();
    const rect = anchorRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    setFlyoutPosition({
      left: rect.right + 8,
      top: rect.top,
    });
    setFlyoutOpen(true);
  }, [clearCloseTimer]);

  // scheduleClose delays closing so pointer travel between folder and menu feels stable.
  // scheduleClose 延迟关闭浮层，让指针在文件夹与菜单间移动时保持稳定。
  const scheduleClose = React.useCallback(() => {
    clearCloseTimer();
    closeTimerRef.current = window.setTimeout(() => {
      setFlyoutOpen(false);
    }, 140);
  }, [clearCloseTimer]);

  React.useEffect(() => clearCloseTimer, [clearCloseTimer]);

  // selectConversation closes the flyout before switching to a conversation.
  // selectConversation 在切换会话前关闭浮层。
  const selectConversation = React.useCallback(
    (conversationId: string) => {
      setFlyoutOpen(false);
      props.onSelectConversation(conversationId);
    },
    [props],
  );

  // createConversation closes the flyout before opening a draft conversation.
  // createConversation 在打开草稿会话前关闭浮层。
  const createConversation = React.useCallback(() => {
    setFlyoutOpen(false);
    props.onCreateConversation(props.project);
  }, [props]);

  return (
    <>
      <div
        className="collapsed-project-hover-root"
        onMouseEnter={openFlyout}
        onMouseLeave={scheduleClose}
        ref={anchorRef}
      >
        <SidebarMenuButton
          aria-label={`${props.project.name} 会话菜单`}
          className={cn(
            "project-folder-button collapsed-project-button",
            props.hasSelectedConversation && "project-folder-active",
          )}
          onClick={(event) => event.preventDefault()}
        >
          <ProjectIcon project={props.project} size={16} />
          <span className="vcn-sidebar-label project-folder-label">
            <strong>{props.project.name}</strong>
          </span>
        </SidebarMenuButton>
      </div>
      {flyoutOpen && flyoutPosition ? (
        <div
          aria-label={`${props.project.name} 会话`}
          className="collapsed-project-flyout"
          onMouseEnter={openFlyout}
          onMouseLeave={scheduleClose}
          role="menu"
          style={{ left: flyoutPosition.left, top: flyoutPosition.top }}
        >
          <div className="collapsed-project-flyout-title" role="presentation">
            {props.project.name}
          </div>
          <button className="collapsed-project-flyout-item" onClick={createConversation} role="menuitem" type="button">
            <SquarePen size={15} />
            <span>新建会话</span>
          </button>
          <div className="collapsed-project-flyout-separator" role="separator" />
          {props.conversations.length > 0 ? (
            props.conversations.map((conversation) => (
              <button
                className={cn(
                  "collapsed-project-flyout-item",
                  props.selectedConversationId === conversation.id && "active",
                )}
                key={conversation.id}
                onClick={() => selectConversation(conversation.id)}
                role="menuitem"
                type="button"
              >
                <span className="collapsed-project-menu-title">{conversation.title}</span>
              </button>
            ))
          ) : (
            <div className="collapsed-project-flyout-item disabled" role="presentation">
              <span>暂无会话</span>
            </div>
          )}
        </div>
      ) : null}
    </>
  );
}

// ProjectContextMenu renders project-level right-click commands.
// ProjectContextMenu 渲染项目级右键命令。
function ProjectContextMenu(props: {
  project: ProjectSummary;
  children: React.ReactNode;
  onAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
}) {
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{props.children}</ContextMenuTrigger>
      <ContextMenuContent className="project-context-menu">
        <ContextMenuItem onSelect={() => props.onAction("toggle-pin", props.project)}>
          <PinOff size={15} />
          <span>{props.project.pinned ? "取消置顶项目" : "置顶项目"}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("open-directory", props.project)}>
          <FolderOpen size={15} />
          <span>在资源管理器中打开</span>
        </ContextMenuItem>
        <ContextMenuItem disabled>
          <PanelTopOpen size={15} />
          <span>创建永久工作树</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("rename", props.project)}>
          <Pencil size={15} />
          <span>重命名项目</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("set-icon", props.project)}>
          <Palette size={15} />
          <span>设置项目图标</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("archive-conversations", props.project)}>
          <Archive size={15} />
          <span>归档对话</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled variant="destructive">
          <Minus size={15} />
          <span>移除</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => props.onAction("settings", props.project)}>
          <FolderCog size={15} />
          <span>项目设置</span>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

// RenameDialog renders the controlled project or conversation rename form.
// RenameDialog 渲染受控的项目或会话重命名表单。
function RenameDialog(props: {
  target: RenameTarget | null;
  onCancel: () => void;
  onConfirm: (target: RenameTarget, name: string) => void;
}) {
  // value stores the pending display name edited by the user.
  // value 保存用户正在编辑的显示名称。
  const [value, setValue] = React.useState("");

  React.useEffect(() => {
    setValue(props.target?.name ?? "");
  }, [props.target]);

  const title = props.target?.kind === "project" ? "重命名项目" : "重命名对话";
  const description = props.target?.kind === "project"
    ? "输入新的项目显示名称。项目路径不会改变；留空确认会还原为项目文件夹名。"
    : "输入新的对话名称。";

  return (
    <Dialog open={Boolean(props.target)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent className="rename-dialog">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <form
          className="dialog-form"
          onSubmit={(event) => {
            event.preventDefault();
            if (props.target) {
              props.onConfirm(props.target, value);
            }
          }}
        >
          <Input
            autoFocus
            value={value}
            onChange={(event) => setValue(event.target.value)}
            placeholder="输入新的名称"
          />
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={props.onCancel}>取消</Button>
            <Button type="submit">确认</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ProjectIconDialog renders the project icon picker and generated badge editor.
// ProjectIconDialog 渲染项目图标选择器与生成式徽标编辑器。
function ProjectIconDialog(props: {
  // Project icon target currently edited by this dialog.
  // 当前弹窗正在编辑的项目图标目标。
  target: ProjectIconDialogTarget | null;
  // Cancels the current icon edit.
  // 取消当前图标编辑。
  onCancel: () => void;
  // Persists the selected icon.
  // 持久化选中的图标。
  onConfirm: (project: ProjectSummary, icon: ProjectIconSelection) => void;
  // Resets the selected project to default folder icons.
  // 将选中项目还原为默认文件夹图标。
  onReset: (project: ProjectSummary) => void;
}) {
  const project = props.target?.project ?? null;
  // selectedIcon stores the currently selected built-in icon.
  // selectedIcon 保存当前选中的内置图标。
  const [selectedIcon, setSelectedIcon] = React.useState<ProjectIconSelection>({ pack: "lucide", key: "folder" });
  // badgeConfig stores the generated badge draft.
  // badgeConfig 保存生成式徽标草稿。
  const [badgeConfig, setBadgeConfig] = React.useState<ProjectIconBadgeConfig>(DEFAULT_PROJECT_BADGE_CONFIG);
  // activeTab stores the visible picker tab.
  // activeTab 保存当前可见的选择器标签。
  const [activeTab, setActiveTab] = React.useState<ProjectIconTab>("skill-icons");
  // iconSearch stores the Lucide icon filter text.
  // iconSearch 保存当前图标包的过滤文本。
  const [iconSearch, setIconSearch] = React.useState("");
  // visibleIconLimit stores the streamed icon render limit.
  // visibleIconLimit 保存图标流式渲染上限。
  const [visibleIconLimit, setVisibleIconLimit] = React.useState(PROJECT_ICON_STREAM_BATCH_SIZE);

  React.useEffect(() => {
    if (!project) {
      return;
    }
    if (project.iconPack === "badge") {
      setActiveTab("badge");
      setBadgeConfig(decodeProjectBadgeKey(project.iconKey) ?? {
        ...DEFAULT_PROJECT_BADGE_CONFIG,
        text: normalizeProjectBadgeText(project.name) || DEFAULT_PROJECT_BADGE_CONFIG.text,
      });
      return;
    }
    if (isIconifyProjectIconPack(project.iconPack) && project.iconKey) {
      setActiveTab(project.iconPack);
      setSelectedIcon({ pack: project.iconPack, key: project.iconKey });
      setIconSearch("");
      setVisibleIconLimit(PROJECT_ICON_STREAM_BATCH_SIZE);
      setBadgeConfig({
        ...DEFAULT_PROJECT_BADGE_CONFIG,
        text: normalizeProjectBadgeText(project.name) || DEFAULT_PROJECT_BADGE_CONFIG.text,
      });
      return;
    }
    const key = project.iconPack === "lucide" && project.iconKey && PROJECT_ICON_MAP.has(project.iconKey)
      ? project.iconKey
      : "Folder";
    setActiveTab("lucide");
    setSelectedIcon({ pack: "lucide", key });
    setIconSearch("");
    setVisibleIconLimit(PROJECT_ICON_STREAM_BATCH_SIZE);
    setBadgeConfig({
      ...DEFAULT_PROJECT_BADGE_CONFIG,
      text: normalizeProjectBadgeText(project.name) || DEFAULT_PROJECT_BADGE_CONFIG.text,
    });
  }, [project]);

  React.useEffect(() => {
    setVisibleIconLimit(PROJECT_ICON_STREAM_BATCH_SIZE);
  }, [activeTab, iconSearch]);

  // updateBadgeText applies the Chinese-one-or-English-two text rule.
  // updateBadgeText 应用中文一个字或英文两个字母的文字规则。
  const updateBadgeText = React.useCallback((value: string) => {
    setBadgeConfig((current) => ({
      ...current,
      text: normalizeProjectBadgeText(value),
    }));
  }, []);

  // handleDialogInteractOutside keeps nested Radix select popovers from closing this dialog.
  // handleDialogInteractOutside 避免嵌套的 Radix 选择器浮层被弹窗误判为外部点击而关闭。
  const handleDialogInteractOutside = React.useCallback((event: Event) => {
    const target = event.target instanceof HTMLElement ? event.target : null;
    if (!target) {
      return;
    }
    if (target.closest("[data-slot='select-content'], [data-slot='select-trigger']")) {
      event.preventDefault();
    }
  }, []);

  if (!project) {
    return null;
  }

  const selectedBadgeIcon: ProjectIconSelection = {
    pack: "badge",
    key: encodeProjectBadgeKey(badgeConfig),
  };
  const finalSelection = activeTab === "badge" ? selectedBadgeIcon : selectedIcon;
  const normalizedSearch = iconSearch.trim().toLowerCase();
  const activeIconOptions = projectIconOptionsForTab(activeTab);
  const visibleIconOptions = normalizedSearch.length === 0
    ? activeIconOptions
    : activeIconOptions.filter((option) => option.label.toLowerCase().includes(normalizedSearch));
  const streamedIconOptions = visibleIconOptions.slice(0, visibleIconLimit);
  const hasMoreIcons = streamedIconOptions.length < visibleIconOptions.length;

  // handleIconGridScroll streams the next icon batch when the user reaches the list bottom.
  // handleIconGridScroll 在用户滚动到列表底部时流式追加下一批图标。
  const handleIconGridScroll = (event: React.UIEvent<HTMLDivElement>) => {
    const element = event.currentTarget;
    if (!hasMoreIcons) {
      return;
    }
    const distanceFromBottom = element.scrollHeight - element.scrollTop - element.clientHeight;
    if (distanceFromBottom > 96) {
      return;
    }
    setVisibleIconLimit((current) => Math.min(current + PROJECT_ICON_STREAM_BATCH_SIZE, visibleIconOptions.length));
  };

  return (
    <Dialog open={Boolean(project)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent
        className="project-icon-dialog sm:max-w-[720px]"
        onInteractOutside={handleDialogInteractOutside}
      >
        <DialogHeader>
          <DialogTitle>设置项目图标</DialogTitle>
          <DialogDescription>
            为 {project.name} 选择固定图标，或生成一个字母徽标。还原后会重新使用默认文件夹开关图标。
          </DialogDescription>
        </DialogHeader>
        <div className="project-icon-layout">
          <div className="project-icon-preview" aria-label="项目图标预览">
            <ProjectIcon project={{ ...project, iconPack: finalSelection.pack, iconKey: finalSelection.key }} size={42} />
            <strong>{project.name}</strong>
          </div>
            <Tabs className="project-icon-tabs" value={activeTab} onValueChange={(value) => setActiveTab(value as ProjectIconTab)}>
              <TabsList className="project-icon-tabs-list">
                <TabsTrigger value="skill-icons">Skill Icons</TabsTrigger>
                <TabsTrigger value="material-icon-theme">Material Icon Theme</TabsTrigger>
                <TabsTrigger value="lucide">Lucide</TabsTrigger>
                <TabsTrigger value="badge">自定义</TabsTrigger>
              </TabsList>
              {(["skill-icons", "material-icon-theme", "lucide"] as ProjectIconPack[]).map((pack) => (
                <TabsContent className="project-icon-tab-content" key={pack} value={pack}>
                  {activeTab === pack ? (
                    <>
                      <div className="project-icon-filter">
                        <Input
                          onChange={(event) => setIconSearch(event.target.value)}
                          placeholder={`过滤 ${projectIconPackLabel(pack)} 图标`}
                          value={iconSearch}
                        />
                        <span>{streamedIconOptions.length} / {visibleIconOptions.length} / {activeIconOptions.length}</span>
                      </div>
                      <div className="project-icon-grid picker-icon-grid" onScroll={handleIconGridScroll}>
                        {streamedIconOptions.map((option) => (
                          <button
                            aria-pressed={selectedIcon.pack === option.pack && selectedIcon.key === option.key}
                            className={cn(
                              "project-icon-option",
                              option.pack !== "lucide" && "color-icon-option",
                              selectedIcon.pack === option.pack && selectedIcon.key === option.key && "selected",
                            )}
                            key={`${option.pack}:${option.key}`}
                            onClick={() => setSelectedIcon({ pack: option.pack, key: option.key })}
                            title={option.label}
                            type="button"
                          >
                            {option.iconName ? (
                              <IconifyIcon aria-hidden="true" height={26} icon={option.iconName} width={26} />
                            ) : option.Icon ? (
                              <option.Icon size={22} />
                            ) : null}
                            <span>{option.label}</span>
                          </button>
                        ))}
                        {hasMoreIcons ? (
                          <button
                            className="project-icon-option icon-load-more"
                            onClick={() => setVisibleIconLimit((current) => Math.min(current + PROJECT_ICON_STREAM_BATCH_SIZE, visibleIconOptions.length))}
                            type="button"
                          >
                            加载更多
                          </button>
                        ) : null}
                      </div>
                    </>
                  ) : null}
                </TabsContent>
              ))}
            <TabsContent className="project-icon-tab-content badge-tab" value="badge">
              <div className="badge-editor">
                <label>
                  <span>文字</span>
                  <Input
                    maxLength={4}
                    onChange={(event) => updateBadgeText(event.target.value)}
                    placeholder="中文 1 个或英文 2 个"
                    value={badgeConfig.text}
                  />
                </label>
                <div className="badge-editor-field">
                  <span>形状</span>
                  <Select
                    value={badgeConfig.shape}
                    onValueChange={(shape) => setBadgeConfig((current) => ({ ...current, shape: shape as ProjectIconBadgeShape }))}
                  >
                    <SelectTrigger className="badge-shape-trigger">
                      <span>{badgeShapeLabel(badgeConfig.shape)}</span>
                    </SelectTrigger>
                    <SelectContent className="project-icon-select-content" position="popper">
                      <SelectItem value="circle">圆形</SelectItem>
                      <SelectItem value="square">方形</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <ColorPickerRow
                  label="背景色"
                  value={badgeConfig.background}
                  onChange={(background) => setBadgeConfig((current) => ({ ...current, background }))}
                />
                <ColorPickerRow
                  label="边框色"
                  value={badgeConfig.border}
                  onChange={(border) => setBadgeConfig((current) => ({ ...current, border }))}
                />
                <ColorPickerRow
                  label="字体色"
                  value={badgeConfig.foreground}
                  onChange={(foreground) => setBadgeConfig((current) => ({ ...current, foreground }))}
                />
              </div>
            </TabsContent>
          </Tabs>
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={props.onCancel}>取消</Button>
          <Button type="button" variant="outline" onClick={() => props.onReset(project)}>还原默认</Button>
          <Button type="button" onClick={() => props.onConfirm(project, finalSelection)}>设置</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ColorPickerRow renders one color input with quick color swatches.
// ColorPickerRow 渲染一个带快捷色块的颜色输入。
function ColorPickerRow(props: {
  // User-facing color field label.
  // 面向用户显示的颜色字段名称。
  label: string;
  // Current hex color.
  // 当前十六进制颜色。
  value: string;
  // Updates the current hex color.
  // 更新当前十六进制颜色。
  onChange: (value: string) => void;
}) {
  return (
    <div className="color-picker-row">
      <span>{props.label}</span>
      <input
        aria-label={props.label}
        className="color-picker-input"
        onChange={(event) => props.onChange(event.target.value)}
        type="color"
        value={props.value}
      />
      <div className="color-swatch-row">
        {PROJECT_BADGE_COLORS.map((color) => (
          <button
            aria-label={`${props.label} ${color}`}
            className="color-swatch"
            key={color}
            onClick={() => props.onChange(color)}
            style={{ backgroundColor: color }}
            type="button"
          />
        ))}
      </div>
    </div>
  );
}

// badgeShapeLabel returns the Chinese label for a generated badge shape.
// badgeShapeLabel 返回生成式徽标形状的中文标签。
function badgeShapeLabel(shape: ProjectIconBadgeShape): string {
  switch (shape) {
    case "circle":
      return "圆形";
    case "square":
      return "方形";
  }
}

// ProjectSettingsSectionId names the left-side categories in the project settings dialog.
// ProjectSettingsSectionId 命名项目设置弹窗左侧分类。
type ProjectSettingsSectionId = "overview" | "vmm" | "agent-skills" | "attachments";

// ProjectSettingsSectionSummary describes one navigable settings category.
// ProjectSettingsSectionSummary 描述单个可导航的设置分类。
interface ProjectSettingsSectionSummary {
  // Stable section identifier.
  // 稳定分类标识。
  id: ProjectSettingsSectionId;
  // Icon rendered in the left navigation.
  // 左侧导航中渲染的图标。
  icon: React.ReactNode;
  // Human-facing section label.
  // 面向用户的分类名称。
  label: string;
  // Compact section description.
  // 紧凑分类说明。
  detail: string;
}

// ProjectAttachmentCleanupKinds stores the project cleanup type filters.
// ProjectAttachmentCleanupKinds 保存项目清理类型筛选。
interface ProjectAttachmentCleanupKinds {
  // Whether managed image attachments are included.
  // 是否包含托管图片附件。
  image: boolean;
  // Whether managed audio attachments are included.
  // 是否包含托管音频附件。
  audio: boolean;
  // Whether managed video attachments are included.
  // 是否包含托管视频附件。
  video: boolean;
  // Whether managed PDF attachments are included.
  // 是否包含托管 PDF 附件。
  pdf: boolean;
}

// ProjectAgentSkillState names one project-level Agent Skill tri-state.
// ProjectAgentSkillState 命名单个项目级 Agent Skill 三态。
type ProjectAgentSkillState = "inherit" | "enabled" | "disabled";

// ProjectAttachmentCleanupRequest describes one project attachment cleanup command.
// ProjectAttachmentCleanupRequest 描述单个项目附件清理命令。
interface ProjectAttachmentCleanupRequest {
  // Stable project identifier owning the cleanup command.
  // 拥有清理命令的稳定项目标识。
  projectId: string;
  // Local cutoff date in YYYY-MM-DD format.
  // YYYY-MM-DD 格式的本地截止日期。
  cutoffDate: string;
  // Attachment-kind filters selected by the user.
  // 用户选择的附件类型筛选。
  selection: ProjectAttachmentCleanupKinds;
}

// ProjectAttachmentCleanupPreviewSummary describes a cleanup preview returned by the service.
// ProjectAttachmentCleanupPreviewSummary 描述服务返回的清理预览。
interface ProjectAttachmentCleanupPreviewSummary {
  // Cutoff date applied by the preview.
  // 预览使用的截止日期。
  cutoffDate: string;
  // Total matched attachment asset count.
  // 匹配的附件资产总数。
  matchedAssetCount: number;
  // Human-readable matched byte count.
  // 匹配字节数的人类可读文本。
  matchedByteLenLabel: string;
  // Unique session count impacted by matched assets.
  // 受匹配资产影响的唯一会话数量。
  impactedSessionCount: number;
  // Per-kind matched asset counts.
  // 按类型统计的匹配资产数。
  perKindCounts: Record<string, number>;
  // One-line user-facing summary.
  // 面向用户的一行摘要。
  summary: string;
}

// ProjectAttachmentCleanupReportSummary describes a cleanup execution report returned by the service.
// ProjectAttachmentCleanupReportSummary 描述服务返回的清理执行报告。
interface ProjectAttachmentCleanupReportSummary {
  // Cutoff date applied by the execution.
  // 执行使用的截止日期。
  cutoffDate: string;
  // Total purged attachment asset count.
  // 被清理的附件资产总数。
  purgedAssetCount: number;
  // Total deleted managed file count.
  // 已删除的托管文件总数。
  deletedFileCount: number;
  // Total managed files already missing before cleanup.
  // 清理前已经缺失的托管文件总数。
  alreadyMissingFileCount: number;
  // Total managed files that could not be deleted.
  // 未能删除的托管文件总数。
  failedDeleteCount: number;
  // Human-readable purged byte count.
  // 清理字节数的人类可读文本。
  purgedByteLenLabel: string;
  // Unique session count impacted by purged assets.
  // 受清理资产影响的唯一会话数量。
  impactedSessionCount: number;
  // One-line user-facing summary.
  // 面向用户的一行摘要。
  summary: string;
}

// ProjectSettingsDialog renders project-level settings migrated from the iced workbench.
// ProjectSettingsDialog 渲染从 iced 工作台迁移来的项目级设置。
function ProjectSettingsDialog(props: {
  // Project currently edited by this dialog.
  // 当前弹窗正在编辑的项目。
  target: ProjectSettingsDialogTarget | null;
  // Workbench snapshot supplying catalogs and counters.
  // 提供目录与计数的工作台快照。
  workbench: WorkbenchSnapshot;
  // Closes the dialog without forcing a save.
  // 关闭弹窗且不强制保存。
  onCancel: () => void;
  // Loads project Agent Skill overrides.
  // 加载项目 Agent Skill 覆盖。
  onLoadAgentSkillOverrides: (projectId: string) => Promise<Record<string, string>>;
  // Loads the project VMM draft.
  // 加载项目 VMM 草稿。
  onLoadVmmDraft: (projectId: string) => Promise<VmmConfigDraftSummary>;
  // Saves the project VMM draft.
  // 保存项目 VMM 草稿。
  onSaveVmmDraft: (projectId: string, draft: VmmConfigDraftSummary) => Promise<void>;
  // Saves one project Agent Skill tri-state override.
  // 保存单个项目 Agent Skill 三态覆盖。
  onSetAgentSkillState: (projectId: string, name: string, stateKey: ProjectAgentSkillState) => Promise<void>;
}) {
  const project = props.target?.project ?? null;
  // activeSection stores the selected settings category.
  // activeSection 保存当前选中的设置分类。
  const [activeSection, setActiveSection] = React.useState<ProjectSettingsSectionId>("overview");
  // vmmDraft stores the editable project-level VMM configuration.
  // vmmDraft 保存可编辑的项目级 VMM 配置。
  const [vmmDraft, setVmmDraft] = React.useState<VmmConfigDraftSummary | null>(null);
  // vmmBusy stores the current VMM load or save phase.
  // vmmBusy 保存当前 VMM 加载或保存阶段。
  const [vmmBusy, setVmmBusy] = React.useState<"idle" | "loading" | "saving">("idle");
  // agentOverrides stores project-local Agent Skill override states.
  // agentOverrides 保存项目本地 Agent Skill 覆盖状态。
  const [agentOverrides, setAgentOverrides] = React.useState<Record<string, string>>({});
  // agentLoading records whether the Agent Skill override map is loading.
  // agentLoading 记录 Agent Skill 覆盖映射是否正在加载。
  const [agentLoading, setAgentLoading] = React.useState(false);
  // agentBusySkill stores the Agent Skill currently being saved.
  // agentBusySkill 保存当前正在保存的 Agent Skill。
  const [agentBusySkill, setAgentBusySkill] = React.useState<string | null>(null);
  // cleanupKinds stores selected attachment families for cleanup preview.
  // cleanupKinds 保存附件清理预览选择的附件类型。
  const [cleanupKinds, setCleanupKinds] = React.useState<ProjectAttachmentCleanupKinds>({
    image: true,
    audio: true,
    video: true,
    pdf: true,
  });
  // cleanupCutoffDate stores the project cleanup cutoff date text.
  // cleanupCutoffDate 保存项目清理截止日期文本。
  const [cleanupCutoffDate, setCleanupCutoffDate] = React.useState("");
  // cleanupNotice stores the modal-local attachment cleanup status.
  // cleanupNotice 保存弹窗内的附件清理状态。
  const [cleanupNotice, setCleanupNotice] = React.useState("选择截止日期和附件类型后可预览当前项目的托管附件清理范围。");
  // cleanupBusy stores the current attachment cleanup phase.
  // cleanupBusy 保存当前附件清理阶段。
  const [cleanupBusy, setCleanupBusy] = React.useState<"idle" | "preview" | "execute">("idle");
  // cleanupPreview stores the latest successful cleanup preview.
  // cleanupPreview 保存最近一次成功的清理预览。
  const [cleanupPreview, setCleanupPreview] = React.useState<ProjectAttachmentCleanupPreviewSummary | null>(null);
  // cleanupReport stores the latest successful cleanup execution report.
  // cleanupReport 保存最近一次成功的清理执行报告。
  const [cleanupReport, setCleanupReport] = React.useState<ProjectAttachmentCleanupReportSummary | null>(null);

  React.useEffect(() => {
    if (!project) {
      return;
    }
    let cancelled = false;
    setActiveSection("overview");
    setVmmDraft(null);
    setVmmBusy("loading");
    setAgentOverrides({});
    setAgentLoading(true);
    setAgentBusySkill(null);
    setCleanupKinds({ image: true, audio: true, video: true, pdf: true });
    setCleanupCutoffDate(new Date().toISOString().slice(0, 10));
    setCleanupNotice("选择截止日期和附件类型后可预览当前项目的托管附件清理范围。");
    setCleanupBusy("idle");
    setCleanupPreview(null);
    setCleanupReport(null);

    void props.onLoadVmmDraft(project.id)
      .then((draft) => {
        if (!cancelled) {
          setVmmDraft(draft);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          toast.error(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setVmmBusy("idle");
        }
      });

    void props.onLoadAgentSkillOverrides(project.id)
      .then((overrides) => {
        if (!cancelled) {
          setAgentOverrides(overrides);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          toast.error(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setAgentLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [project, props.onLoadAgentSkillOverrides, props.onLoadVmmDraft]);

  if (!project) {
    return null;
  }

  const sections = projectSettingsSections(project, props.workbench);
  const attachmentRoot = projectAttachmentRoot(project);
  const conversationCount = projectConversationCount(props.workbench, project.id);

  // updateVmmDraft applies a partial edit to the current VMM draft.
  // updateVmmDraft 将局部编辑应用到当前 VMM 草稿。
  const updateVmmDraft = (patch: Partial<VmmConfigDraftSummary>) => {
    setVmmDraft((current) => current ? { ...current, ...patch } : current);
  };

  // saveVmmDraft persists the current VMM draft through the host.
  // saveVmmDraft 通过宿主持久化当前 VMM 草稿。
  const saveVmmDraft = async () => {
    if (!vmmDraft) {
      return;
    }
    try {
      setVmmBusy("saving");
      await props.onSaveVmmDraft(project.id, vmmDraft);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setVmmBusy("idle");
    }
  };

  // setProjectAgentSkill persists one Agent Skill project tri-state override optimistically.
  // setProjectAgentSkill 乐观持久化单个 Agent Skill 项目三态覆盖。
  const setProjectAgentSkill = async (name: string, stateKey: ProjectAgentSkillState) => {
    const previousOverrides = agentOverrides;
    setAgentOverrides((current) => {
      const next = { ...current };
      if (stateKey === "inherit") {
        delete next[name];
      } else {
        next[name] = stateKey;
      }
      return next;
    });
    setAgentBusySkill(name);
    try {
      await props.onSetAgentSkillState(project.id, name, stateKey);
    } catch (error) {
      setAgentOverrides(previousOverrides);
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setAgentBusySkill(null);
    }
  };

  // cleanupRequest builds the current attachment cleanup command payload.
  // cleanupRequest 构建当前附件清理命令载荷。
  const cleanupRequest = (): ProjectAttachmentCleanupRequest => ({
    projectId: project.id,
    cutoffDate: cleanupCutoffDate,
    selection: cleanupKinds,
  });

  // previewCleanup loads the current project attachment cleanup preview.
  // previewCleanup 加载当前项目附件清理预览。
  const previewCleanup = async () => {
    try {
      setCleanupBusy("preview");
      const preview = await previewProjectAttachmentCleanup(cleanupRequest());
      setCleanupPreview(preview);
      setCleanupReport(null);
      setCleanupNotice(preview.summary);
    } catch (error) {
      setCleanupPreview(null);
      setCleanupNotice(error instanceof Error ? error.message : String(error));
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setCleanupBusy("idle");
    }
  };

  // executeCleanup executes the cleanup represented by the latest matching preview.
  // executeCleanup 执行最近一次匹配预览代表的清理。
  const executeCleanup = async () => {
    try {
      setCleanupBusy("execute");
      const report = await executeProjectAttachmentCleanup(cleanupRequest());
      setCleanupReport(report);
      setCleanupPreview(null);
      setCleanupNotice(report.summary);
      toast.success("项目附件缓存清理已完成。");
    } catch (error) {
      setCleanupNotice(error instanceof Error ? error.message : String(error));
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setCleanupBusy("idle");
    }
  };

  // cleanupPreviewMatchesCurrent gates destructive execution behind a fresh positive preview.
  // cleanupPreviewMatchesCurrent 用最近一次有效正向预览控制破坏性执行入口。
  const cleanupPreviewMatchesCurrent = cleanupPreview
    && cleanupPreview.cutoffDate === cleanupCutoffDate
    && cleanupPreview.matchedAssetCount > 0;

  return (
    <Dialog open={Boolean(project)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent className="project-settings-dialog">
        <DialogHeader>
          <DialogTitle>项目设置：{project.name}</DialogTitle>
          <DialogDescription>项目级配置与缓存清理只作用于当前项目空间。</DialogDescription>
        </DialogHeader>
        <div className="project-settings-layout">
          <nav className="project-settings-nav" aria-label="项目设置分类">
            {sections.map((section) => (
              <button
                className={cn("project-settings-nav-button", activeSection === section.id && "active")}
                key={section.id}
                onClick={() => setActiveSection(section.id)}
                type="button"
              >
                {section.icon}
                <span>
                  <strong>{section.label}</strong>
                  <small>{section.detail}</small>
                </span>
              </button>
            ))}
          </nav>
          <section className="project-settings-content">
            {activeSection === "overview" ? (
              <div className="project-settings-section">
                <div className="project-settings-summary">
                  <FolderCog size={22} />
                  <div>
                    <strong>{project.name}</strong>
                    <span>{project.path}</span>
                  </div>
                </div>
                <div className="project-settings-grid">
                  <div className="project-settings-panel">
                    <strong>项目空间</strong>
                    <p>项目标识：{project.id}</p>
                    <p>对话数量：{conversationCount}</p>
                    <p>置顶状态：{project.pinned ? "已置顶" : "未置顶"}</p>
                  </div>
                  <div className="project-settings-panel">
                    <strong>附件缓存</strong>
                    <p>{attachmentRoot}</p>
                  </div>
                  <div className="project-settings-panel">
                    <strong>VMM 记忆</strong>
                    <p>{vmmDraft?.hasOverride ? "当前项目已启用独立覆盖。" : "当前项目继承全局 VMM 设置。"}</p>
                  </div>
                  <div className="project-settings-panel">
                    <strong>Agent Skills</strong>
                    <p>{Object.keys(agentOverrides).length > 0 ? `当前项目有 ${Object.keys(agentOverrides).length} 条覆盖。` : "未设置项目级覆盖，继承全局规则。"}</p>
                  </div>
                </div>
              </div>
            ) : null}

            {activeSection === "vmm" ? (
              <div className="project-settings-section">
                <header className="project-settings-heading">
                  <BrainCircuit size={18} />
                  <div>
                    <strong>项目级 VMM 记忆</strong>
                    <span>覆盖当前项目的记忆开关、用户/项目映射与召回注入参数。</span>
                  </div>
                </header>
                {vmmBusy === "loading" || !vmmDraft ? (
                  <p className="project-settings-muted">正在加载项目 VMM 设置...</p>
                ) : (
                  <>
                    <label className="project-settings-toggle-row">
                      <input
                        checked={vmmDraft.hasOverride}
                        onChange={(event) => updateVmmDraft({ hasOverride: event.target.checked })}
                        type="checkbox"
                      />
                      <span>
                        <strong>启用项目覆盖</strong>
                        <small>{vmmDraft.hasOverride ? "保存后当前项目使用下方设置。" : "保存后清空项目覆盖并继承全局设置。"}</small>
                      </span>
                    </label>
                    <div className={cn("project-settings-form-grid", !vmmDraft.hasOverride && "disabled")}>
                      <label className="project-settings-check-row">
                        <input checked={vmmDraft.enabled} disabled={!vmmDraft.hasOverride} onChange={(event) => updateVmmDraft({ enabled: event.target.checked })} type="checkbox" />
                        启用记忆连接
                      </label>
                      <label className="project-settings-check-row">
                        <input checked={vmmDraft.aiToolsEnabled} disabled={!vmmDraft.hasOverride} onChange={(event) => updateVmmDraft({ aiToolsEnabled: event.target.checked })} type="checkbox" />
                        暴露 AI 工具
                      </label>
                      <label className="project-settings-check-row">
                        <input checked={vmmDraft.precheckEnabled} disabled={!vmmDraft.hasOverride} onChange={(event) => updateVmmDraft({ precheckEnabled: event.target.checked })} type="checkbox" />
                        自动前置召回
                      </label>
                      <ProjectSettingsField disabled={!vmmDraft.hasOverride} label="用户 ID" value={vmmDraft.userId} onChange={(userId) => updateVmmDraft({ userId })} />
                      <ProjectSettingsField disabled={!vmmDraft.hasOverride} label="项目 ID" value={vmmDraft.projectId} onChange={(projectId) => updateVmmDraft({ projectId })} />
                      <ProjectSettingsField disabled={!vmmDraft.hasOverride} label="临时记忆 TTL" value={vmmDraft.implicitMemoryTurns} onChange={(implicitMemoryTurns) => updateVmmDraft({ implicitMemoryTurns })} />
                      <ProjectSettingsField disabled={!vmmDraft.hasOverride} label="画像刷新间隔" value={vmmDraft.profileRefreshTurns} onChange={(profileRefreshTurns) => updateVmmDraft({ profileRefreshTurns })} />
                      <ProjectSettingsField disabled={!vmmDraft.hasOverride} label="画像节点上限" value={vmmDraft.profileNodeLimit} onChange={(profileNodeLimit) => updateVmmDraft({ profileNodeLimit })} />
                      <ProjectSettingsField disabled={!vmmDraft.hasOverride} label="注入标签" value={vmmDraft.injectionTag} onChange={(injectionTag) => updateVmmDraft({ injectionTag })} />
                      <label className="project-settings-field wide">
                        <span>注入说明</span>
                        <Textarea disabled={!vmmDraft.hasOverride} value={vmmDraft.injectionDescription} onChange={(event) => updateVmmDraft({ injectionDescription: event.target.value })} />
                      </label>
                    </div>
                    <p className="project-settings-muted">项目级设置继承全局 gRPC 连接与 API Key；连接参数请在 VMM 记忆的全局配置中调整。</p>
                    <div className="project-settings-actions">
                      <Button disabled={vmmBusy === "saving"} onClick={() => void saveVmmDraft()} type="button">
                        <Save size={15} />
                        {vmmBusy === "saving" ? "保存中" : "保存 VMM 设置"}
                      </Button>
                    </div>
                  </>
                )}
              </div>
            ) : null}

            {activeSection === "agent-skills" ? (
              <div className="project-settings-section">
                <header className="project-settings-heading">
                  <Bot size={18} />
                  <div>
                    <strong>项目级 Agent Skills</strong>
                    <span>对系统与用户提示词技能建立当前项目专属启停覆盖。</span>
                  </div>
                </header>
                <div className="project-settings-panel compact">
                  <StatusPill status={props.workbench.agentSkills.globalEnabled ? "ready" : "pending"} label={props.workbench.agentSkills.globalEnabled ? "全局已启用" : "全局已关闭"} />
                  <p>{props.workbench.agentSkills.notice}</p>
                </div>
                {agentLoading ? (
                  <p className="project-settings-muted">正在加载项目技能覆盖...</p>
                ) : props.workbench.agentSkills.skills.length === 0 ? (
                  <p className="project-settings-muted">系统与用户目录暂未发现提示词技能。</p>
                ) : (
                  <div className="project-settings-skill-list">
                    {props.workbench.agentSkills.skills.map((skill) => {
                      const projectState = projectAgentSkillCurrentState(agentOverrides[skill.name]);
                      return (
                        <article className="project-settings-skill-row" key={skill.name}>
                          <span>
                            <strong>{skill.displayName || skill.name}</strong>
                            <small>{skill.description || skill.skillFile}</small>
                          </span>
                          <StatusPill status={projectState === "inherit" ? "pending" : "ready"} label={projectAgentSkillStateLabel(skill.enabled, projectState)} />
                          <div className="project-settings-segmented" role="group" aria-label={`${skill.displayName || skill.name} 项目状态`}>
                            {(["inherit", "enabled", "disabled"] as ProjectAgentSkillState[]).map((stateKey) => (
                              <button
                                className={cn("project-settings-segmented-button", projectState === stateKey && "active")}
                                disabled={agentBusySkill === skill.name}
                                key={stateKey}
                                onClick={() => void setProjectAgentSkill(skill.name, stateKey)}
                                type="button"
                              >
                                {projectAgentSkillOptionLabel(stateKey, skill.enabled)}
                              </button>
                            ))}
                          </div>
                        </article>
                      );
                    })}
                  </div>
                )}
              </div>
            ) : null}

            {activeSection === "attachments" ? (
              <div className="project-settings-section">
                <header className="project-settings-heading">
                  <Database size={18} />
                  <div>
                    <strong>项目附件缓存清理</strong>
                    <span>按日期和附件类型筛选当前项目的托管历史附件。</span>
                  </div>
                </header>
                <div className="project-settings-panel">
                  <strong>托管目录</strong>
                  <p>{attachmentRoot}</p>
                </div>
                <div className="project-settings-cleanup-grid">
                  <label className="project-settings-field">
                    <span>截止日期</span>
                    <Input
                      value={cleanupCutoffDate}
                      onChange={(event) => {
                        setCleanupCutoffDate(event.target.value);
                        setCleanupPreview(null);
                      }}
                      type="date"
                    />
                  </label>
                  <div className="project-settings-kind-grid">
                    {(["image", "audio", "video", "pdf"] as const).map((kind) => (
                      <label className="project-settings-check-row" key={kind}>
                        <input
                          checked={cleanupKinds[kind]}
                          onChange={(event) => {
                            setCleanupKinds((current) => ({ ...current, [kind]: event.target.checked }));
                            setCleanupPreview(null);
                          }}
                          type="checkbox"
                        />
                        {projectAttachmentKindLabel(kind)}
                      </label>
                    ))}
                  </div>
                </div>
                <div className="project-settings-actions">
                  <Button
                    disabled={cleanupBusy !== "idle"}
                    onClick={() => void previewCleanup()}
                    type="button"
                    variant="secondary"
                  >
                    <Calendar size={15} />
                    {cleanupBusy === "preview" ? "预览中" : "预览清理范围"}
                  </Button>
                  <Button disabled={!cleanupPreviewMatchesCurrent || cleanupBusy !== "idle"} onClick={() => void executeCleanup()} type="button">
                    <Trash2 size={15} />
                    {cleanupBusy === "execute" ? "清理中" : "执行清理"}
                  </Button>
                </div>
                <div className="project-settings-panel compact">
                  <strong>最近一次预览</strong>
                  <p>{cleanupPreview ? cleanupPreview.summary : cleanupNotice}</p>
                </div>
                <div className="project-settings-panel compact">
                  <strong>最近一次执行</strong>
                  <p>{cleanupReport ? cleanupReport.summary : "尚未执行历史附件缓存清理。"}</p>
                </div>
              </div>
            ) : null}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ProjectSettingsField renders one labeled text input inside project settings.
// ProjectSettingsField 渲染项目设置中的单个带标签文本输入。
function ProjectSettingsField(props: {
  // Whether the input is disabled.
  // 输入框是否禁用。
  disabled?: boolean;
  // Field label.
  // 字段标签。
  label: string;
  // Field value.
  // 字段值。
  value: string;
  // Updates the field value.
  // 更新字段值。
  onChange: (value: string) => void;
}) {
  return (
    <label className="project-settings-field">
      <span>{props.label}</span>
      <Input disabled={props.disabled} value={props.value} onChange={(event) => props.onChange(event.target.value)} />
    </label>
  );
}

// projectSettingsSections builds the left-side project settings navigation.
// projectSettingsSections 构建项目设置左侧导航。
function projectSettingsSections(project: ProjectSummary, workbench: WorkbenchSnapshot): ProjectSettingsSectionSummary[] {
  return [
    {
      id: "overview",
      icon: <FolderCog size={16} />,
      label: "概览",
      detail: project.pinned ? "已置顶项目" : "项目空间摘要",
    },
    {
      id: "vmm",
      icon: <BrainCircuit size={16} />,
      label: "VMM 记忆",
      detail: workbench.vmmMemory.projectDraft.hasOverride ? "项目覆盖" : "继承全局",
    },
    {
      id: "agent-skills",
      icon: <Bot size={16} />,
      label: "Agent Skills",
      detail: `${workbench.agentSkills.skills.length} 个提示词技能`,
    },
    {
      id: "attachments",
      icon: <Database size={16} />,
      label: "附件缓存",
      detail: "历史附件清理",
    },
  ];
}

// projectAttachmentRoot returns the managed attachment directory shown for a project.
// projectAttachmentRoot 返回项目展示的托管附件目录。
function projectAttachmentRoot(project: ProjectSummary): string {
  const separator = project.path.includes("\\") ? "\\" : "/";
  return `${project.path.replace(/[\\/]+$/, "")}${separator}.vulcan${separator}code${separator}attachments`;
}

// projectAgentSkillCurrentState normalizes one stored project Agent Skill override.
// projectAgentSkillCurrentState 归一化单个已存储的项目 Agent Skill 覆盖。
function projectAgentSkillCurrentState(overrideState?: string): ProjectAgentSkillState {
  if (overrideState === "enabled" || overrideState === "disabled") {
    return overrideState;
  }
  return "inherit";
}

// projectAgentSkillStateLabel returns the status label for one Agent Skill row.
// projectAgentSkillStateLabel 返回单个 Agent Skill 行的状态标签。
function projectAgentSkillStateLabel(inheritedEnabled: boolean, stateKey: ProjectAgentSkillState): string {
  if (stateKey === "enabled") {
    return "项目启用";
  }
  if (stateKey === "disabled") {
    return "项目关闭";
  }
  return inheritedEnabled ? "继承启用" : "继承关闭";
}

// projectAgentSkillOptionLabel returns the segmented-control label for one project state.
// projectAgentSkillOptionLabel 返回单个项目状态的分段控件标签。
function projectAgentSkillOptionLabel(stateKey: ProjectAgentSkillState, inheritedEnabled: boolean): string {
  if (stateKey === "inherit") {
    return inheritedEnabled ? "继承全局（启用）" : "继承全局（关闭）";
  }
  if (stateKey === "enabled") {
    return "启用";
  }
  return "关闭";
}

// projectAgentSkillSaveMessage returns the toast message for one project state save.
// projectAgentSkillSaveMessage 返回单个项目状态保存后的提示消息。
function projectAgentSkillSaveMessage(name: string, stateKey: ProjectAgentSkillState): string {
  if (stateKey === "inherit") {
    return `已将 ${name} 改为继承全局设置`;
  }
  return stateKey === "enabled" ? `已在项目中启用 ${name}` : `已在项目中关闭 ${name}`;
}

// projectAttachmentKindLabel returns the localized label for an attachment kind.
// projectAttachmentKindLabel 返回附件类型的本地化标签。
function projectAttachmentKindLabel(kind: keyof ProjectAttachmentCleanupKinds): string {
  switch (kind) {
    case "image":
      return "图片";
    case "audio":
      return "音频";
    case "video":
      return "视频";
    case "pdf":
      return "PDF";
  }
}

// projectAttachmentPreviewFallback builds a no-op cleanup preview for browser-only mode.
// projectAttachmentPreviewFallback 为纯浏览器模式构建无副作用清理预览。
function projectAttachmentPreviewFallback(request: ProjectAttachmentCleanupRequest): ProjectAttachmentCleanupPreviewSummary {
  const selectedKinds = projectAttachmentSelectedKindLabels(request.selection);
  let summary = `预览模式将检查 ${request.cutoffDate} 之前的 ${selectedKinds.join("、")} 托管附件。`;
  if (!request.cutoffDate) {
    summary = "请选择有效的截止日期。";
  } else if (selectedKinds.length === 0) {
    summary = "至少选择一种附件类型后才能预览。";
  }
  return {
    cutoffDate: request.cutoffDate,
    matchedAssetCount: 0,
    matchedByteLenLabel: "0 B",
    impactedSessionCount: 0,
    perKindCounts: {},
    summary,
  };
}

// projectAttachmentSelectedKindLabels returns selected cleanup-kind labels.
// projectAttachmentSelectedKindLabels 返回选中的清理类型标签。
function projectAttachmentSelectedKindLabels(kinds: ProjectAttachmentCleanupKinds): string[] {
  return (Object.keys(kinds) as Array<keyof ProjectAttachmentCleanupKinds>)
    .filter((kind) => kinds[kind])
    .map(projectAttachmentKindLabel);
}

// ArchiveProjectDialog confirms bulk conversation archiving for one project.
// ArchiveProjectDialog 确认单个项目下的对话批量归档。
function ArchiveProjectDialog(props: {
  project: ProjectSummary | null;
  conversationCount: number;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog open={Boolean(props.project)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent className="archive-dialog">
        <DialogHeader>
          <DialogTitle>归档 {props.conversationCount} 个对话？</DialogTitle>
          <DialogDescription>
            这会将 {props.project?.name ?? "当前项目"} 中的对话归档。之后你可以在已归档对话中找到它们。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={props.onCancel}>取消</Button>
          <Button type="button" variant="destructive" onClick={props.onConfirm}>全部归档</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ConversationContextMenu renders conversation-level right-click commands.
// ConversationContextMenu 渲染会话级右键命令。
function ConversationContextMenu(props: {
  conversation: ConversationSummary;
  children: React.ReactNode;
  isPinned: boolean;
  onAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
}) {
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{props.children}</ContextMenuTrigger>
      <ContextMenuContent className="conversation-context-menu">
        <ContextMenuItem onSelect={() => props.onAction("pin", props.conversation)}>
          <Pin size={15} />
          <span>{props.isPinned ? "取消置顶对话" : "置顶对话"}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("rename", props.conversation)}>
          <Pencil size={15} />
          <span>重命名对话</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("archive", props.conversation)}>
          <Archive size={15} />
          <span>归档对话</span>
        </ContextMenuItem>
        <ContextMenuItem disabled>
          <MailOpen size={15} />
          <span>标记为未读</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => props.onAction("open-directory", props.conversation)}>
          <FolderOpen size={15} />
          <span>在资源管理器中打开</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("copy-workdir", props.conversation)}>
          <Copy size={15} />
          <span>复制工作目录</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("copy-session-id", props.conversation)}>
          <FileText size={15} />
          <span>复制会话 ID</span>
        </ContextMenuItem>
        <ContextMenuItem disabled>
          <Link size={15} />
          <span>复制深度链接</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled>
          <GitBranch size={15} />
          <span>派生到本地</span>
        </ContextMenuItem>
        <ContextMenuItem disabled>
          <GitBranch size={15} />
          <span>派生到新工作树</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled>
          <ExternalLink size={15} />
          <span>在新窗口中打开</span>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

// projectFolderName extracts the folder name from a platform path.
// projectFolderName 从平台路径中提取文件夹名称。
function projectFolderName(path: string): string {
  return path.split(/[\\/]/).map((segment) => segment.trim()).filter(Boolean).pop() ?? "";
}

// restoreProjectDisplayName resolves the default project name used when rename input is empty.
// restoreProjectDisplayName 解析重命名输入为空时使用的默认项目名称。
function restoreProjectDisplayName(project: ProjectSummary | null, target: RenameTarget): string {
  const folderName = project ? projectFolderName(project.path) : "";
  if (folderName) {
    return folderName;
  }
  const currentName = target.name.trim();
  if (currentName && !isPathSeparatorOnlyName(currentName)) {
    return currentName;
  }
  return target.id;
}

// isPathSeparatorOnlyName detects invalid slash-only names from older reset attempts.
// isPathSeparatorOnlyName 检测旧还原尝试产生的仅路径分隔符名称。
function isPathSeparatorOnlyName(name: string): boolean {
  return name.length > 0 && /^[\\/]+$/.test(name);
}

// ChatArea renders the conversation workspace and composer.
// ChatArea 渲染会话工作区与输入区域。
function ChatArea(props: {
  startup: StartupSnapshot;
  workbench: WorkbenchSnapshot;
  selectedDraftConversation: DraftConversationSelection | null;
  selectedConversationId: string | null;
  loadingConversationId: string | null;
  conversationLoadError: string | null;
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  onStopConversationRun: () => Promise<boolean>;
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  onCloseSubagentPanel: () => Promise<void>;
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
}) {
  // t resolves localized runtime labels for conversation overlays and cues.
  // t 解析会话浮层与提示使用的本地化运行文案。
  const { t } = useI18n();
  // workspaceRef stores the scrollable conversation timeline element.
  // workspaceRef 保存可滚动的会话时间线元素。
  const workspaceRef = React.useRef<HTMLElement | null>(null);
  // shouldFollowConversationTailRef remembers the pre-update tail-follow decision across streaming renders.
  // shouldFollowConversationTailRef 在流式渲染之间记住更新前是否应追随尾部。
  const shouldFollowConversationTailRef = React.useRef(true);
  // selectedConversationTailRef tracks conversation switches so opening a session can jump directly to the bottom.
  // selectedConversationTailRef 跟踪会话切换，使打开会话时可直接跳到底部。
  const selectedConversationTailRef = React.useRef<string | null>(null);
  // isAtConversationBottom tracks whether the user is viewing the latest conversation content.
  // isAtConversationBottom 跟踪用户是否正在查看最新的会话内容。
  const [isAtConversationBottom, setIsAtConversationBottom] = React.useState(true);
  // composerDockHeight tracks the actual floating composer stack height for overlay positioning.
  // composerDockHeight 跟踪浮动输入区堆栈真实高度，用于定位浮层。
  const [composerDockHeight, setComposerDockHeight] = React.useState(132);
  // thinkingOverlayEnabled controls whether streaming thinking uses the floating surface or inline transcript.
  // thinkingOverlayEnabled 控制流式思考使用浮层还是会话内联展示。
  const [thinkingOverlayEnabled, setThinkingOverlayEnabled] = React.useState(() => initialThinkingOverlayEnabled());
  const selectedConversation = props.workbench.conversations.find((conversation) => conversation.id === props.selectedConversationId);
  const subagentNavigation = props.workbench.subagentNavigation?.parentConversationId === props.selectedConversationId
    ? props.workbench.subagentNavigation
    : null;
  const activeSurfaceConversationId = subagentNavigation?.activeConversationId ?? props.selectedConversationId;
  // renderedSurfaceConversationIdRef tracks the surface currently represented by the shared scroll element.
  // renderedSurfaceConversationIdRef 跟踪当前共享滚动元素代表的界面。
  const renderedSurfaceConversationIdRef = React.useRef<string | null>(activeSurfaceConversationId ?? null);
  // conversationScrollSnapshotsRef keeps scroll restoration data per parent or child surface.
  // conversationScrollSnapshotsRef 按主会话或子代理界面保存滚动恢复数据。
  const conversationScrollSnapshotsRef = React.useRef<Map<string, ConversationScrollSnapshot>>(new Map());
  // conversationVirtualizerStatesRef keeps measured row heights per parent or child surface.
  // conversationVirtualizerStatesRef 按主会话或子代理界面保存已测量行高。
  const conversationVirtualizerStatesRef = React.useRef<Map<string, ConversationTimelineVirtualRestoreState>>(new Map());
  // scrollRestoreGuardRef prevents transient restore measurements from overwriting tail-pinned memory.
  // scrollRestoreGuardRef 防止恢复过程中的临时测量覆盖贴底记忆。
  const scrollRestoreGuardRef = React.useRef<{ surfaceId: string; wasAtBottom: boolean } | null>(null);
  const activeSubagentTask = subagentNavigation?.tasks?.find((task) => task.isActive) ?? null;
  const surfaceConversation = activeSubagentTask
    ? {
      id: activeSubagentTask.taskId,
      title: activeSubagentTask.label,
      status: activeSubagentTask.isRunning ? "running" : activeSubagentTask.runState,
      runPhase: activeSubagentTask.isRunning ? "requesting" : activeSubagentTask.runState,
      projectId: selectedConversation?.projectId ?? null,
    }
    : selectedConversation;
  const messages = React.useMemo(
    () => props.workbench.messages.filter((message) => message.conversationId === activeSurfaceConversationId),
    [activeSurfaceConversationId, props.workbench.messages],
  );
  // renderItems stores parsed transcript rows for this active surface.
  // renderItems 保存当前活动界面的已解析转录行。
  const renderItems = React.useMemo(() => projectConversationRenderItems(messages), [messages]);
  const isLoading = props.loadingConversationId === props.selectedConversationId;
  // showConversationLoadingOnly is true only when no cached messages can be shown during hydration.
  // showConversationLoadingOnly 仅在水合期间没有可展示缓存消息时为真。
  const showConversationLoadingOnly = isLoading && messages.length === 0 && !props.conversationLoadError;
  const selectedProject = selectedConversation ? projectForConversation(props.workbench, selectedConversation) : null;
  const emptyProjectName = props.selectedDraftConversation?.projectName ?? selectedProject?.name ?? "当前项目";
  const isEmptyConversation = Boolean(
    props.selectedDraftConversation
      || (!activeSubagentTask && selectedConversation && !showConversationLoadingOnly && !props.conversationLoadError && messages.length === 0),
  );
  // emptyPromptVisible keeps the empty prompt mounted long enough to fade out after first submit.
  // emptyPromptVisible 让空会话提示在首次提交后保留足够时间完成淡出。
  const [emptyPromptVisible, setEmptyPromptVisible] = React.useState(isEmptyConversation);
  // emptyPromptLeaving marks the empty prompt exit animation.
  // emptyPromptLeaving 标记空会话提示退出动画。
  const [emptyPromptLeaving, setEmptyPromptLeaving] = React.useState(false);
  // runtimeCue stores the compact in-progress label shown above the composer.
  // runtimeCue 保存显示在输入区上方的紧凑进行中提示。
  const streamingThinkingBlock = activeStreamingThinkingBlock(surfaceConversation, renderItems);
  const displayedStreamingThinkingBlock = thinkingOverlayEnabled ? streamingThinkingBlock : null;
  // streamingThinkingOverlayBlock keeps the last thinking popup mounted during its delayed exit.
  // streamingThinkingOverlayBlock 在思考弹窗延迟退场期间保留最后一个思考块。
  const [streamingThinkingOverlayBlock, setStreamingThinkingOverlayBlock] = React.useState<ConversationBlock | null>(null);
  // streamingThinkingOverlayLeaving marks the fade-out half of the delayed thinking popup exit.
  // streamingThinkingOverlayLeaving 标记思考弹窗延迟退场中的淡出阶段。
  const [streamingThinkingOverlayLeaving, setStreamingThinkingOverlayLeaving] = React.useState(false);
  const visibleStreamingThinkingOverlayBlock = displayedStreamingThinkingBlock ?? streamingThinkingOverlayBlock;
  const displayedStreamingThinkingOverlaySignature = displayedStreamingThinkingBlock
    ? thinkingOverlayBlockSignature(displayedStreamingThinkingBlock)
    : null;
  const retainedStreamingThinkingOverlayId = streamingThinkingOverlayBlock?.id ?? null;
  // hasRunningSubagents reflects the backend-routed subagent lane rather than local UI inference.
  // hasRunningSubagents 反映后端路由的子代理通道，而不是本地界面推断。
  const hasRunningSubagents = subagentNavigation?.tasks?.some((task) => task.isRunning) ?? false;
  // suppressSubagentOnlyMainCue keeps delegated work in the footer instead of the main request cue.
  // suppressSubagentOnlyMainCue 将纯子代理工作保留在底部栏，不显示为主窗口请求提示。
  const suppressSubagentOnlyMainCue = hasRunningSubagents
    && (Boolean(activeSubagentTask?.isRunning)
      || normalizedSessionRunPhase(surfaceConversation?.runPhase) === "toolrunning");
  const runtimeCue = visibleStreamingThinkingOverlayBlock || suppressSubagentOnlyMainCue
    ? null
    : conversationRuntimeCue(surfaceConversation, renderItems, t);
  const isConversationRunning = conversationIsRunning(selectedConversation) || hasRunningSubagents;
  // conversationStreamSignature changes when streamed content grows inside existing transcript rows.
  // conversationStreamSignature 在现有转录行内部的流式内容增长时变化。
  const conversationStreamSignature = React.useMemo(
    () => buildConversationStreamSignature(messages),
    [messages],
  );
  // rememberConversationScrollSnapshot captures the current shared scroll element before route changes.
  // rememberConversationScrollSnapshot 在路由切换前捕获当前共享滚动元素状态。
  const rememberConversationScrollSnapshot = React.useCallback(() => {
    const element = workspaceRef.current;
    const surfaceId = element?.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    if (!element || !surfaceId) {
      return;
    }
    const wasAtBottom = conversationViewportAllowsTailFollow(element);
    conversationScrollSnapshotsRef.current.set(surfaceId, {
      distanceFromBottom: conversationDistanceFromBottom(element),
      wasAtBottom,
    });
    shouldFollowConversationTailRef.current = wasAtBottom;
  }, [activeSurfaceConversationId]);

  // updateConversationBottomState refreshes the bottom affordance from the timeline scroll position.
  // updateConversationBottomState 根据时间线滚动位置刷新底部跳转按钮状态。
  const updateConversationBottomState = React.useCallback(() => {
    const element = workspaceRef.current;
    if (!element) {
      shouldFollowConversationTailRef.current = true;
      setIsAtConversationBottom(true);
      return;
    }
    const shouldFollow = conversationViewportAllowsTailFollow(element);
    const surfaceId = element.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    const restoreGuard = scrollRestoreGuardRef.current;
    if (surfaceId && restoreGuard?.surfaceId === surfaceId && restoreGuard.wasAtBottom && !shouldFollow) {
      shouldFollowConversationTailRef.current = true;
      conversationScrollSnapshotsRef.current.set(surfaceId, {
        distanceFromBottom: 0,
        wasAtBottom: true,
      });
      setIsAtConversationBottom(true);
      return;
    }
    shouldFollowConversationTailRef.current = shouldFollow;
    if (surfaceId) {
      conversationScrollSnapshotsRef.current.set(surfaceId, {
        distanceFromBottom: conversationDistanceFromBottom(element),
        wasAtBottom: shouldFollow,
      });
    }
    setIsAtConversationBottom(shouldFollow);
  }, [activeSurfaceConversationId]);

  // handleOpenSubagentPanelFromTimeline preserves the current scroll before opening a child transcript.
  // handleOpenSubagentPanelFromTimeline 在打开子代理转录前保留当前滚动位置。
  const handleOpenSubagentPanelFromTimeline = React.useCallback((
    parentConversationId: string,
    taskId: string,
  ) => {
    rememberConversationScrollSnapshot();
    return props.onOpenSubagentPanel(parentConversationId, taskId);
  }, [props.onOpenSubagentPanel, rememberConversationScrollSnapshot]);

  // scrollConversationToBottom moves the timeline to the latest content.
  // scrollConversationToBottom 将时间线滚动到最新内容。
  const scrollConversationToBottom = React.useCallback((behavior: ScrollBehavior = "smooth") => {
    const element = workspaceRef.current;
    if (!element) {
      return;
    }
    const safeBehavior = behavior === "auto" || behavior === "instant" || behavior === "smooth" ? behavior : "smooth";
    element.scrollTo({ top: element.scrollHeight, behavior: safeBehavior });
  }, []);

  // jumpConversationToBottom immediately pins the timeline to its latest rendered height.
  // jumpConversationToBottom 立即把时间线固定到最新已渲染高度。
  const jumpConversationToBottom = React.useCallback(() => {
    const element = workspaceRef.current;
    if (!element) {
      return false;
    }
    element.scrollTop = element.scrollHeight;
    return true;
  }, []);

  // handleComposerDockHeightChange keeps the sticky footer from pushing a tail-locked transcript away.
  // handleComposerDockHeightChange 避免粘性底部输入区高度变化时把贴底转录推离底部。
  const handleComposerDockHeightChange = React.useCallback((height: number) => {
    setComposerDockHeight((current) => {
      if (current === height) {
        return current;
      }
      if (shouldFollowConversationTailRef.current) {
        window.requestAnimationFrame(() => {
          const element = workspaceRef.current;
          if (element) {
            element.scrollTop = element.scrollHeight;
          }
        });
      }
      return height;
    });
  }, []);

  // handleTimelineVirtualizerStateChange remembers measured row heights for the active surface.
  // handleTimelineVirtualizerStateChange 记住当前界面的已测量行高。
  const handleTimelineVirtualizerStateChange = React.useCallback((surfaceId: string, state: ConversationTimelineVirtualRestoreState | null) => {
    if (!surfaceId) {
      return;
    }
    if (!state || Object.keys(state.rowHeightsByKey).length === 0) {
      conversationVirtualizerStatesRef.current.delete(surfaceId);
      return;
    }
    conversationVirtualizerStatesRef.current.set(surfaceId, state);
  }, []);

  React.useLayoutEffect(() => {
    if (!props.selectedConversationId || isEmptyConversation || showConversationLoadingOnly) {
      return;
    }

    const element = workspaceRef.current;
    const previousSurfaceId = selectedConversationTailRef.current;
    const selectedChanged = previousSurfaceId !== activeSurfaceConversationId;
    if (
      selectedChanged
      && previousSurfaceId
      && element
      && !conversationScrollSnapshotsRef.current.has(previousSurfaceId)
      && shouldFollowConversationTailRef.current
    ) {
      conversationScrollSnapshotsRef.current.set(previousSurfaceId, {
        distanceFromBottom: 0,
        wasAtBottom: true,
      });
    }
    selectedConversationTailRef.current = activeSurfaceConversationId;
    renderedSurfaceConversationIdRef.current = activeSurfaceConversationId ?? null;
    const savedScrollSnapshot = selectedChanged && activeSurfaceConversationId
      ? conversationScrollSnapshotsRef.current.get(activeSurfaceConversationId)
      : null;
    const restoreSurfaceId = activeSurfaceConversationId;
    if (selectedChanged && savedScrollSnapshot && element && restoreSurfaceId) {
      const restoreSavedScrollPosition = () => {
        if (savedScrollSnapshot.wasAtBottom) {
          element.scrollTop = element.scrollHeight;
          return;
        }
        conversationScrollToDistanceFromBottom(element, savedScrollSnapshot.distanceFromBottom);
      };
      shouldFollowConversationTailRef.current = savedScrollSnapshot.wasAtBottom;
      scrollRestoreGuardRef.current = {
        surfaceId: restoreSurfaceId,
        wasAtBottom: savedScrollSnapshot.wasAtBottom,
      };
      restoreSavedScrollPosition();
      let secondFrame: number | null = null;
      const firstFrame = window.requestAnimationFrame(() => {
        restoreSavedScrollPosition();
        secondFrame = window.requestAnimationFrame(() => {
          restoreSavedScrollPosition();
          updateConversationBottomState();
          if (scrollRestoreGuardRef.current?.surfaceId === restoreSurfaceId) {
            scrollRestoreGuardRef.current = null;
          }
        });
      });
      return () => {
        window.cancelAnimationFrame(firstFrame);
        if (secondFrame !== null) {
          window.cancelAnimationFrame(secondFrame);
        }
        if (scrollRestoreGuardRef.current?.surfaceId === restoreSurfaceId) {
          scrollRestoreGuardRef.current = null;
        }
      };
    }

    const shouldFollowTail = selectedChanged
      || !element
      || shouldFollowConversationTailRef.current
      || conversationViewportAllowsTailFollow(element);
    if (!shouldFollowTail) {
      updateConversationBottomState();
      return;
    }

    shouldFollowConversationTailRef.current = true;
    let secondFrame: number | null = null;
    let thirdFrame: number | null = null;
    jumpConversationToBottom();
    const firstFrame = window.requestAnimationFrame(() => {
      jumpConversationToBottom();
      secondFrame = window.requestAnimationFrame(() => {
        jumpConversationToBottom();
        thirdFrame = window.requestAnimationFrame(() => {
          jumpConversationToBottom();
          updateConversationBottomState();
        });
        updateConversationBottomState();
      });
    });

    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame !== null) {
        window.cancelAnimationFrame(secondFrame);
      }
      if (thirdFrame !== null) {
        window.cancelAnimationFrame(thirdFrame);
      }
    };
  }, [
    conversationStreamSignature,
    isEmptyConversation,
    jumpConversationToBottom,
    activeSurfaceConversationId,
    showConversationLoadingOnly,
    updateConversationBottomState,
  ]);

  React.useEffect(() => {
    updateConversationBottomState();
  }, [activeSurfaceConversationId, conversationStreamSignature, updateConversationBottomState]);

  React.useEffect(() => {
    window.localStorage.setItem(THINKING_OVERLAY_STORAGE_KEY, thinkingOverlayEnabled ? "true" : "false");
  }, [thinkingOverlayEnabled]);

  React.useEffect(() => {
    if (displayedStreamingThinkingBlock) {
      setStreamingThinkingOverlayBlock((current) => (
        current && thinkingOverlayBlockSignature(current) === displayedStreamingThinkingOverlaySignature
          ? current
          : displayedStreamingThinkingBlock
      ));
      setStreamingThinkingOverlayLeaving(false);
      return;
    }
    if (!streamingThinkingOverlayBlock) {
      return;
    }
    const fadeTimer = window.setTimeout(() => {
      setStreamingThinkingOverlayLeaving(true);
    }, 500);
    const removeTimer = window.setTimeout(() => {
      setStreamingThinkingOverlayBlock(null);
      setStreamingThinkingOverlayLeaving(false);
    }, 1000);
    return () => {
      window.clearTimeout(fadeTimer);
      window.clearTimeout(removeTimer);
    };
  }, [displayedStreamingThinkingBlock, displayedStreamingThinkingOverlaySignature, retainedStreamingThinkingOverlayId, streamingThinkingOverlayBlock]);

  React.useEffect(() => {
    if (isEmptyConversation) {
      setEmptyPromptVisible(true);
      setEmptyPromptLeaving(false);
      return;
    }
    if (!emptyPromptVisible) {
      return;
    }
    setEmptyPromptLeaving(true);
    const timeout = window.setTimeout(() => {
      setEmptyPromptVisible(false);
      setEmptyPromptLeaving(false);
    }, 360);
    return () => window.clearTimeout(timeout);
  }, [emptyPromptVisible, isEmptyConversation]);

  // composerDock renders either inside the scroll footer or as the empty-state floating composer.
  // composerDock 渲染滚动底部插槽中的输入区，或空会话状态下的浮动输入区。
  const composerDock = (
    <ComposerDock
      aiCapabilities={props.workbench.aiCapabilities ?? []}
      attachmentCapabilities={props.workbench.composerAttachmentCapabilities ?? []}
      projectId={selectedProject?.id ?? props.selectedDraftConversation?.projectId ?? null}
      conversationId={selectedConversation?.id ?? null}
      slashCommands={props.workbench.composerSlashCommands ?? []}
      contextUsage={isEmptyConversation ? null : props.workbench.contextUsage ?? null}
      emptyMode={isEmptyConversation}
      onScrollToBottom={() => scrollConversationToBottom()}
      onSelectWorkbenchModel={props.onSelectWorkbenchModel}
      onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
      onSlashCommand={props.onSlashCommand}
      onComposerDockHeightChange={handleComposerDockHeightChange}
      onSubmitPrompt={props.onSubmitPrompt}
      onStopConversationRun={props.onStopConversationRun}
      providers={props.workbench.providers}
      pendingPrompts={props.workbench.pendingPrompts ?? []}
      isConversationRunning={isConversationRunning}
      thinkingOverlayEnabled={thinkingOverlayEnabled}
      reasoningSelection={props.workbench.reasoningSelection ?? null}
      runtimeCue={isEmptyConversation ? null : runtimeCue}
      showScrollToBottom={!isEmptyConversation && !isAtConversationBottom}
      onEditPendingPrompt={props.onEditPendingPrompt}
      onInsertPendingPrompt={props.onInsertPendingPrompt}
      onRemovePendingPrompt={props.onRemovePendingPrompt}
      onReorderPendingPrompts={props.onReorderPendingPrompts}
      onThinkingOverlayEnabledChange={setThinkingOverlayEnabled}
    />
  );

  return (
    <div
      className={cn("area-grid chat-grid", isEmptyConversation && "empty-conversation-grid", showConversationLoadingOnly && "loading-conversation-grid")}
      style={{ "--composer-dock-height": `${composerDockHeight}px` } as React.CSSProperties}
    >
      {showConversationLoadingOnly ? (
        <ConversationLoadingCenter />
      ) : (
        <>
          {emptyPromptVisible ? (
            <EmptyConversationPrompt
              leaving={emptyPromptLeaving}
              projectName={emptyProjectName}
            />
          ) : null}
          <ConversationWorkspace
            conversation={surfaceConversation}
            hiddenThinkingBlockId={displayedStreamingThinkingBlock?.id ?? null}
            messages={messages}
            renderItems={renderItems}
            error={props.conversationLoadError}
            parentConversationId={activeSurfaceConversationId ?? null}
            workspacePath={selectedProject?.path ?? null}
            onScrollStateChange={updateConversationBottomState}
            onOpenSubagentPanel={handleOpenSubagentPanelFromTimeline}
            scrollRef={workspaceRef}
            initialScrollDistanceFromBottom={activeSurfaceConversationId
              ? conversationScrollSnapshotsRef.current.get(activeSurfaceConversationId)?.distanceFromBottom ?? 0
              : 0}
            initialVirtualizerState={activeSurfaceConversationId
              ? conversationVirtualizerStatesRef.current.get(activeSurfaceConversationId) ?? null
              : null}
            onVirtualizerStateChange={handleTimelineVirtualizerStateChange}
            footer={isEmptyConversation ? null : composerDock}
            suppressEmptyPlaceholder={isEmptyConversation}
          />
          {visibleStreamingThinkingOverlayBlock ? (
            <StreamingThinkingOverlay
              block={visibleStreamingThinkingOverlayBlock}
              leaving={streamingThinkingOverlayLeaving}
            />
          ) : null}
          {isEmptyConversation ? composerDock : null}
        </>
      )}
    </div>
  );
}

// SubagentNavigationMenu renders the backend-projected subagent switcher from the conversation title bar.
// SubagentNavigationMenu 从会话标题栏渲染由后端投影的子代理切换器。
function SubagentNavigationMenu(props: {
  navigation: WorkbenchSnapshot["subagentNavigation"];
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  onCloseSubagentPanel: () => Promise<void>;
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
}) {
  const navigation = props.navigation;
  const tasks = navigation?.tasks ?? [];
  const activeTaskId = navigation?.activeTaskId ?? null;
  const runningCount = tasks.filter((task) => task.isRunning).length;
  const canReturnToParent = Boolean(activeTaskId);
  if (!navigation || tasks.length === 0) {
    return null;
  }
  return (
    <div className={cn("subagent-title-menu", runningCount > 0 && "running")}>
      {canReturnToParent ? (
        <Button
          aria-label="返回主会话"
          className="subagent-title-return"
          onClick={() => {
            webuiClickTrace("subagent.menu.return", {
              parentConversationId: navigation.parentConversationId,
              activeTaskId,
              runningCount,
            });
            void props.onCloseSubagentPanel();
          }}
          size="icon-sm"
          title="返回主会话"
          type="button"
          variant="ghost"
        >
          <ChevronLeft size={15} />
        </Button>
      ) : null}
      <Button
        aria-label={runningCount > 0 ? `${runningCount} 个子代理正在运行` : "子代理调度"}
        className="subagent-title-trigger"
        size="icon-sm"
        type="button"
        variant="ghost"
      >
        <Bot size={15} />
      </Button>
      <nav className="subagent-title-flyout" aria-label="子代理调度切换">
        {tasks.map((task) => (
          <div className={cn("subagent-title-row-group", task.isActive && "active")} key={task.taskId}>
            <Button
              className="subagent-title-row"
              onClick={() => {
                webuiClickTrace("subagent.menu.open_task", {
                  parentConversationId: navigation.parentConversationId,
                  taskId: task.taskId,
                  runState: task.runState,
                  isRunning: task.isRunning,
                });
                void props.onOpenSubagentPanel(navigation.parentConversationId, task.taskId);
              }}
              size="sm"
              type="button"
              variant="ghost"
            >
              <Boxes size={14} />
              <span>{task.label}</span>
              <span className={cn("subagent-navigation-status", task.runState)}>{subagentRunStateLabel(task.runState)}</span>
            </Button>
            {task.isRunning ? (
              <Button
                aria-label={`停止子代理 ${task.label}`}
                className="subagent-title-stop"
                onClick={() => {
                  webuiClickTrace("subagent.menu.stop_task", {
                    parentConversationId: navigation.parentConversationId,
                    taskId: task.taskId,
                    runState: task.runState,
                  });
                  void props.onStopSubagentTask(navigation.parentConversationId, task.taskId);
                }}
                size="icon"
                type="button"
                variant="ghost"
              >
                <Square size={12} />
              </Button>
            ) : null}
          </div>
        ))}
      </nav>
    </div>
  );
}


// subagentRunStateLabel maps backend run-state keys to compact WebUI labels.
// subagentRunStateLabel 将后端运行状态键映射为紧凑 WebUI 标签。
function subagentRunStateLabel(runState: string): string {
  switch (runState) {
    case "running":
      return "运行中";
    case "stopped":
      return "已停止";
    case "failed":
      return "失败";
    default:
      return "已完成";
  }
}

// useConversationTimelineVirtualWindow returns the mounted transcript range for long timelines.
// useConversationTimelineVirtualWindow 返回长转录当前需要挂载的范围。
function useConversationTimelineVirtualWindow(
  items: ConversationTimelineItem[],
  scrollRef: React.RefObject<HTMLElement | null>,
  cacheScopeKey: string,
  initialDistanceFromBottom: number,
  initialVirtualizerState: ConversationTimelineVirtualRestoreState | null,
  onVirtualizerStateChange: (surfaceId: string, state: ConversationTimelineVirtualRestoreState | null) => void,
): {
  // Current virtual window projection.
  // 当前虚拟窗口投影。
  window: ConversationTimelineVirtualWindow;
  // Refreshes viewport metrics after a scroll event.
  // 滚动事件后刷新视口指标。
  onScroll: () => void;
  // Stores a measured row height by stable timeline key.
  // 按稳定时间线键保存测量后的行高。
  onRowHeight: (key: string, height: number) => void;
} {
  const measuredHeightsRef = React.useRef<Map<string, number>>(
    new Map(Object.entries(initialVirtualizerState?.rowHeightsByKey ?? {})),
  );
  const cacheScopeKeyRef = React.useRef(cacheScopeKey);
  const pendingInitialDistanceFromBottomRef = React.useRef<number | null>(Math.max(0, initialDistanceFromBottom));
  const [measureVersion, setMeasureVersion] = React.useState(0);
  const [viewport, setViewport] = React.useState({ scrollTop: 0, height: 0, width: 0 });
  if (cacheScopeKeyRef.current !== cacheScopeKey) {
    measuredHeightsRef.current = new Map(Object.entries(initialVirtualizerState?.rowHeightsByKey ?? {}));
    cacheScopeKeyRef.current = cacheScopeKey;
    pendingInitialDistanceFromBottomRef.current = Math.max(0, initialDistanceFromBottom);
  }
  const onScroll = React.useCallback(() => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    setViewport({
      scrollTop: element.scrollTop,
      height: element.clientHeight,
      width: element.clientWidth,
    });
  }, [scrollRef]);

  React.useLayoutEffect(() => {
    onScroll();
  }, [items.length, onScroll]);

  React.useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", onScroll);
      return () => window.removeEventListener("resize", onScroll);
    }
    const observer = new ResizeObserver(onScroll);
    observer.observe(element);
    return () => observer.disconnect();
  }, [onScroll, scrollRef]);

  React.useLayoutEffect(() => {
    if (pendingInitialDistanceFromBottomRef.current === null) {
      return;
    }
    let cancelled = false;
    let frame: number | null = null;
    // restorePendingBottomDistance aligns the DOM scroll position with the virtual projection.
    // restorePendingBottomDistance 让 DOM 滚动位置与虚拟窗口投影保持一致。
    const restorePendingBottomDistance = () => {
      const element = scrollRef.current;
      const distanceFromBottom = pendingInitialDistanceFromBottomRef.current;
      if (!element || distanceFromBottom === null) {
        return;
      }
      if (distanceFromBottom <= 0) {
        element.scrollTop = element.scrollHeight;
        return;
      }
      conversationScrollToDistanceFromBottom(element, distanceFromBottom);
    };
    const clearAfterFrame = (remainingFrames: number) => {
      frame = window.requestAnimationFrame(() => {
        if (cancelled) {
          return;
        }
        restorePendingBottomDistance();
        if (remainingFrames <= 0) {
          pendingInitialDistanceFromBottomRef.current = null;
          onScroll();
          return;
        }
        clearAfterFrame(remainingFrames - 1);
      });
    };
    clearAfterFrame(3);
    return () => {
      cancelled = true;
      if (frame !== null) {
        window.cancelAnimationFrame(frame);
      }
    };
  }, [cacheScopeKey, onScroll, scrollRef]);

  React.useEffect(() => {
    let changed = false;
    const liveKeys = new Set(items.map(conversationTimelineItemKey));
    for (const key of measuredHeightsRef.current.keys()) {
      if (!liveKeys.has(key)) {
        measuredHeightsRef.current.delete(key);
        changed = true;
      }
    }
    if (changed) {
      setMeasureVersion((version) => version + 1);
      onVirtualizerStateChange(cacheScopeKey, {
        rowHeightsByKey: Object.fromEntries(measuredHeightsRef.current),
      });
    }
  }, [cacheScopeKey, items, onVirtualizerStateChange]);

  const onRowHeight = React.useCallback((key: string, height: number) => {
    if (!Number.isFinite(height) || height <= 0) {
      return;
    }
    const rounded = Math.ceil(height);
    const current = measuredHeightsRef.current.get(key);
    if (current === rounded) {
      return;
    }
    const scrollElement = scrollRef.current;
    const heightDelta = current == null ? 0 : rounded - current;
    const measuredHeights = measuredHeightsRef.current;
    const rowIndex = items.findIndex((item) => conversationTimelineItemKey(item) === key);
    const rowItem = rowIndex < 0 ? null : items[rowIndex] ?? null;
    const rowTopBefore = rowIndex < 0
      ? null
      : items.slice(0, rowIndex).reduce((sum, item) => (
        sum + (measuredHeights.get(conversationTimelineItemKey(item)) ?? conversationTimelineItemEstimatedHeight(item))
      ), 0);
    const rowBottomBefore = rowTopBefore == null
      ? null
      : rowTopBefore + (current ?? (rowItem ? conversationTimelineItemEstimatedHeight(rowItem) : 0));
    const distanceFromBottom = scrollElement
      ? conversationDistanceFromBottom(scrollElement)
      : Number.POSITIVE_INFINITY;
    const wasNearBottom = scrollElement
      ? distanceFromBottom <= 24 || (heightDelta > 0 && distanceFromBottom - heightDelta <= 24)
      : false;
    const shouldPreserveViewport = Boolean(
      scrollElement
      && heightDelta !== 0
      && rowBottomBefore != null
      && rowBottomBefore <= scrollElement.scrollTop + 1,
    );
    measuredHeightsRef.current.set(key, rounded);
    onVirtualizerStateChange(cacheScopeKey, {
      rowHeightsByKey: Object.fromEntries(measuredHeightsRef.current),
    });
    setMeasureVersion((version) => version + 1);
    if (!scrollElement) {
      return;
    }
    if (wasNearBottom) {
      window.requestAnimationFrame(() => {
        scrollElement.scrollTop = scrollElement.scrollHeight;
      });
      return;
    }
    if (shouldPreserveViewport) {
      scrollElement.scrollTop += heightDelta;
    }
  }, [cacheScopeKey, items, onVirtualizerStateChange, scrollRef]);

  const virtualWindow = React.useMemo<ConversationTimelineVirtualWindow>(() => {
    void measureVersion;
    if (items.length < CONVERSATION_VIRTUALIZATION_MIN_ITEMS || viewport.height <= 0) {
      return {
        items: items.map((item, index) => ({ index, item })),
        paddingTop: 0,
        paddingBottom: 0,
        measuring: false,
        virtualized: false,
      };
    }
    const heights = items.map((item) => measuredHeightsRef.current.get(conversationTimelineItemKey(item))
      ?? conversationTimelineItemEstimatedHeight(item));
    const offsets: number[] = [];
    let totalHeight = 0;
    for (const height of heights) {
      offsets.push(totalHeight);
      totalHeight += height;
    }
    const pendingDistanceFromBottom = pendingInitialDistanceFromBottomRef.current;
    const effectiveScrollTop = pendingDistanceFromBottom === null
      ? viewport.scrollTop
      : Math.max(0, totalHeight - viewport.height - pendingDistanceFromBottom);
    const windowTop = Math.max(0, effectiveScrollTop - CONVERSATION_VIRTUALIZATION_OVERSCAN_PX);
    const windowBottom = effectiveScrollTop + viewport.height + CONVERSATION_VIRTUALIZATION_OVERSCAN_PX;
    let startIndex = 0;
    while (startIndex < items.length && offsets[startIndex] + heights[startIndex] < windowTop) {
      startIndex += 1;
    }
    let endIndex = startIndex;
    while (endIndex < items.length && offsets[endIndex] <= windowBottom) {
      endIndex += 1;
    }
    const safeStart = Math.max(0, Math.min(startIndex, items.length - 1));
    const safeEnd = Math.max(safeStart + 1, Math.min(items.length, endIndex));
    const paddingTop = offsets[safeStart] ?? 0;
    const mountedHeight = heights.slice(safeStart, safeEnd).reduce((sum, height) => sum + height, 0);
    const paddingBottom = Math.max(0, totalHeight - paddingTop - mountedHeight);
    return {
      items: items.slice(safeStart, safeEnd).map((item, offset) => ({ index: safeStart + offset, item })),
      paddingTop,
      paddingBottom,
      measuring: true,
      virtualized: true,
    };
  }, [cacheScopeKey, items, measureVersion, viewport.height, viewport.scrollTop, viewport.width]);
  return { window: virtualWindow, onScroll, onRowHeight };
}

// conversationTimelineItemKey returns a stable measurement key for one timeline entry.
// conversationTimelineItemKey 返回单条时间线记录的稳定测量键。
function conversationTimelineItemKey(item: ConversationTimelineItem): string {
  return item.kind === "item" ? `item:${item.item.id}` : `process:${item.process.id}`;
}

// conversationTimelineItemEstimatedHeight returns a conservative initial height by entry kind.
// conversationTimelineItemEstimatedHeight 按记录类型返回保守初始高度。
function conversationTimelineItemEstimatedHeight(item: ConversationTimelineItem): number {
  return item.kind === "process"
    ? CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX
    : CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX;
}

// ConversationTimelineVirtualRow measures one mounted timeline entry for the virtual window.
// ConversationTimelineVirtualRow 为虚拟窗口测量单条已挂载时间线记录。
function ConversationTimelineVirtualRow(props: {
  // Stable row key used for height caching.
  // 用于高度缓存的稳定行键。
  rowKey: string;
  // Notifies the virtualizer when the row height changes.
  // 行高变化时通知虚拟列表。
  onHeight: (key: string, height: number) => void;
  // Mounted timeline content.
  // 已挂载的时间线内容。
  children: React.ReactNode;
}) {
  const elementRef = React.useRef<HTMLDivElement | null>(null);
  const { onHeight, rowKey } = props;
  React.useLayoutEffect(() => {
    const element = elementRef.current;
    if (!element) {
      return;
    }
    const updateHeight = () => {
      onHeight(rowKey, element.getBoundingClientRect().height);
    };
    updateHeight();
    if (typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(updateHeight);
    observer.observe(element);
    return () => observer.disconnect();
  }, [onHeight, rowKey]);
  return (
    <div className="timeline-virtual-row" ref={elementRef}>
      {props.children}
    </div>
  );
}

// ConversationWorkspace renders the conversation timeline replacement.
// ConversationWorkspace 渲染会话时间线替代组件。
function ConversationWorkspace(props: {
  conversation?: ConversationSummary;
  hiddenThinkingBlockId: string | null;
  messages: ConversationMessage[];
  // Parsed rows for the same message window.
  // 同一消息窗口对应的已解析展示行。
  renderItems: ConversationRenderItem[];
  error: string | null;
  parentConversationId: string | null;
  // Project workspace path used to resolve relative Markdown media references.
  // 用于解析 Markdown 相对媒体引用的项目工作区路径。
  workspacePath?: string | null;
  onScrollStateChange: () => void;
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  scrollRef: React.RefObject<HTMLElement | null>;
  // Initial bottom distance used before the virtual window has measured this surface.
  // 虚拟窗口测量当前界面前使用的初始底部距离。
  initialScrollDistanceFromBottom: number;
  // Previously measured virtualizer state for this transcript surface.
  // 当前转录界面此前测量过的虚拟列表状态。
  initialVirtualizerState: ConversationTimelineVirtualRestoreState | null;
  // Persists the latest measured virtualizer state for the active surface.
  // 持久保存当前界面最新测量到的虚拟列表状态。
  onVirtualizerStateChange: (surfaceId: string, state: ConversationTimelineVirtualRestoreState | null) => void;
  // Sticky footer rendered inside the transcript scroll container.
  // 渲染在转录滚动容器内部的粘性底部插槽。
  footer?: React.ReactNode;
  suppressEmptyPlaceholder?: boolean;
}) {
  // detail stores the currently opened transcript detail modal.
  // detail 保存当前打开的转录详情弹窗。
  const [detail, setDetail] = React.useState<ConversationDetail | null>(null);
  // mediaPlayer stores the generated media currently opened in the independent player window.
  // mediaPlayer 保存当前在独立播放窗口中打开的生成媒体。
  const [mediaPlayer, setMediaPlayer] = React.useState<GeneratedMediaAttachment | null>(null);
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  // processCollapseOverrides stores user-toggled process fold states for the current UI session.
  // processCollapseOverrides 存储当前 UI 会话中用户手动切换的处理过程折叠状态。
  const [processCollapseOverrides, setProcessCollapseOverrides] = React.useState<Map<string, boolean>>(
    () => new Map(conversationProcessCollapseMemory),
  );
  // userPromptExpansionOverrides mirrors WebUI-session-only user prompt expansion memory for virtualized rows.
  // userPromptExpansionOverrides 镜像仅当前 WebUI 会话有效的用户提示词展开记忆，供虚拟行复挂载使用。
  const [userPromptExpansionOverrides, setUserPromptExpansionOverrides] = React.useState<Set<string>>(
    () => new Set(conversationUserPromptExpansionMemory),
  );
  const sessionRunning = conversationIsRunning(props.conversation);
  const timelineItems = React.useMemo(
    () => projectConversationTimelineItems(props.renderItems, props.conversation, nowMs),
    [props.renderItems, props.conversation, nowMs],
  );
  const visibleTimelineItems = React.useMemo(
    () => timelineItems.filter((item) => timelineItemVisibleInMainTranscript(item, props.hiddenThinkingBlockId)),
    [props.hiddenThinkingBlockId, timelineItems],
  );
  const timelineCacheScopeKey = props.conversation?.id ?? props.parentConversationId ?? "conversation";
  const timelineVirtualizer = useConversationTimelineVirtualWindow(
    visibleTimelineItems,
    props.scrollRef,
    timelineCacheScopeKey,
    props.initialScrollDistanceFromBottom,
    props.initialVirtualizerState,
    props.onVirtualizerStateChange,
  );
  const autoExpandThinkingBlockId = sessionRunning && !props.hiddenThinkingBlockId
    ? latestThinkingBlockIdAfterLastUser(props.renderItems)
    : null;
  const processCollapseKey = React.useCallback((process: ConversationProcessRenderItem) => {
    const surfaceId = props.conversation?.id ?? props.parentConversationId ?? "unknown";
    return `${surfaceId}:${process.id}`;
  }, [props.conversation?.id, props.parentConversationId]);
  const handleProcessCollapsedChange = React.useCallback((process: ConversationProcessRenderItem, collapsed: boolean) => {
    const key = processCollapseKey(process);
    conversationProcessCollapseMemory.set(key, collapsed);
    setProcessCollapseOverrides((current) => {
      const next = new Map(current);
      next.set(key, collapsed);
      return next;
    });
  }, [processCollapseKey]);
  const handleUserPromptExpandedChange = React.useCallback((messageKey: string, expanded: boolean) => {
    if (expanded) {
      conversationUserPromptExpansionMemory.add(messageKey);
    } else {
      conversationUserPromptExpansionMemory.delete(messageKey);
    }
    setUserPromptExpansionOverrides(new Set(conversationUserPromptExpansionMemory));
  }, []);

  React.useEffect(() => {
    if (!sessionRunning) {
      return;
    }
    const interval = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, [sessionRunning]);

  const handleConversationScroll = React.useCallback(() => {
    timelineVirtualizer.onScroll();
    props.onScrollStateChange();
  }, [props, timelineVirtualizer]);

  const renderTimelineItem = (timelineItem: ConversationTimelineItem): React.ReactNode => timelineItem.kind === "item" ? (
    <ConversationRenderItemView
      item={timelineItem.item}
      autoExpandThinkingBlockId={autoExpandThinkingBlockId}
      hiddenThinkingBlockId={null}
      parentConversationId={props.parentConversationId}
      sessionRunning={sessionRunning}
      expandedUserPromptIds={userPromptExpansionOverrides}
      onOpenDetail={setDetail}
      onOpenMedia={setMediaPlayer}
      onOpenSubagentPanel={props.onOpenSubagentPanel}
      onUserPromptExpandedChange={handleUserPromptExpandedChange}
    />
  ) : (
    <ConversationProcessGroup
      autoExpandThinkingBlockId={autoExpandThinkingBlockId}
      collapsedOverride={processCollapseOverrides.get(processCollapseKey(timelineItem.process))}
      hiddenThinkingBlockId={props.hiddenThinkingBlockId}
      nowMs={nowMs}
      parentConversationId={props.parentConversationId}
      process={timelineItem.process}
      sessionRunning={sessionRunning}
      expandedUserPromptIds={userPromptExpansionOverrides}
      onCollapsedChange={(collapsed) => handleProcessCollapsedChange(timelineItem.process, collapsed)}
      onOpenDetail={setDetail}
      onOpenMedia={setMediaPlayer}
      onOpenSubagentPanel={props.onOpenSubagentPanel}
      onUserPromptExpandedChange={handleUserPromptExpandedChange}
    />
  );

  return (
    <MarkdownImageWorkspacePathContext.Provider value={props.workspacePath?.trim() || null}>
      <section
        className="conversation-panel"
        data-conversation-surface-id={props.conversation?.id ?? props.parentConversationId ?? undefined}
        onScroll={handleConversationScroll}
        ref={props.scrollRef}
      >
          <div className="conversation-scroll-content">
            <div className="timeline">
              {props.error ? (
                <article className="message-card system error-message">
                  <span>system</span>
                  <p>会话正文加载失败：{props.error}</p>
                </article>
              ) : null}
              {props.messages.length === 0 && !props.suppressEmptyPlaceholder ? (
                <article className="message-card system">
                  <span>system</span>
                  <p>{props.conversation ? "当前会话正文尚未载入或没有可见消息。" : "尚未选择会话，等待工作区数据载入。"}</p>
                </article>
              ) : null}
              {timelineVirtualizer.window.paddingTop > 0 ? (
                <div aria-hidden="true" className="timeline-virtual-spacer" style={{ height: timelineVirtualizer.window.paddingTop }} />
              ) : null}
              {timelineVirtualizer.window.items.map(({ item: timelineItem }) => {
                const rowKey = conversationTimelineItemKey(timelineItem);
                return timelineVirtualizer.window.measuring ? (
                  <ConversationTimelineVirtualRow
                    key={rowKey}
                    rowKey={rowKey}
                    onHeight={timelineVirtualizer.onRowHeight}
                  >
                    {renderTimelineItem(timelineItem)}
                  </ConversationTimelineVirtualRow>
                ) : (
                  <React.Fragment key={rowKey}>
                    {renderTimelineItem(timelineItem)}
                  </React.Fragment>
                );
              })}
              {timelineVirtualizer.window.paddingBottom > 0 ? (
                <div aria-hidden="true" className="timeline-virtual-spacer" style={{ height: timelineVirtualizer.window.paddingBottom }} />
              ) : null}
            </div>
            {props.footer ? (
              <div className="conversation-footer" data-conversation-scroll-footer="true">
                {props.footer}
              </div>
            ) : null}
          </div>
          {detail ? (
            <ConversationDetailModal
              detail={detail}
              onClose={() => setDetail(null)}
              onOpenMedia={setMediaPlayer}
            />
          ) : null}
          {mediaPlayer ? (
            <GeneratedMediaPlayerWindow attachment={mediaPlayer} onClose={() => setMediaPlayer(null)} />
          ) : null}
      </section>
    </MarkdownImageWorkspacePathContext.Provider>
  );
}

// renderItemHiddenByThinkingOverlay hides the whole render row that is mirrored by the live thinking overlay.
// renderItemHiddenByThinkingOverlay 隐藏被实时思考浮层镜像展示的整条渲染行。
function renderItemHiddenByThinkingOverlay(item: ConversationRenderItem, hiddenThinkingBlockId: string | null): boolean {
  return Boolean(hiddenThinkingBlockId && item.blocks.some((block) => block.id === hiddenThinkingBlockId));
}

// timelineItemVisibleInMainTranscript filters virtualized rows before height estimation.
// timelineItemVisibleInMainTranscript 在高度估算前过滤主转录中不可见的虚拟行。
function timelineItemVisibleInMainTranscript(item: ConversationTimelineItem, hiddenThinkingBlockId: string | null): boolean {
  return item.kind !== "item" || !renderItemHiddenByThinkingOverlay(item.item, hiddenThinkingBlockId);
}

// ConversationProcessGroup renders one per-turn process header and its foldable rows.
// ConversationProcessGroup 渲染单个轮次过程标题及其可折叠处理行。
function ConversationProcessGroup(props: {
  // autoExpandThinkingBlockId keeps parity with normal block rendering.
  // autoExpandThinkingBlockId 保持与普通块渲染一致。
  autoExpandThinkingBlockId: string | null;
  // Optional user-controlled collapsed state remembered by the conversation surface.
  // 会话界面记住的可选用户控制折叠状态。
  collapsedOverride?: boolean;
  // hiddenThinkingBlockId hides the block currently shown in the floating thinking overlay.
  // hiddenThinkingBlockId 隐藏当前在浮动思考面板中展示的块。
  hiddenThinkingBlockId: string | null;
  // Current clock tick used for live duration projection.
  // 用于实时耗时投影的当前时钟刻度。
  nowMs: number;
  // Parent conversation id used by subagent navigation.
  // 子代理导航使用的父会话标识。
  parentConversationId: string | null;
  // Process group payload.
  // 过程分组载荷。
  process: ConversationProcessRenderItem;
  // Whether the owning session is currently running.
  // 所属会话是否正在运行。
  sessionRunning: boolean;
  // WebUI-session expansion memory for long user prompt bubbles.
  // 长用户提示词气泡使用的当前 WebUI 会话展开记忆。
  expandedUserPromptIds: Set<string>;
  // Records one user-triggered collapsed state change.
  // 记录一次用户触发的折叠状态变化。
  onCollapsedChange: (collapsed: boolean) => void;
  // Detail modal opener.
  // 详情弹窗打开器。
  onOpenDetail: (detail: ConversationDetail) => void;
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Subagent panel opener.
  // 子代理面板打开器。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Records one user prompt expansion state change.
  // 记录一次用户提示词展开状态变化。
  onUserPromptExpandedChange: (messageKey: string, expanded: boolean) => void;
}) {
  const fallbackStartedAtMsRef = React.useRef(Date.now());
  React.useEffect(() => {
    fallbackStartedAtMsRef.current = Date.now();
  }, [props.process.id]);
  const collapsed = props.collapsedOverride ?? props.process.defaultCollapsed;
  const visibleGuidanceItems = props.process.guidanceItems.filter((item) => !renderItemHiddenByThinkingOverlay(item, props.hiddenThinkingBlockId));
  const visibleProcessItems = props.process.items.filter((item) => !renderItemHiddenByThinkingOverlay(item, props.hiddenThinkingBlockId));
  const streamingProcessItemId = props.process.running ? visibleProcessItems.at(-1)?.id ?? null : null;
  const liveDurationMs = props.process.running
    ? Math.max(0, props.nowMs - (props.process.startedAtMs ?? fallbackStartedAtMsRef.current))
    : null;
  const displayedDurationMs = props.process.durationMs ?? liveDurationMs;
  const durationLabel = formatWorkProcessDuration(displayedDurationMs);
  const headerText = durationLabel ? `处理过程 ${durationLabel}` : "处理过程";
  const stoppedText = durationLabel ? `你在 ${durationLabel} 后停止了` : "你已停止本轮工作";
  const bodyVisible = !props.process.collapsible || !collapsed;
  return (
    <section
      className={cn("work-process-group", props.process.running && "running", props.process.stopped && "stopped")}
      data-process-body-visible={bodyVisible ? "true" : "false"}
      data-process-collapsed={collapsed ? "true" : "false"}
    >
      <button
        aria-expanded={bodyVisible}
        className="work-process-toggle"
        disabled={!props.process.collapsible}
        onClick={() => props.process.collapsible && props.onCollapsedChange(!collapsed)}
        type="button"
      >
        <Clock3 className="work-process-icon" size={14} />
        <span>{headerText}</span>
        {props.process.collapsible ? (collapsed ? <ChevronRight size={13} /> : <ChevronDown size={13} />) : null}
      </button>
      <div className="work-process-guidance">
        {visibleGuidanceItems.map((item) => (
          <ConversationRenderItemView
            autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
            hiddenThinkingBlockId={null}
            item={item}
            itemStreaming={false}
            key={item.id}
            parentConversationId={props.parentConversationId}
            sessionRunning={props.sessionRunning}
            expandedUserPromptIds={props.expandedUserPromptIds}
            onOpenDetail={props.onOpenDetail}
            onOpenMedia={props.onOpenMedia}
            onOpenSubagentPanel={props.onOpenSubagentPanel}
            onUserPromptExpandedChange={props.onUserPromptExpandedChange}
            forceEagerMarkdown
          />
        ))}
      </div>
      <div aria-hidden={!bodyVisible} className="work-process-body" hidden={!bodyVisible}>
        {visibleProcessItems.map((item) => (
          <ConversationRenderItemView
            autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
            hiddenThinkingBlockId={null}
            item={item}
            itemStreaming={item.id === streamingProcessItemId}
            key={item.id}
            parentConversationId={props.parentConversationId}
            sessionRunning={props.sessionRunning}
            expandedUserPromptIds={props.expandedUserPromptIds}
            onOpenDetail={props.onOpenDetail}
            onOpenMedia={props.onOpenMedia}
            onOpenSubagentPanel={props.onOpenSubagentPanel}
            onUserPromptExpandedChange={props.onUserPromptExpandedChange}
            forceEagerMarkdown
          />
        ))}
        {props.process.stopped ? (
          <div className="work-process-stopped-note">
            {stoppedText}
          </div>
        ) : null}
      </div>
    </section>
  );
}

// ConversationRenderItemView renders one projected transcript item.
// ConversationRenderItemView 渲染单条投影后的转录展示项。
function ConversationRenderItemView(props: {
  item: ConversationRenderItem;
  autoExpandThinkingBlockId: string | null;
  hiddenThinkingBlockId: string | null;
  itemStreaming?: boolean;
  // Force Markdown blocks in externally hidden panels to stay mounted when visible.
  // 强制外层隐藏面板中的 Markdown 块在可见时保持直接挂载。
  forceEagerMarkdown?: boolean;
  parentConversationId: string | null;
  sessionRunning: boolean;
  // WebUI-session expansion memory for long user prompt bubbles.
  // 长用户提示词气泡使用的当前 WebUI 会话展开记忆。
  expandedUserPromptIds: Set<string>;
  onOpenDetail: (detail: ConversationDetail) => void;
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Records one user prompt expansion state change.
  // 记录一次用户提示词展开状态变化。
  onUserPromptExpandedChange: (messageKey: string, expanded: boolean) => void;
}) {
  const userBody = React.useMemo(
    () => stripUserPromptHeading(props.item.userMessage?.body ?? ""),
    [props.item.userMessage?.body],
  );
  const userPromptTextRef = React.useRef<HTMLParagraphElement | null>(null);
  const [userPromptOverflowing, setUserPromptOverflowing] = React.useState(false);
  const [userPromptCopied, setUserPromptCopied] = React.useState(false);
  const userPromptKey = React.useMemo(() => {
    const message = props.item.userMessage;
    const messageId = message?.id ?? props.item.id;
    const surfaceId = props.parentConversationId ?? message?.conversationId ?? "conversation";
    return `${surfaceId}:${messageId}`;
  }, [props.item.id, props.item.userMessage, props.parentConversationId]);
  const userPromptExpanded = props.expandedUserPromptIds.has(userPromptKey);
  const userTimestampMs = conversationMessageUnixMillis(props.item.userMessage);
  const userTimestamp = formatFullConversationTimestamp(props.item.userMessage);
  const handleCopyUserPrompt = React.useCallback(async () => {
    if (!userBody) {
      return;
    }
    await copyText(userBody);
    setUserPromptCopied(true);
  }, [userBody]);
  React.useLayoutEffect(() => {
    if (props.item.role !== "user" || !userBody) {
      setUserPromptOverflowing(false);
      return;
    }
    const element = userPromptTextRef.current;
    if (!element) {
      return;
    }
    const measureOverflow = () => {
      const computedStyle = window.getComputedStyle(element);
      const lineHeight = Number.parseFloat(computedStyle.lineHeight);
      const effectiveLineHeight = Number.isFinite(lineHeight) && lineHeight > 0 ? lineHeight : 20;
      const collapsedHeight = effectiveLineHeight * USER_PROMPT_COLLAPSE_LINE_LIMIT;
      setUserPromptOverflowing(element.scrollHeight > collapsedHeight + 1);
    };
    measureOverflow();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measureOverflow);
      return () => window.removeEventListener("resize", measureOverflow);
    }
    const observer = new ResizeObserver(measureOverflow);
    observer.observe(element);
    return () => observer.disconnect();
  }, [props.item.role, userBody, userPromptExpanded]);
  if (props.item.role === "user") {
    const attachments = props.item.userMessage?.attachments ?? [];
    return (
      <article className="message-row user">
        {attachments.length ? (
          <ComposerAttachmentStrip
            attachments={attachments}
            onOpenMedia={(attachment) => {
              const media = composerAttachmentMedia(attachment);
              if (media) {
                props.onOpenMedia(media);
              }
            }}
          />
        ) : null}
        <div className="user-bubble">
          {userBody ? (
            <p
              className={cn("user-prompt-text", userPromptOverflowing && !userPromptExpanded && "collapsed")}
              ref={userPromptTextRef}
            >
              {userBody}
            </p>
          ) : null}
          {userPromptOverflowing ? (
            <button
              aria-expanded={userPromptExpanded}
              className="user-prompt-expand-toggle"
              onClick={() => props.onUserPromptExpandedChange(userPromptKey, !userPromptExpanded)}
              type="button"
            >
              <span>{userPromptExpanded ? "收起" : "显示更多"}</span>
              {userPromptExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
            </button>
          ) : null}
        </div>
        <div className="user-message-meta">
          {userTimestamp && userTimestampMs ? (
            <time dateTime={new Date(userTimestampMs).toISOString()}>{userTimestamp}</time>
          ) : null}
          {userBody ? (
            <button
              aria-label="复制用户提示词"
              className="user-message-copy"
              onClick={() => void handleCopyUserPrompt()}
              onMouseLeave={() => setUserPromptCopied(false)}
              title="复制用户提示词"
              type="button"
            >
              {userPromptCopied ? <Check size={14} /> : <Copy size={14} />}
            </button>
          ) : null}
        </div>
      </article>
    );
  }

  const visibleBlocks = props.hiddenThinkingBlockId
    ? props.item.blocks.filter((block) => block.id !== props.hiddenThinkingBlockId)
    : props.item.blocks;
  if (visibleBlocks.length === 0) {
    return null;
  }

  return (
    <article className={`message-row ${props.item.role}`}>
      <div className="assistant-flow">
        {props.item.role === "system" ? <span className="message-role-label">SYSTEM</span> : null}
        <ConversationBlocksView
          autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
          blocks={visibleBlocks}
          conversationId={props.item.sourceMessage?.conversationId ?? props.item.userMessage?.conversationId ?? null}
          forceEagerMarkdown={props.forceEagerMarkdown}
          itemStreaming={Boolean(props.itemStreaming)}
          parentConversationId={props.parentConversationId}
          sessionRunning={props.sessionRunning}
          onOpenDetail={props.onOpenDetail}
          onOpenMedia={props.onOpenMedia}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
        />
      </div>
    </article>
  );
}

// stripUserPromptHeading removes legacy transcript headings from visible user bubbles.
// stripUserPromptHeading 从可见用户气泡中移除旧版转录标题。
function stripUserPromptHeading(body: string): string {
  return body
    .replace(/^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:用户指令|User Prompt|USER)(?:\*\*)?\s*\n+/i, "")
    .trim();
}

// ConversationBlockView dispatches one parsed block to its visual renderer.
// ConversationBlockView 将单个解析块分派给对应视觉渲染器。
function ConversationBlocksView(props: {
  autoExpandThinkingBlockId: string | null;
  blocks: ConversationBlock[];
  conversationId: string | null;
  // Force Markdown blocks in externally hidden panels to stay mounted when visible.
  // 强制外层隐藏面板中的 Markdown 块在可见时保持直接挂载。
  forceEagerMarkdown?: boolean;
  itemStreaming: boolean;
  parentConversationId: string | null;
  sessionRunning: boolean;
  onOpenDetail: (detail: ConversationDetail) => void;
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
}) {
  if (props.blocks.length > 1 && props.blocks.every(isMergeableToolBlock)) {
    return (
      <ToolExecutionBlock
        blocks={props.blocks}
        conversationId={props.conversationId}
        parentConversationId={props.parentConversationId}
        sessionRunning={props.sessionRunning}
        onOpenDetail={props.onOpenDetail}
        onOpenMedia={props.onOpenMedia}
        onOpenSubagentPanel={props.onOpenSubagentPanel}
      />
    );
  }
  if (props.blocks.some(isContextCompactionBlock)) {
    return (
      <ContextCompactionBlock
        blocks={props.blocks}
        onOpenMedia={props.onOpenMedia}
      />
    );
  }

  const [block] = props.blocks;
  if (!block) {
    return null;
  }
  if (block.kind === "markdown") {
    return (
      <MarkdownMessage
        content={block.body}
        eager={props.forceEagerMarkdown}
        streaming={props.itemStreaming}
        onOpenMedia={props.onOpenMedia}
      />
    );
  }
  return (
    <SpecialConversationBlock
      autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
      block={block}
      conversationId={props.conversationId}
      itemStreaming={props.itemStreaming}
      parentConversationId={props.parentConversationId}
      sessionRunning={props.sessionRunning}
      onOpenDetail={props.onOpenDetail}
      onOpenMedia={props.onOpenMedia}
      onOpenSubagentPanel={props.onOpenSubagentPanel}
    />
  );
}

// MarkdownMessage renders assistant Markdown with Vulcan-specific component styling.
// MarkdownMessage 使用 Vulcan 定制组件样式渲染 assistant Markdown。
const MarkdownMessage = React.memo(function MarkdownMessage(props: {
  // Markdown source content to render.
  // 需要渲染的 Markdown 源内容。
  content: string;
  // Whether to bypass viewport lazy mounting for overlays and modal surfaces.
  // 是否为浮层和弹窗界面绕过视口懒挂载。
  eager?: boolean;
  // Whether the content is still growing and should avoid parsing incomplete block syntax.
  // 内容是否仍在增长，需要避免解析未闭合的块级语法。
  streaming?: boolean;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  const workspacePath = React.useContext(MarkdownImageWorkspacePathContext);
  // finalizedContent keeps non-streaming Markdown normalized only when the stable body changes.
  // finalizedContent 让非流式 Markdown 只在稳定正文变化时执行归一化。
  const finalizedContent = React.useMemo(
    () => props.streaming ? props.content : normalizeMarkdownLocalImageLinks(props.content, workspacePath),
    [props.content, props.streaming, workspacePath],
  );
  return (
    <div className="markdown-message">
      {props.streaming ? (
        <StreamingMarkdownMessage
          content={props.content}
          onOpenMedia={props.onOpenMedia}
          workspacePath={workspacePath}
        />
      ) : (
        <LazyMarkdownMessage
          content={finalizedContent}
          eager={props.eager}
          onOpenMedia={props.onOpenMedia}
          workspacePath={workspacePath}
        />
      )}
    </div>
  );
});

// StreamingMarkdownMessage renders the active Markdown block as one complete region.
// StreamingMarkdownMessage 将活跃 Markdown 块作为完整区域渲染。
function StreamingMarkdownMessage(props: {
  // Raw Markdown content for the active streaming block.
  // 活跃流式块的原始 Markdown 内容。
  content: string;
  // Current project workspace path for image source resolution.
  // 当前项目工作区路径，用于解析图片来源。
  workspacePath: string | null;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  const normalizedContent = React.useMemo(
    () => normalizeMarkdownLocalImageLinks(props.content, props.workspacePath),
    [props.content, props.workspacePath],
  );
  return (
    <MarkdownContentRenderer
      content={normalizedContent}
      onOpenMedia={props.onOpenMedia}
      workspacePath={props.workspacePath}
    />
  );
}

// MarkdownContentRenderer renders one already-normalized Markdown fragment.
// MarkdownContentRenderer 渲染单个已经规范化的 Markdown 片段。
function MarkdownContentRenderer(props: {
  // Markdown fragment to render.
  // 需要渲染的 Markdown 片段。
  content: string;
  // Current project workspace path for image source resolution.
  // 当前项目工作区路径，用于解析图片来源。
  workspacePath: string | null;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  const components = React.useMemo(() => ({
    a: ({ children, href }: { children?: React.ReactNode; href?: string }) => (
      <a href={href} rel="noreferrer" target="_blank">{children}</a>
    ),
    code: ({ children, className }: { children?: React.ReactNode; className?: string }) => {
      const inline = !className;
      return inline ? <code className="inline-code">{children}</code> : <code className={className}>{children}</code>;
    },
    img: ({ alt, src, title }: { alt?: string; src?: string; title?: string }) => {
      const width = markdownImageWidthFromTitle(title);
      const imageSource = normalizeMarkdownImageSource(src, props.workspacePath) ?? src;
      const imageAttachment = markdownImageAttachment(imageSource, alt);
      const imageStyle = width ? { width, maxWidth: 800 } : undefined;
      if (imageAttachment && props.onOpenMedia) {
        return (
          <button
            className="markdown-image-button"
            onClick={() => props.onOpenMedia?.(imageAttachment)}
            style={imageStyle}
            type="button"
          >
            <img
              alt={alt ?? ""}
              decoding="async"
              loading="lazy"
              src={imageSource}
            />
          </button>
        );
      }
      return (
        <img
          alt={alt ?? ""}
          decoding="async"
          loading="lazy"
          src={imageSource}
          style={imageStyle}
        />
      );
    },
    pre: ({ children }: { children?: React.ReactNode }) => <pre className="markdown-code-block">{children}</pre>,
    table: ({ children }: { children?: React.ReactNode }) => <div className="markdown-table-wrap"><table>{children}</table></div>,
  }), [props.onOpenMedia, props.workspacePath]);
  return (
    <ReactMarkdown
      components={components}
      rehypePlugins={markdownSanitizers}
      remarkPlugins={markdownPlugins}
    >
      {props.content}
    </ReactMarkdown>
  );
}

// LazyMarkdownMessage keeps one complete Markdown document mounted for stable layout.
// LazyMarkdownMessage 保持完整 Markdown 文档挂载以获得稳定布局。
function LazyMarkdownMessage(props: {
  // Already-normalized Markdown content.
  // 已经规范化的 Markdown 内容。
  content: string;
  // Kept for call-site compatibility after removing chunk-level unmounting.
  // 移除分块卸载后保留该参数以兼容现有调用点。
  eager?: boolean;
  // Current project workspace path for image source resolution.
  // 当前项目工作区路径，用于解析图片来源。
  workspacePath: string | null;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  return (
    <MarkdownContentRenderer
      content={props.content}
      onOpenMedia={props.onOpenMedia}
      workspacePath={props.workspacePath}
    />
  );
}

// SpecialConversationBlock renders compact tool, command, diff, thinking, and notice rows.
// SpecialConversationBlock 渲染紧凑的工具、命令、diff、思考与提示行。
function SpecialConversationBlock(props: {
  autoExpandThinkingBlockId: string | null;
  block: ConversationBlock;
  conversationId: string | null;
  itemStreaming: boolean;
  parentConversationId: string | null;
  sessionRunning: boolean;
  onOpenDetail: (detail: ConversationDetail) => void;
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
}) {
  if (props.block.kind === "turn-stats") {
    return <TurnStatsBlock block={props.block} />;
  }

  if (isMergeableToolBlock(props.block)) {
    return (
      <ToolExecutionBlock
        blocks={[props.block]}
        conversationId={props.conversationId}
        parentConversationId={props.parentConversationId}
        sessionRunning={props.sessionRunning}
        onOpenDetail={props.onOpenDetail}
        onOpenMedia={props.onOpenMedia}
        onOpenSubagentPanel={props.onOpenSubagentPanel}
      />
    );
  }
  if (props.block.kind === "thinking") {
    return (
      <ThinkingBlock
        autoExpanded={props.block.id === props.autoExpandThinkingBlockId}
        block={props.block}
        sessionRunning={props.sessionRunning}
        streaming={props.itemStreaming}
        onOpenMedia={props.onOpenMedia}
      />
    );
  }

  const Icon = blockIcon(props.block.kind);
  const canOpen = detailEligibleBlockKinds.has(props.block.kind);
  return (
    <button
      className={`special-block ${props.block.kind}`}
      disabled={!canOpen}
      onClick={() => {
        if (canOpen) {
          props.onOpenDetail({
            title: props.block.title,
            kind: props.block.kind,
            body: props.block.body,
            metadata: [
              ["类型", blockTitle(props.block.kind, props.block.toolName)],
              ...(props.block.status ? [["状态", props.block.status] as [string, string]] : []),
              ...(props.block.toolName ? [["工具", props.block.toolName] as [string, string]] : []),
            ],
          });
        }
      }}
      type="button"
    >
      <div className="special-block-main">
        <Icon size={16} />
        <div>
          <strong>{props.block.title}</strong>
          <span>{props.block.summary}</span>
        </div>
      </div>
      {props.block.status ? <StatusPill status={props.block.status} label={props.block.status} /> : null}
      {canOpen ? <Maximize2 size={15} /> : null}
    </button>
  );
}

// ContextCompactionBlock renders the original compacted context status section as one collapsible row.
// ContextCompactionBlock 将原本的上下文压缩状态分段渲染为单个可折叠行。
function ContextCompactionBlock(props: {
  // Section blocks contain start, optional body, and optional end rows.
  // 分段块包含开始、可选正文和可选结束行。
  blocks: ConversationBlock[];
  // onOpenMedia opens generated media referenced from markdown content.
  // onOpenMedia 打开 Markdown 内容引用的生成媒体。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  const startBlock = props.blocks.find((block) => block.compactionRole === "start") ?? props.blocks[0];
  const thinkingBlocks = props.blocks.filter((block) => block.compactionRole === "thinking" && block.kind === "thinking");
  const bodyBlock = props.blocks.find((block) => block.compactionRole === "body");
  const endBlock = props.blocks.find((block) => block.compactionRole === "end");
  const state = endBlock?.compactionState ?? bodyBlock?.compactionState ?? startBlock?.compactionState ?? "running";
  const running = state === "running";
  const displayStatus = running ? "running" : state === "failed" ? "failed" : "completed";
  const [expanded, setExpanded] = React.useState(running);
  const sectionIdRef = React.useRef(startBlock?.compactionSectionId ?? startBlock?.id ?? "");

  React.useEffect(() => {
    const sectionId = startBlock?.compactionSectionId ?? startBlock?.id ?? "";
    if (sectionIdRef.current !== sectionId) {
      sectionIdRef.current = sectionId;
      setExpanded(running);
      return;
    }
    if (!running) {
      setExpanded(false);
    }
  }, [running, startBlock?.compactionSectionId, startBlock?.id]);

  const title = running ? (startBlock?.title ?? "上下文正在压缩") : (endBlock?.title ?? "上下文已压缩");
  const detailBody = bodyBlock?.body?.trim() || (displayStatus === "failed" ? endBlock?.body?.trim() ?? "" : "");
  const hasExpandedContent = thinkingBlocks.length > 0 || Boolean(detailBody);

  return (
    <section className={cn("context-compaction-block", displayStatus)}>
      <button
        aria-expanded={expanded}
        className="context-compaction-toggle"
        onClick={() => setExpanded((current) => !current)}
        type="button"
      >
        <span className="context-compaction-title">
          <ScrollText size={15} />
          <strong>{title}</strong>
          {expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        </span>
      </button>
      {expanded ? (
        <div className="context-compaction-body">
          {thinkingBlocks.map((block) => {
            const content = block.body.trim() || block.summary || "暂无思考内容";
            return (
              <div className="context-compaction-thinking thinking-block" key={block.id}>
                <div className="thinking-body">
                  <MarkdownMessage content={`**Thinking:** ${content}`} eager onOpenMedia={props.onOpenMedia} />
                </div>
              </div>
            );
          })}
          {detailBody ? (
            <MarkdownMessage content={detailBody} onOpenMedia={props.onOpenMedia} />
          ) : !hasExpandedContent ? (
            <span>正在生成上下文压缩总结。</span>
          ) : null}
          <div className="context-compaction-end-line" aria-hidden="true" />
        </div>
      ) : null}
    </section>
  );
}

// ThinkingBlock renders thinking content directly inside the transcript.
// ThinkingBlock 在会话转录中直接渲染思考内容。
function ThinkingBlock(props: {
  // autoExpanded requests the current turn thinking block to open after the floating stream hides.
  // autoExpanded 请求当前 turn 的思考块在流式浮层隐藏后自动展开。
  autoExpanded: boolean;
  // block contains the thinking transcript payload.
  // block 包含思考转录载荷。
  block: ConversationBlock;
  // sessionRunning collapses the current turn thinking block once the turn completes.
  // sessionRunning 用于在当前 turn 完成后自动折叠思考块。
  sessionRunning: boolean;
  // streaming marks the currently growing thinking row.
  // streaming 标记当前仍在增长的思考行。
  streaming: boolean;
  // onOpenMedia opens generated media referenced from markdown content.
  // onOpenMedia 打开 Markdown 内容引用的生成媒体。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  const content = props.block.body.trim() || props.block.summary || "暂无思考内容";
  return (
    <section className="thinking-block">
      <div className="thinking-body">
        <MarkdownMessage content={`**Thinking:** ${content}`} eager streaming={props.streaming} onOpenMedia={props.onOpenMedia} />
      </div>
    </section>
  );
}

// ToolExecutionBlock renders one compact merged tool execution row.
// ToolExecutionBlock 渲染单条紧凑的合并工具执行行。
function ToolExecutionBlock(props: {
  blocks: ConversationBlock[];
  conversationId: string | null;
  parentConversationId: string | null;
  sessionRunning: boolean;
  onOpenDetail: (detail: ConversationDetail) => void;
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
}) {
  const callBlock = props.blocks.find((block) => block.kind === "tool-call");
  const resultBlock = props.blocks.find((block) => block.kind === "tool-result");
  const commandBlock = props.blocks.find((block) => block.kind === "command");
  const primaryBlock = preferredTaskSessionDisplayBlock(props.blocks) ?? resultBlock ?? commandBlock ?? callBlock ?? props.blocks[0];
  if (!primaryBlock) {
    return null;
  }

  const summary = toolExecutionSummary(props.blocks, props.sessionRunning);
  const toolName = toolExecutionName(props.blocks);
  const displayState = toolExecutionDisplayState(props.blocks, props.sessionRunning);
  const mediaAttachments = toolExecutionMediaAttachments(props.blocks);
  const taskSessionId = props.blocks.find((block) => block.taskSessionId)?.taskSessionId ?? null;
  const identifier = toolExecutionIdentifier(props.blocks);
  const errorPreview = toolExecutionErrorPreview(props.blocks);
  const detailRefFields = toolExecutionDetailRef(props.blocks);
  const detailRef = props.conversationId && (detailRefFields || primaryBlock.toolCallId)
    ? { sessionId: props.conversationId, ...(detailRefFields ?? {}), ...(primaryBlock.toolCallId ? { toolCallId: primaryBlock.toolCallId } : {}) }
    : undefined;
  const hydratedMediaAttachments = useToolDetailMediaAttachments(
    detailRef,
    mediaAttachments.length === 0 && toolExecutionMayHaveGeneratedMedia(toolName),
  );
  const displayedMediaAttachments = mergeGeneratedMediaAttachments(mediaAttachments, hydratedMediaAttachments);
  const metadata = [
    ...(toolName ? [["工具", toolName] as [string, string]] : []),
    ...(identifier ? [["标识", identifier] as [string, string]] : []),
    ...(errorPreview ? [["错误", errorPreview] as [string, string]] : []),
    ["状态", toolDisplayStateLabel(displayState)] as [string, string],
    ...(resultBlock ? [["结果", "已返回"] as [string, string]] : []),
  ];

  const openFallbackDetail = () => {
    webuiClickTrace("tool.open_fallback_detail", {
      toolName,
      displayState,
      primaryKind: primaryBlock.kind,
      taskSessionId,
      blockKinds: props.blocks.map((block) => block.kind),
    });
    props.onOpenDetail({
      title: summary,
      kind: primaryBlock.kind,
      body: buildToolExecutionDetailBody(props.blocks),
      detailRef,
      metadata,
      mediaAttachments: displayedMediaAttachments,
    });
  };
  const openDetail = () => {
    webuiClickTrace("tool.open", {
      toolName,
      displayState,
      taskSessionId,
      parentConversationId: props.parentConversationId,
      blockKinds: props.blocks.map((block) => block.kind),
    });
    if (props.parentConversationId && taskSessionId) {
      void props.onOpenSubagentPanel(props.parentConversationId, taskSessionId).then((opened) => {
        webuiClickTrace("tool.open_subagent_result", {
          toolName,
          taskSessionId,
          opened,
          parentConversationId: props.parentConversationId,
        });
        if (!opened) {
          openFallbackDetail();
        }
      });
      return;
    }
    openFallbackDetail();
  };

  return (
    <div className="tool-execution-stack">
      <button
        className={cn("tool-execution-row", displayState === "running" && "running")}
        onClick={openDetail}
        type="button"
      >
        <Wrench size={15} />
        <span>{summary}</span>
      </button>
      {displayedMediaAttachments.length > 0 ? (
        <GeneratedMediaStrip
          attachments={displayedMediaAttachments}
          onOpenDetail={openDetail}
          onOpenMedia={props.onOpenMedia}
        />
      ) : null}
    </div>
  );
}

// useToolDetailMediaAttachments hydrates generated media from hidden tool details when the visible call row is compact.
// useToolDetailMediaAttachments 在可见工具调用行较紧凑时，从隐藏工具详情中水合生成媒体。
function useToolDetailMediaAttachments(
  detailRef: ConversationDetailRef | undefined,
  enabled: boolean,
): GeneratedMediaAttachment[] {
  const detailCacheKey = detailRef ? conversationDetailRefCacheKey(detailRef) : null;
  const [attachments, setAttachments] = React.useState<GeneratedMediaAttachment[]>(
    () => detailCacheKey ? conversationToolDetailMediaCache.get(detailCacheKey) ?? [] : [],
  );
  React.useEffect(() => {
    const cachedAttachments = detailCacheKey
      ? conversationToolDetailMediaCache.get(detailCacheKey)
      : undefined;
    setAttachments(cachedAttachments ?? []);
    if (
      !enabled
      || !detailRef
      || (!detailRef.recordId && !detailRef.toolCallId && detailRef.seq == null && detailRef.messageIndex == null)
    ) {
      return;
    }
    let cancelled = false;
    void loadCachedConversationToolDetail(detailRef)
      .then((payload) => {
        if (!cancelled && payload) {
          const nextAttachments = generatedMediaAttachmentsFromToolDetailPayload(payload);
          if (detailCacheKey) {
            conversationToolDetailMediaCache.set(detailCacheKey, nextAttachments);
          }
          setAttachments(nextAttachments);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAttachments([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    detailCacheKey,
    detailRef?.messageIndex,
    detailRef?.recordId,
    detailRef?.seq,
    detailRef?.sessionId,
    detailRef?.toolCallId,
    enabled,
  ]);
  return attachments;
}

// toolExecutionMayHaveGeneratedMedia gates detail hydration to media-producing tools.
// toolExecutionMayHaveGeneratedMedia 将详情水合限制在会生成媒体的工具上。
function toolExecutionMayHaveGeneratedMedia(toolName: string | null): boolean {
  const normalized = toolName?.trim().toLowerCase();
  return normalized === "image_generation"
    || normalized === "video_generation"
    || normalized === "speech_synthesis"
    || normalized === "music_generation";
}

// mergeGeneratedMediaAttachments combines visible and hydrated media without duplicate cards.
// mergeGeneratedMediaAttachments 合并可见与水合媒体，并避免重复卡片。
function mergeGeneratedMediaAttachments(
  visibleAttachments: GeneratedMediaAttachment[],
  hydratedAttachments: GeneratedMediaAttachment[],
): GeneratedMediaAttachment[] {
  const seen = new Set<string>();
  const merged: GeneratedMediaAttachment[] = [];
  for (const attachment of [...visibleAttachments, ...hydratedAttachments]) {
    const key = `${attachment.kind}:${attachment.localPath}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    merged.push(attachment);
  }
  return merged;
}

// buildToolExecutionDetailBody builds the expensive modal body only when the user opens details.
// buildToolExecutionDetailBody 仅在用户打开详情时构建成本较高的弹窗正文。
function buildToolExecutionDetailBody(blocks: ConversationBlock[]): string {
  return blocks
    .map((block) => {
      const sections = [`## ${blockTitle(block.kind, block.toolName)}`];
      const argumentsValue = metadataValue(block.rawMetadata, ["arguments", "payload.arguments"]);
      if (argumentsValue !== undefined) {
        sections.push(`### Arguments\n\n\`\`\`json\n${JSON.stringify(argumentsValue, null, 2)}\n\`\`\``);
      }
      const resultStatus = metadataString(block.rawMetadata, ["result.status"]);
      const detailRecordId = metadataString(block.rawMetadata, [
        "result.detail_ref.record_id",
        "result.detailRef.recordId",
      ]);
      if (resultStatus || detailRecordId) {
        sections.push(`### Result\n\n${[
          resultStatus ? `Status: ${resultStatus}` : null,
          detailRecordId ? `Detail Record: ${detailRecordId}` : null,
        ].filter(Boolean).join("\n")}`);
      }
      if (block.body.trim()) {
        sections.push(block.body);
      }
      return sections.join("\n\n");
    })
    .join("\n\n---\n\n");
}

// toolExecutionSummary builds the compact transcript row summary for one tool execution.
// toolExecutionSummary 为单次工具执行构建紧凑转录行摘要。
function toolExecutionSummary(blocks: ConversationBlock[], sessionRunning: boolean): string {
  const commandBlock = blocks.find((block) => block.kind === "command");
  const toolName = toolExecutionName(blocks);
  const displayState = toolExecutionDisplayState(blocks, sessionRunning);
  const statusLabel = toolDisplayStateLabel(displayState);
  const identifier = toolExecutionIdentifier(blocks);
  const errorPreview = displayState === "failed" ? toolExecutionErrorPreview(blocks) : null;
  if (commandBlock && !toolName) {
    return commandBlock.summary.includes("命令")
      ? commandBlock.summary
      : "已运行 1 条命令";
  }

  if (toolName) {
    return `调用 ${toolName} 工具 · ${statusLabel}${errorPreview ? ` · ${errorPreview}` : ""}`;
  }
  return `工具执行 · ${statusLabel}${identifier ? ` · ${identifier}` : ""}${errorPreview ? ` · ${errorPreview}` : ""}`;
}

// toolExecutionName resolves the best human-visible tool name across merged call and result blocks.
// toolExecutionName 从合并后的调用与返回块中解析最佳人类可见工具名。
function toolExecutionName(blocks: ConversationBlock[]): string | null {
  for (const block of blocks) {
    const name = block.toolName ?? extractToolNameFromText(block.title);
    if (name) {
      return name;
    }
  }
  return null;
}

// toolExecutionIdentifier resolves the most useful fallback identifier for a tool row.
// toolExecutionIdentifier 为工具行解析最有用的兜底标识。
function toolExecutionIdentifier(blocks: ConversationBlock[]): string | null {
  for (const block of blocks) {
    const identifier = block.toolCallId
      ?? block.toolResultId
      ?? block.commandSessionId
      ?? block.taskSessionId
      ?? metadataString(block.rawMetadata, [
        "tool_call_id",
        "toolCallId",
        "payload.tool_call_id",
        "payload.toolCallId",
        "host_result.payload.tool_call_id",
        "hostResult.payload.toolCallId",
        "task_session_id",
        "taskSessionId",
        "payload.task_session_id",
        "payload.taskSessionId",
        "host_result.payload.task_session_id",
        "hostResult.payload.taskSessionId",
      ]);
    if (identifier) {
      return shortenToolExecutionIdentifier(identifier);
    }
  }
  return null;
}

// toolExecutionDetailRef resolves the hidden full-detail reference for one tool execution.
// toolExecutionDetailRef 解析单次工具执行对应的隐藏完整详情引用。
function toolExecutionDetailRef(blocks: ConversationBlock[]): Omit<ConversationDetailRef, "sessionId"> | null {
  for (const block of blocks) {
    const recordId = metadataString(block.rawMetadata, [
      "result.detail_ref.record_id",
      "result.detailRef.recordId",
    ]);
    if (recordId) {
      const seq = numericMetadata(metadataValue(block.rawMetadata, [
        "result.detail_ref.seq",
        "result.detailRef.seq",
      ]));
      const messageIndex = numericMetadata(metadataValue(block.rawMetadata, [
        "result.detail_ref.message_index",
        "result.detailRef.messageIndex",
      ]));
      return {
        recordId,
        ...(seq !== null ? { seq: Math.trunc(seq) } : {}),
        ...(messageIndex !== null ? { messageIndex: Math.trunc(messageIndex) } : {}),
      };
    }
  }
  return null;
}

// shortenToolExecutionIdentifier keeps long UUID-like ids readable in compact rows.
// shortenToolExecutionIdentifier 让较长的 UUID 类标识在紧凑行中保持可读。
function shortenToolExecutionIdentifier(identifier: string): string {
  const trimmed = identifier.trim();
  if (trimmed.length <= 18) {
    return trimmed;
  }
  return `${trimmed.slice(0, 8)}...${trimmed.slice(-6)}`;
}

// toolExecutionErrorPreview extracts a compact failure reason from metadata or body text.
// toolExecutionErrorPreview 从元数据或正文中提取紧凑失败原因。
function toolExecutionErrorPreview(blocks: ConversationBlock[]): string | null {
  for (const block of blocks) {
    const metadataError = metadataString(block.rawMetadata, [
      "result.error",
      "result.reason",
      "host_result.payload.message",
      "hostResult.payload.message",
      "host_result.payload.error",
      "hostResult.payload.error",
      "host_result.payload.reason",
      "hostResult.payload.reason",
      "payload.message",
      "payload.error",
      "payload.reason",
      "message",
      "error",
      "reason",
    ]);
    if (metadataError) {
      return compactToolExecutionErrorPreview(metadataError);
    }
  }
  const failedBlock = blocks.find((block) => normalizeToolDisplayState(block.status) === "failed")
    ?? blocks.find((block) => /失败|报错|错误|failed|error|panic/i.test(block.body));
  const preview = failedBlock?.body || failedBlock?.summary;
  return preview ? compactToolExecutionErrorPreview(preview) : null;
}

// compactToolExecutionErrorPreview normalizes verbose error text for one-line tool summaries.
// compactToolExecutionErrorPreview 将冗长错误文本规整为工具摘要中的单行内容。
function compactToolExecutionErrorPreview(source: string): string | null {
  const line = source
    .split(/\r?\n/u)
    .map((part) => part.trim())
    .find((part) => part.length > 0 && !/^#+\s/.test(part));
  if (!line) {
    return null;
  }
  return compactTextPreview(line).slice(0, 96);
}

// preferredTaskSessionDisplayBlock keeps routed task-session rows anchored to the newest terminal metadata.
// preferredTaskSessionDisplayBlock 让已路由 task-session 行优先使用最新终态元数据作为展示锚点。
function preferredTaskSessionDisplayBlock(blocks: ConversationBlock[]): ConversationBlock | null {
  const taskBlocks = blocks.filter((block) => block.taskSessionId);
  if (!taskBlocks.length) {
    return null;
  }
  return taskBlocks.find((block) => {
    const state = normalizeToolDisplayState(block.status);
    return state === "failed" || state === "stopped" || state === "completed";
  }) ?? taskBlocks.find((block) => block.kind === "tool-call") ?? taskBlocks[0] ?? null;
}

// taskSessionExecutionDisplayState resolves the visible state for routed task-session rows from explicit task metadata.
// taskSessionExecutionDisplayState 从显式 task 元数据解析已路由 task-session 行的可见状态。
function taskSessionExecutionDisplayState(blocks: ConversationBlock[]): "failed" | "completed" | "running" | "stopped" | null {
  const taskBlocks = blocks.filter((block) => block.taskSessionId);
  if (!taskBlocks.length) {
    return null;
  }
  const states = taskBlocks
    .map((block) => normalizeToolDisplayState(block.status))
    .filter((state): state is string => Boolean(state));
  if (states.includes("failed")) {
    return "failed";
  }
  if (states.includes("stopped")) {
    return "stopped";
  }
  if (states.includes("completed")) {
    return "completed";
  }
  if (states.includes("running")) {
    return "running";
  }
  return null;
}

// toolExecutionDisplayState resolves the stable compact display state for one tool execution row.
// toolExecutionDisplayState 解析单条工具执行行使用的稳定紧凑显示状态。
function toolExecutionDisplayState(blocks: ConversationBlock[], sessionRunning: boolean): "failed" | "completed" | "running" | "stopped" {
  const taskSessionState = taskSessionExecutionDisplayState(blocks);
  if (taskSessionState) {
    return taskSessionState;
  }
  const explicitStates = blocks
    .map((block) => normalizeToolDisplayState(block.status))
    .filter((state): state is string => Boolean(state));
  if (explicitStates.includes("failed")) {
    return "failed";
  }
  if (explicitStates.includes("stopped")) {
    return "stopped";
  }
  if (explicitStates.includes("completed")) {
    return "completed";
  }
  if (blocks.some((block) => block.toolResultId || toolExecutionDetailRef([block]))) {
    return "completed";
  }
  if (blocks.some((block) => block.kind === "tool-result" || block.kind === "command")) {
    return "completed";
  }
  if (!sessionRunning) {
    return "stopped";
  }
  return "running";
}

// toolExecutionMediaAttachments resolves generated media outputs across merged tool call/result rows.
// toolExecutionMediaAttachments 从合并后的工具调用/返回行中解析生成媒体输出。
function toolExecutionMediaAttachments(blocks: ConversationBlock[]): GeneratedMediaAttachment[] {
  const seen = new Set<string>();
  const attachments: GeneratedMediaAttachment[] = [];
  for (const block of blocks) {
    for (const attachment of block.mediaAttachments ?? []) {
      const key = `${attachment.kind}:${attachment.localPath}`;
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      attachments.push(attachment);
    }
  }
  return attachments;
}

// GeneratedMediaStrip renders generated image, audio, and video outputs directly under their tool row.
// GeneratedMediaStrip 在工具行下直接渲染生成图片、音频和视频输出。
function GeneratedMediaStrip(props: {
  // attachments are the generated media outputs for one tool execution.
  // attachments 是单次工具执行的生成媒体输出。
  attachments: GeneratedMediaAttachment[];
  // onOpenDetail opens the full tool detail modal.
  // onOpenDetail 打开完整工具详情弹窗。
  onOpenDetail: () => void;
  // onOpenMedia opens the independent media playback window.
  // onOpenMedia 打开独立媒体播放窗口。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  return (
    <div className="generated-media-strip">
      {props.attachments.map((attachment) => (
        <GeneratedMediaInlineCard
          attachment={attachment}
          key={`${attachment.kind}:${attachment.localPath}`}
          onOpenDetail={props.onOpenDetail}
          onOpenMedia={props.onOpenMedia}
        />
      ))}
    </div>
  );
}

// GeneratedMediaInlineCard renders one compact media preview inside the transcript.
// GeneratedMediaInlineCard 在转录中渲染单条紧凑媒体预览。
function GeneratedMediaInlineCard(props: {
  // attachment describes the generated file to preview.
  // attachment 描述需要预览的生成文件。
  attachment: GeneratedMediaAttachment;
  // onOpenDetail opens the parent tool detail modal.
  // onOpenDetail 打开父级工具详情弹窗。
  onOpenDetail: () => void;
  // onOpenMedia opens the independent media playback window.
  // onOpenMedia 打开独立媒体播放窗口。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  const source = useGeneratedMediaSource(
    props.attachment.previewLocalPath ?? props.attachment.localPath,
    props.attachment.mimeType,
  );
  const MediaIcon = generatedMediaIcon(props.attachment.kind);
  if (props.attachment.kind === "image") {
    return (
      <button className="generated-media-card image" onClick={() => props.onOpenMedia(props.attachment)} type="button">
        <img alt={props.attachment.fileName} decoding="async" loading="lazy" src={source} />
        <span>{props.attachment.fileName}</span>
      </button>
    );
  }
  if (props.attachment.kind === "video") {
    const mediaSource = useGeneratedMediaSource(props.attachment.localPath, props.attachment.mimeType);
    return (
      <button
        className="generated-media-card video"
        onClick={() => props.onOpenMedia(props.attachment)}
        type="button"
      >
        <span className="generated-media-video-thumb">
          {props.attachment.previewLocalPath ? (
            <img alt={props.attachment.fileName} decoding="async" loading="lazy" src={source} />
          ) : (
            <video
              aria-label={props.attachment.fileName}
              muted
              playsInline
              preload="metadata"
              src={mediaSource}
            />
          )}
          <span className="generated-media-play-mark"><Play size={18} fill="currentColor" /></span>
        </span>
        <span>{props.attachment.fileName}</span>
      </button>
    );
  }
  if (props.attachment.kind === "audio") {
    const mediaSource = useGeneratedMediaSource(props.attachment.localPath, props.attachment.mimeType);
    return (
      <div className="generated-media-card audio">
        <audio controls preload="metadata" src={mediaSource} />
      </div>
    );
  }
  return (
    <button className="generated-media-card document" onClick={props.onOpenDetail} type="button">
      <MediaIcon size={18} />
      <span>{props.attachment.fileName}</span>
    </button>
  );
}

// generatedMediaIcon returns the icon used by generated media cards and modals.
// generatedMediaIcon 返回生成媒体卡片与弹窗使用的图标。
function generatedMediaIcon(kind: GeneratedMediaKind) {
  if (kind === "image") return ImageIcon;
  if (kind === "video") return Video;
  if (kind === "audio") return Volume2;
  return FileText;
}

// toolDisplayStateLabel returns the localized label used inside compact tool rows.
// toolDisplayStateLabel 返回紧凑工具行中使用的本地化状态文案。
function toolDisplayStateLabel(status: "failed" | "completed" | "running" | string): string {
  if (status === "failed") return "失败";
  if (status === "completed") return "已完成";
  if (status === "stopped") return "已停止";
  return "进行中";
}

// TurnStatsBlock renders the compact non-clickable per-turn statistics row.
// TurnStatsBlock 渲染不可点击的紧凑按轮次统计横条。
function TurnStatsBlock(props: { block: ConversationBlock }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className={`turn-stats-row ${props.block.status ?? "completed"}`}>
          <span aria-hidden="true">{turnStatsIcon(props.block.status)}</span>
          <strong>{props.block.summary || props.block.title}</strong>
        </div>
      </TooltipTrigger>
      <TooltipContent className="turn-stats-tooltip" side="right" sideOffset={8}>
        <pre>{props.block.body}</pre>
      </TooltipContent>
    </Tooltip>
  );
}

// turnStatsIcon returns the compact status mark used by turn statistics.
// turnStatsIcon 返回轮次统计使用的紧凑状态标记。
function turnStatsIcon(status?: string): string {
  if (status === "failed") return "!";
  if (status === "stopped") return "■";
  return "◔";
}

// blockIcon returns the icon component used by one special transcript block.
// blockIcon 返回单个特殊转录块使用的图标组件。
function blockIcon(kind: ConversationBlockKind) {
  if (kind === "diff") return FileText;
  if (kind === "thinking") return BrainCircuit;
  if (kind === "command") return TerminalSquare;
  if (kind === "tool-call" || kind === "tool-result") return Hammer;
  return ScrollText;
}

// renderToolDetailPayloadMarkdown converts structured tool detail metadata into Markdown for the modal.
// renderToolDetailPayloadMarkdown 将结构化工具详情元数据转换为弹窗使用的 Markdown。
function renderToolDetailPayloadMarkdown(payload: ToolDetailPayload): string {
  const display = metadataValue(payload.metadata, ["display"]);
  const modelOutput = payload.body;
  const displayMarkdown = renderToolDisplayMarkdown(display);
  const sections = displayMarkdown ? [`## Structured Display\n\n${displayMarkdown}`] : [];
  if (modelOutput.trim()) {
    sections.push(`## Model Output\n\n${fencedMarkdownBlock(modelOutput, "")}`);
  }
  return sections.join("\n\n---\n\n") || payload.body;
}

// renderToolDisplayMarkdown renders a structured display payload without parsing model-output text.
// renderToolDisplayMarkdown 渲染结构化展示载荷，不解析模型组合输出文本。
function renderToolDisplayMarkdown(display: unknown): string | null {
  const object = objectValue(display);
  if (!object) {
    return null;
  }
  const kind = stringValue(object.kind);
  if (kind === "shell_command") {
    return renderShellCommandDisplayMarkdown(object);
  }
  if (kind === "vulcan_file_list") {
    return renderFileListDisplayMarkdown(object);
  }
  const status = stringValue(object.status);
  const summary = stringValue(object.summary);
  const sections = [
    `Kind: ${kind ?? "generic"}`,
    status ? `Status: ${status}` : null,
  ].filter(Boolean);
  if (object.arguments !== undefined) {
    sections.push(`Arguments:\n\n${fencedMarkdownBlock(JSON.stringify(object.arguments, null, 2), "json")}`);
  }
  if (summary) {
    sections.push(`Summary: ${summary}`);
  }
  return sections.join("\n\n");
}

// renderFileListDisplayMarkdown renders structured file-list fields without duplicating raw output.
// renderFileListDisplayMarkdown 渲染结构化文件列表字段，避免重复原始输出。
function renderFileListDisplayMarkdown(display: Record<string, unknown>): string {
  const rows = [
    ["Root", stringValue(display.root)],
    ["Pattern", stringValue(display.pattern)],
    ["Recursive", display.recursive === undefined ? null : String(display.recursive)],
    ["Ignore", display.ignore === undefined ? null : String(display.ignore)],
    ["Files", display.files_count === undefined ? null : String(display.files_count)],
    ["Truncated", display.truncated === undefined ? null : String(display.truncated)],
    ["Ignored", display.ignored_count === undefined ? null : String(display.ignored_count)],
    ["Status", stringValue(display.status)],
  ] as Array<[string, string | null]>;
  const sections = rows
    .filter(([, value]) => value)
    .map(([label, value]) => `- ${label}: ${value}`);
  const argumentsValue = display.arguments;
  if (argumentsValue !== undefined) {
    sections.push(`### Arguments\n\n${fencedMarkdownBlock(JSON.stringify(argumentsValue, null, 2), "json")}`);
  }
  const files = Array.isArray(display.files)
    ? display.files.map((file) => String(file)).filter(Boolean)
    : [];
  if (files.length > 0) {
    sections.push(`### Files\n\n${files.map((file) => `- \`${file}\``).join("\n")}`);
  }
  return sections.join("\n");
}

// renderShellCommandDisplayMarkdown renders shell_command fields as stable rows and stream blocks.
// renderShellCommandDisplayMarkdown 将 shell_command 字段渲染为稳定信息行和输出流块。
function renderShellCommandDisplayMarkdown(display: Record<string, unknown>): string {
  const rows = [
    ["Command", stringValue(display.command)],
    ["Shell", stringValue(display.shell)],
    ["Working directory", stringValue(display.working_directory)],
    ["Status", stringValue(display.status)],
    ["Duration", stringValue(display.duration)],
    ["Exit code", display.exit_code === undefined ? null : String(display.exit_code)],
  ] as Array<[string, string | null]>;
  const sections = rows
    .filter(([, value]) => value)
    .map(([label, value]) => `- ${label}: ${value}`);
  const stdout = stringValue(display.stdout);
  const stderr = stringValue(display.stderr);
  if (stdout) {
    sections.push(`### stdout\n\n${fencedMarkdownBlock(stdout, "")}`);
  }
  if (stderr) {
    sections.push(`### stderr\n\n${fencedMarkdownBlock(stderr, "")}`);
  }
  return sections.join("\n");
}

// fencedMarkdownBlock wraps arbitrary text in a Markdown code fence with backtick escaping.
// fencedMarkdownBlock 使用 Markdown 代码围栏包裹任意文本并规避反引号冲突。
function fencedMarkdownBlock(source: string, language: string): string {
  const fence = source.includes("```") ? "````" : "```";
  return `${fence}${language}\n${source}\n${fence}`;
}

// objectValue narrows an unknown JSON value into a plain object record.
// objectValue 将未知 JSON 值收窄为普通对象记录。
function objectValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

// ConversationDetailModal renders the enlarged detail surface for special transcript blocks.
// ConversationDetailModal 渲染特殊转录块的放大详情界面。
function ConversationDetailModal(props: {
  detail: ConversationDetail;
  onClose: () => void;
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  // loadedToolDetail stores the hidden structured tool result loaded after the modal opens.
  // loadedToolDetail 保存弹窗打开后按需加载到的隐藏结构化工具结果。
  const [loadedToolDetail, setLoadedToolDetail] = React.useState<ToolDetailPayload | null>(null);
  // detailLoadError stores a short failure note for on-demand detail hydration.
  // detailLoadError 保存按需详情水合失败时的简短提示。
  const [detailLoadError, setDetailLoadError] = React.useState<string | null>(null);
  React.useEffect(() => {
    setLoadedToolDetail(null);
    setDetailLoadError(null);
    const detailRef = props.detail.detailRef;
    if (!detailRef) {
      return;
    }
    let cancelled = false;
    void loadCachedConversationToolDetail(detailRef)
      .then((payload) => {
        if (!cancelled) {
          setLoadedToolDetail(payload);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setDetailLoadError(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    props.detail.detailRef?.messageIndex,
    props.detail.detailRef?.recordId,
    props.detail.detailRef?.seq,
    props.detail.detailRef?.sessionId,
    props.detail.detailRef?.toolCallId,
  ]);
  const DetailIcon = blockIcon(props.detail.kind);
  const detailType = blockTitle(props.detail.kind);
  const detailSubtitle = props.detail.metadata
    ?.map(([label, value]) => `${label}: ${value}`)
    .join(" · ");
  const fullBody = loadedToolDetail
    ? `${props.detail.body}\n\n---\n\n${renderToolDetailPayloadMarkdown(loadedToolDetail)}`
    : detailLoadError
      ? `${props.detail.body}\n\n---\n\nDetail load failed: ${detailLoadError}`
      : props.detail.body;
  // diffSnapshot stores the structured view of the current diff detail body.
  // diffSnapshot 保存当前 diff 详情正文对应的结构化视图。
  const diffBody = props.detail.body;
  const diffSnapshot = React.useMemo(
    () => props.detail.kind === "diff" && hasRenderableDiff(diffBody)
      ? parseUnifiedDiff(diffBody, "message", "")
      : null,
    [diffBody, props.detail.kind],
  );
  const diffDetailSnapshot = diffSnapshot ?? {
    source: "message",
    repositoryRoot: "",
    files: [],
    stats: { files: 0, additions: 0, deletions: 0 },
    notice: "当前 diff 内容无法结构化解析，已保留原始文本供复制。",
    rawDiff: fullBody,
  } satisfies DiffPanelSnapshot;
  return (
    <Dialog open onOpenChange={(open) => !open && props.onClose()}>
      <DialogContent className={cn("detail-modal sm:max-w-[min(1080px,96vw)]", props.detail.kind === "thinking" && "thinking-detail-modal")} showCloseButton={false}>
        <header className="detail-modal-header">
          <div className="detail-title-row">
            <div className="detail-title-icon">
              <DetailIcon size={18} />
            </div>
            <div className="detail-title-copy">
              <span>{detailType}</span>
              <DialogTitle>{props.detail.title}</DialogTitle>
              {detailSubtitle ? <p>{detailSubtitle}</p> : null}
            </div>
          </div>
          <div className="detail-modal-actions">
            <Button aria-label="复制详情" className="icon-button" onClick={() => void copyText(fullBody)} size="icon" type="button" variant="outline">
              <Copy size={17} />
            </Button>
            <Button aria-label="关闭详情" className="icon-button" onClick={props.onClose} size="icon" type="button" variant="outline">
              <X size={17} />
            </Button>
          </div>
        </header>
        <div className="detail-modal-body">
          {props.detail.mediaAttachments?.length ? (
            <GeneratedMediaDetail
              attachments={props.detail.mediaAttachments}
              onOpenMedia={props.onOpenMedia}
            />
          ) : null}
          {props.detail.kind === "diff" ? (
            <DiffReviewPanel
              emptyBody="当前 diff 内容无法结构化解析，已保留原始文本供复制。"
              emptyTitle="Git Diff"
              snapshot={diffDetailSnapshot}
              onCopyRaw={(rawDiff) => void copyText(rawDiff)}
            />
          ) : (
            <MarkdownMessage content={fullBody} eager onOpenMedia={props.onOpenMedia} />
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ConversationLoadingCenter renders the cache-miss conversation hydration placeholder.
// ConversationLoadingCenter 渲染无缓存会话水合时的居中占位提示。
function ConversationLoadingCenter() {
  return (
    <section className="conversation-loading-center" aria-live="polite" aria-busy="true">
      <ShinyText text="✨ 正在加载会话信息" />
    </section>
  );
}

// clampGeneratedMediaZoom keeps image zoom values inside the supported media viewer range.
// clampGeneratedMediaZoom 将图片缩放值限制在媒体查看器支持范围内。
function clampGeneratedMediaZoom(value: number): number {
  return Math.min(GENERATED_MEDIA_MAX_ZOOM, Math.max(GENERATED_MEDIA_MIN_ZOOM, value));
}

// generatedMediaFitScale returns the best fit scale relative to the original image size.
// generatedMediaFitScale 返回相对图片原始尺寸的最佳适配缩放比例。
function generatedMediaFitScale(
  viewportSize: GeneratedMediaImageSize | null,
  naturalSize: GeneratedMediaImageSize | null,
): number | null {
  if (!viewportSize || !naturalSize || viewportSize.width <= 0 || viewportSize.height <= 0 || naturalSize.width <= 0 || naturalSize.height <= 0) {
    return null;
  }
  return clampGeneratedMediaZoom(Math.min(1, viewportSize.width / naturalSize.width, viewportSize.height / naturalSize.height));
}

// clampGeneratedMediaImageOffset keeps a panned image reachable inside the viewport.
// clampGeneratedMediaImageOffset 将拖拽后的图片偏移限制在视口可达范围内。
function clampGeneratedMediaImageOffset(
  offset: GeneratedMediaImageOffset,
  scale: number,
  viewportSize: GeneratedMediaImageSize | null,
  naturalSize: GeneratedMediaImageSize | null,
): GeneratedMediaImageOffset {
  if (!viewportSize || !naturalSize || viewportSize.width <= 0 || viewportSize.height <= 0 || naturalSize.width <= 0 || naturalSize.height <= 0) {
    return { x: 0, y: 0 };
  }
  const scaledWidth = naturalSize.width * scale;
  const scaledHeight = naturalSize.height * scale;
  const maxX = Math.max(0, (scaledWidth - viewportSize.width) / 2);
  const maxY = Math.max(0, (scaledHeight - viewportSize.height) / 2);
  return {
    x: Math.min(maxX, Math.max(-maxX, offset.x)),
    y: Math.min(maxY, Math.max(-maxY, offset.y)),
  };
}

// generatedMediaZoomOffsetAroundPoint preserves the visual anchor while changing scale.
// generatedMediaZoomOffsetAroundPoint 在缩放时保持鼠标或中心锚点的视觉位置。
function generatedMediaZoomOffsetAroundPoint(
  offset: GeneratedMediaImageOffset,
  currentScale: number,
  nextScale: number,
  anchor: GeneratedMediaImageOffset,
  viewportSize: GeneratedMediaImageSize | null,
  naturalSize: GeneratedMediaImageSize | null,
): GeneratedMediaImageOffset {
  if (currentScale <= 0) {
    return clampGeneratedMediaImageOffset(offset, nextScale, viewportSize, naturalSize);
  }
  const ratio = nextScale / currentScale;
  return clampGeneratedMediaImageOffset({
    x: anchor.x - (anchor.x - offset.x) * ratio,
    y: anchor.y - (anchor.y - offset.y) * ratio,
  }, nextScale, viewportSize, naturalSize);
}

// generatedMediaHasImageOffset returns whether the image is currently panned.
// generatedMediaHasImageOffset 返回图片当前是否存在拖拽偏移。
function generatedMediaHasImageOffset(offset: GeneratedMediaImageOffset): boolean {
  return Math.abs(offset.x) > 0.5 || Math.abs(offset.y) > 0.5;
}

// GeneratedMediaDetail renders media outputs in a zoomable/player-capable detail modal section.
// GeneratedMediaDetail 在详情弹窗中渲染可缩放或可播放的媒体输出区域。
function GeneratedMediaDetail(props: {
  // attachments are generated media outputs carried by the opened tool detail.
  // attachments 是当前工具详情携带的生成媒体输出。
  attachments: GeneratedMediaAttachment[];
  // onOpenMedia opens the independent media playback window.
  // onOpenMedia 打开独立媒体播放窗口。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  const [zoom, setZoom] = React.useState(1);
  const primary = props.attachments[0];
  const primarySource = useGeneratedMediaSource(primary?.localPath ?? "", primary?.mimeType);
  if (!primary) {
    return null;
  }
  const MediaIcon = generatedMediaIcon(primary.kind);
  return (
    <section className="generated-media-detail">
      <header className="generated-media-detail-header">
        <span><MediaIcon size={17} />{primary.fileName}</span>
        {primary.kind === "image" ? (
          <span className="generated-media-zoom-controls">
            <Button onClick={() => setZoom((value) => clampGeneratedMediaZoom(value - GENERATED_MEDIA_ZOOM_STEP))} size="sm" type="button" variant="outline">-</Button>
            <strong>{Math.round(zoom * 100)}%</strong>
            <Button onClick={() => setZoom((value) => clampGeneratedMediaZoom(value + GENERATED_MEDIA_ZOOM_STEP))} size="sm" type="button" variant="outline">+</Button>
          </span>
        ) : null}
      </header>
      {primary.kind === "image" ? (
        <div className="generated-media-image-preview">
          <img
            alt={primary.fileName}
            src={primarySource}
            style={{ transform: `scale(${zoom})` }}
          />
        </div>
      ) : primary.kind === "video" ? (
        <button
          className="generated-media-open-player"
          onClick={() => props.onOpenMedia(primary)}
          type="button"
        >
          <span className="generated-media-open-player-icon">
            <Play size={22} fill="currentColor" />
          </span>
          <span>
            <strong>打开视频播放器</strong>
            <small>{primary.fileName}</small>
          </span>
        </button>
      ) : primary.kind === "audio" ? (
        <audio className="generated-media-audio-preview" controls src={primarySource} />
      ) : (
        <a className="generated-media-document-link" href={primarySource} rel="noreferrer" target="_blank">
          <FileText size={18} />
          {primary.fileName}
        </a>
      )}
    </section>
  );
}

// GeneratedMediaPlayerWindow renders a dialog viewer for generated media assets.
// GeneratedMediaPlayerWindow 为生成媒体资源渲染弹窗查看器。
function GeneratedMediaPlayerWindow(props: {
  // attachment describes the generated media file to play.
  // attachment 描述需要播放或查看的生成媒体文件。
  attachment: GeneratedMediaAttachment;
  // onClose closes the media player dialog.
  // onClose 关闭媒体播放弹窗。
  onClose: () => void;
}) {
  const imageStageRef = React.useRef<HTMLDivElement | null>(null);
  const imageElementRef = React.useRef<HTMLImageElement | null>(null);
  const imageInitializedRef = React.useRef(false);
  const imageDragRef = React.useRef<{
    // Pointer id captured by the image stage during one drag.
    // 单次拖拽期间图片舞台捕获的指针标识。
    pointerId: number;
    // Pointer x coordinate at drag start.
    // 拖拽开始时的指针 x 坐标。
    startX: number;
    // Pointer y coordinate at drag start.
    // 拖拽开始时的指针 y 坐标。
    startY: number;
    // Image offset at drag start.
    // 拖拽开始时的图片偏移。
    startOffset: GeneratedMediaImageOffset;
  } | null>(null);
  const [imageScale, setImageScale] = React.useState(1);
  const [imageOffset, setImageOffset] = React.useState<GeneratedMediaImageOffset>({ x: 0, y: 0 });
  const [isImageDragging, setIsImageDragging] = React.useState(false);
  const [isMaximized, setIsMaximized] = React.useState(false);
  const [imageNaturalSize, setImageNaturalSize] = React.useState<GeneratedMediaImageSize | null>(null);
  const [imageViewportSize, setImageViewportSize] = React.useState<GeneratedMediaImageSize | null>(null);
  const source = useGeneratedMediaSource(props.attachment.localPath, props.attachment.mimeType);
  const MediaIcon = generatedMediaIcon(props.attachment.kind);
  const isImage = props.attachment.kind === "image";
  const imageFitScale = generatedMediaFitScale(imageViewportSize, imageNaturalSize) ?? 1;
  // measureImageViewportSize reads the live viewer bounds and stores usable image viewport dimensions.
  // measureImageViewportSize 读取实时查看器边界并保存可用的图片视口尺寸。
  const measureImageViewportSize = React.useCallback((): GeneratedMediaImageSize | null => {
    const node = imageStageRef.current;
    if (!node || !isImage) {
      return null;
    }
    const rect = node.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      return null;
    }
    const nextSize = { width: rect.width, height: rect.height };
    setImageViewportSize((current) => (
      current && Math.abs(current.width - nextSize.width) < 0.5 && Math.abs(current.height - nextSize.height) < 0.5
        ? current
        : nextSize
    ));
    return nextSize;
  }, [isImage]);
  React.useEffect(() => {
    imageInitializedRef.current = false;
    imageDragRef.current = null;
    imageElementRef.current = null;
    setImageScale(1);
    setImageOffset({ x: 0, y: 0 });
    setIsImageDragging(false);
    setIsMaximized(false);
    setImageNaturalSize(null);
    setImageViewportSize(null);
  }, [source]);
  React.useEffect(() => {
    if (!isImage || !source) {
      return undefined;
    }
    let cancelled = false;
    const image = new Image();
    image.onload = () => {
      if (!cancelled) {
        setImageNaturalSize({ width: image.naturalWidth, height: image.naturalHeight });
      }
    };
    image.src = source;
    return () => {
      cancelled = true;
    };
  }, [isImage, source]);
  const syncImageNaturalSize = React.useCallback((image: HTMLImageElement | null) => {
    if (!image || image.naturalWidth <= 0 || image.naturalHeight <= 0) {
      return;
    }
    setImageNaturalSize({ width: image.naturalWidth, height: image.naturalHeight });
  }, []);
  React.useLayoutEffect(() => {
    const node = imageStageRef.current;
    if (!node || !isImage) {
      return undefined;
    }
    measureImageViewportSize();
    const animationFrame = window.requestAnimationFrame(measureImageViewportSize);
    const observer = new ResizeObserver(measureImageViewportSize);
    observer.observe(node);
    return () => {
      window.cancelAnimationFrame(animationFrame);
      observer.disconnect();
    };
  }, [isImage, isMaximized, measureImageViewportSize]);
  React.useEffect(() => {
    syncImageNaturalSize(imageElementRef.current);
  }, [syncImageNaturalSize, source]);
  React.useEffect(() => {
    if (imageInitializedRef.current || !imageNaturalSize || !imageViewportSize) {
      return;
    }
    imageInitializedRef.current = true;
    setImageScale(generatedMediaFitScale(imageViewportSize, imageNaturalSize) ?? 1);
    setImageOffset({ x: 0, y: 0 });
  }, [imageNaturalSize, imageViewportSize]);
  React.useEffect(() => {
    setImageOffset((offset) => clampGeneratedMediaImageOffset(offset, imageScale, imageViewportSize, imageNaturalSize));
  }, [imageNaturalSize, imageScale, imageViewportSize]);
  const applyImageScaleAroundCenter = React.useCallback((nextScale: number) => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    if (!imageNaturalSize || !viewportSize) {
      return;
    }
    const clampedScale = clampGeneratedMediaZoom(nextScale);
    setImageOffset((offset) => generatedMediaZoomOffsetAroundPoint(
      offset,
      imageScale,
      clampedScale,
      { x: 0, y: 0 },
      viewportSize,
      imageNaturalSize,
    ));
    setImageScale(clampedScale);
  }, [imageNaturalSize, imageScale, imageViewportSize, measureImageViewportSize]);
  const handleImageFit = React.useCallback(() => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    const nextScale = generatedMediaFitScale(viewportSize, imageNaturalSize);
    if (nextScale === null) {
      return;
    }
    setImageScale(nextScale);
    setImageOffset({ x: 0, y: 0 });
  }, [imageNaturalSize, imageViewportSize, measureImageViewportSize]);
  const handleImageOriginalSize = React.useCallback(() => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    if (!imageNaturalSize || !viewportSize) {
      return;
    }
    setImageScale(1);
    setImageOffset({ x: 0, y: 0 });
  }, [imageNaturalSize, imageViewportSize, measureImageViewportSize]);
  const zoomImageAtPoint = React.useCallback((deltaY: number, clientX: number, clientY: number, target: HTMLElement) => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    if (!imageNaturalSize || !viewportSize) {
      return;
    }
    const rect = target.getBoundingClientRect();
    const anchor = {
      x: clientX - rect.left - rect.width / 2,
      y: clientY - rect.top - rect.height / 2,
    };
    const nextScale = clampGeneratedMediaZoom(imageScale * (deltaY < 0 ? 1.1 : 1 / 1.1));
    setImageOffset((offset) => generatedMediaZoomOffsetAroundPoint(
      offset,
      imageScale,
      nextScale,
      anchor,
      viewportSize,
      imageNaturalSize,
    ));
    setImageScale(nextScale);
  }, [imageNaturalSize, imageScale, imageViewportSize, measureImageViewportSize]);
  React.useEffect(() => {
    const node = imageStageRef.current;
    if (!node || !isImage) {
      return undefined;
    }
    const handleWheel = (event: WheelEvent) => {
      event.preventDefault();
      zoomImageAtPoint(event.deltaY, event.clientX, event.clientY, node);
    };
    node.addEventListener("wheel", handleWheel, { passive: false });
    return () => node.removeEventListener("wheel", handleWheel);
  }, [isImage, zoomImageAtPoint]);
  const handleImagePointerDown = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    if (event.button !== 0 || !imageNaturalSize || !viewportSize) {
      return;
    }
    event.preventDefault();
    imageDragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      startOffset: imageOffset,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
    setIsImageDragging(true);
  }, [imageNaturalSize, imageOffset, imageViewportSize, measureImageViewportSize]);
  const handleImagePointerMove = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const drag = imageDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }
    event.preventDefault();
    setImageOffset(clampGeneratedMediaImageOffset({
      x: drag.startOffset.x + event.clientX - drag.startX,
      y: drag.startOffset.y + event.clientY - drag.startY,
    }, imageScale, imageViewportSize, imageNaturalSize));
  }, [imageNaturalSize, imageScale, imageViewportSize]);
  const handleImagePointerEnd = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const drag = imageDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }
    imageDragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setIsImageDragging(false);
  }, []);

  return (
    <Dialog open onOpenChange={(open) => !open && props.onClose()}>
      <DialogContent
        className={cn(
          "generated-media-player-window sm:max-w-[min(1120px,96vw)]",
          isMaximized && "maximized",
        )}
        showCloseButton={false}
      >
        <header className="generated-media-player-header">
          <span className="generated-media-player-title">
            <MediaIcon size={18} />
            <DialogTitle>{props.attachment.fileName}</DialogTitle>
          </span>
          <span className="generated-media-player-actions">
            {isImage ? (
              <GeneratedMediaImageZoomControl
                canUseZoom={imageNaturalSize !== null && imageViewportSize !== null}
                fitScale={imageFitScale}
                naturalSize={imageNaturalSize}
                offset={imageOffset}
                onFit={handleImageFit}
                onOriginalSize={handleImageOriginalSize}
                onZoomIn={() => applyImageScaleAroundCenter(imageScale + GENERATED_MEDIA_ZOOM_STEP)}
                onZoomOut={() => applyImageScaleAroundCenter(imageScale - GENERATED_MEDIA_ZOOM_STEP)}
                scale={imageScale}
              />
            ) : null}
            <Button
              aria-label={isMaximized ? "还原播放器" : "最大化播放器"}
              className="icon-button"
              onClick={() => setIsMaximized((value) => !value)}
              size="icon"
              type="button"
              variant="outline"
            >
              {isMaximized ? <Minimize2 size={17} /> : <Maximize2 size={17} />}
            </Button>
            <Button aria-label="关闭播放器" className="icon-button" onClick={props.onClose} size="icon" type="button" variant="outline">
              <X size={17} />
            </Button>
          </span>
        </header>
        <div
          className={cn("generated-media-player-body", isImage && "image")}
        >
          {isImage ? (
            <div
              className={cn("generated-media-image-viewer", isImageDragging && "dragging")}
              onPointerCancel={handleImagePointerEnd}
              onPointerDown={handleImagePointerDown}
              onPointerMove={handleImagePointerMove}
              onPointerUp={handleImagePointerEnd}
              ref={imageStageRef}
            >
              {imageNaturalSize ? (
                <div
                  className="generated-media-image-canvas"
                  style={{
                    transform: `translate(calc(-50% + ${imageOffset.x}px), calc(-50% + ${imageOffset.y}px))`,
                  }}
                >
                  <img
                    alt={props.attachment.fileName}
                    className="generated-media-player-image"
                    draggable={false}
                    onLoad={(event) => syncImageNaturalSize(event.currentTarget)}
                    ref={imageElementRef}
                    src={source}
                    style={{
                      height: imageNaturalSize.height,
                      transform: `scale(${imageScale})`,
                      width: imageNaturalSize.width,
                    }}
                  />
                </div>
              ) : (
                <img
                  alt={props.attachment.fileName}
                  className="generated-media-player-image loading"
                  draggable={false}
                  onLoad={(event) => syncImageNaturalSize(event.currentTarget)}
                  ref={imageElementRef}
                  src={source}
                />
              )}
            </div>
          ) : props.attachment.kind === "video" ? (
            <video
              className="generated-media-player-video"
              controls
              preload="metadata"
              src={source}
            />
          ) : props.attachment.kind === "audio" ? (
            <audio
              className="generated-media-player-audio"
              controls
              preload="metadata"
              src={source}
            />
          ) : (
            <a className="generated-media-document-link" href={source} rel="noreferrer" target="_blank">
              <FileText size={18} />
              {props.attachment.fileName}
            </a>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

// GeneratedMediaImageZoomControl renders the lightweight image zoom toolbar in the dialog header.
// GeneratedMediaImageZoomControl 在弹窗标题栏渲染轻量图片缩放工具条。
function GeneratedMediaImageZoomControl(props: {
  // Whether the image has enough measurements to accept zoom actions.
  // 图片是否已有足够尺寸信息来接受缩放操作。
  canUseZoom: boolean;
  // Fit-to-window scale relative to the image's original size.
  // 相对图片原始尺寸的自适应窗口缩放比例。
  fitScale: number;
  // naturalSize is the loaded image's original pixel size.
  // naturalSize 是已加载图片的原始像素尺寸。
  naturalSize: GeneratedMediaImageSize | null;
  // offset is the current pan offset.
  // offset 是当前拖拽平移偏移。
  offset: GeneratedMediaImageOffset;
  // Restores fit-to-window scale and centered position.
  // 恢复自适应窗口缩放与居中位置。
  onFit: () => void;
  // Restores original image size and centered position.
  // 恢复图片原始尺寸与居中位置。
  onOriginalSize: () => void;
  // Increases the image scale.
  // 放大图片。
  onZoomIn: () => void;
  // Decreases the image scale.
  // 缩小图片。
  onZoomOut: () => void;
  // Current scale relative to the image's original size.
  // 当前相对图片原始尺寸的缩放比例。
  scale: number;
}) {
  const hasOffset = generatedMediaHasImageOffset(props.offset);
  const percent = Math.round(props.scale * 100);
  const canUseZoom = props.canUseZoom && props.naturalSize !== null;
  const canZoomOut = canUseZoom && props.scale > GENERATED_MEDIA_MIN_ZOOM + 0.001;
  const canZoomIn = canUseZoom && props.scale < GENERATED_MEDIA_MAX_ZOOM - 0.001;
  const canFitWindow = canUseZoom && (Math.abs(props.scale - props.fitScale) > 0.001 || hasOffset);
  const canUseOriginalSize = canUseZoom && (Math.abs(props.scale - 1) > 0.001 || hasOffset);
  return (
    <span className="generated-media-image-zoom-control">
      <GeneratedMediaImageZoomIconButton
        disabled={!canFitWindow}
        icon={<Scan size={16} />}
        label="自适应窗口"
        onClick={props.onFit}
      />
      <GeneratedMediaImageZoomIconButton
        disabled={!canUseOriginalSize}
        icon={<SquareIcon size={15} />}
        label="原始尺寸"
        onClick={props.onOriginalSize}
      />
      <GeneratedMediaImageZoomIconButton
        disabled={!canZoomOut}
        icon={<Minus size={16} />}
        label="缩小图片"
        onClick={props.onZoomOut}
      />
      <strong>{percent}%</strong>
      <GeneratedMediaImageZoomIconButton
        disabled={!canZoomIn}
        icon={<Plus size={16} />}
        label="放大图片"
        onClick={props.onZoomIn}
      />
    </span>
  );
}

// GeneratedMediaImageZoomIconButton renders one icon-only image zoom toolbar action with a tooltip.
// GeneratedMediaImageZoomIconButton 渲染带提示的单个纯图标图片缩放工具按钮。
function GeneratedMediaImageZoomIconButton(props: {
  // Whether the action is currently unavailable.
  // 当前操作是否不可用。
  disabled: boolean;
  // Icon rendered inside the toolbar button.
  // 工具按钮内渲染的图标。
  icon: React.ReactNode;
  // Tooltip and accessibility label.
  // Tooltip 与无障碍标签。
  label: string;
  // Click handler for the toolbar action.
  // 工具按钮点击处理函数。
  onClick: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="generated-media-image-zoom-trigger">
          <Button
            aria-label={props.label}
            className="icon-button"
            disabled={props.disabled}
            onClick={props.onClick}
            size="icon-sm"
            title={props.label}
            type="button"
            variant="ghost"
          >
            {props.icon}
          </Button>
        </span>
      </TooltipTrigger>
      <TooltipContent sideOffset={6}>{props.label}</TooltipContent>
    </Tooltip>
  );
}

// activeStreamingThinkingBlock returns the selected session thinking block that should float while streaming.
// activeStreamingThinkingBlock 返回当前选中会话在流式思考时应浮动展示的思考块。
function activeStreamingThinkingBlock(
  conversation: ConversationSummary | undefined,
  renderItems: ConversationRenderItem[],
): ConversationBlock | null {
  if (normalizedSessionRunPhase(conversation?.runPhase) !== "thinking") {
    return null;
  }
  const blocks = renderItems
    .filter((item) => item.role !== "user")
    .flatMap((item) => item.blocks);
  return [...blocks].reverse().find((block) => block.kind === "thinking") ?? null;
}

// StreamingThinkingOverlay renders the active thinking stream as a floating session-local surface.
// StreamingThinkingOverlay 将活动思考流渲染为当前会话内的浮动面板。
function StreamingThinkingOverlay(props: {
  // block contains the currently streaming thinking transcript.
  // block 包含当前正在流式输出的思考转录。
  block: ConversationBlock;
  // leaving marks the popup fade-out phase after the hold delay.
  // leaving 标记保留等待后的弹窗淡出阶段。
  leaving: boolean;
}) {
  // t resolves localized labels for the floating thinking surface.
  // t 解析浮动思考面板使用的本地化文案。
  const { t } = useI18n();
  // bodyRef stores the scrollable thinking body for tail following.
  // bodyRef 保存可滚动思考正文，用于追随最新尾部。
  const bodyRef = React.useRef<HTMLDivElement | null>(null);

  React.useLayoutEffect(() => {
    const element = bodyRef.current;
    if (!element) {
      return;
    }
    element.scrollTop = element.scrollHeight;
  }, [props.block.body]);

  return (
    <section className={cn("streaming-thinking-overlay", props.leaving && "leaving")} aria-live="polite" aria-busy={!props.leaving}>
      <header className="streaming-thinking-header">
        <span className="streaming-thinking-title">
          <BrainCircuit size={16} />
          <ShinyText text={t("runtime.thinking")} />
        </span>
        <span className="streaming-thinking-caption">{t("runtime.thinkingOverlayHint")}</span>
      </header>
      <div className="streaming-thinking-body" ref={bodyRef}>
        <MarkdownMessage content={props.block.body || t("runtime.thinking")} eager streaming={!props.leaving} />
      </div>
    </section>
  );
}

// conversationRuntimeCue resolves the compact active-state cue shown above the composer.
// conversationRuntimeCue 解析显示在输入区上方的紧凑运行状态提示。
function conversationRuntimeCue(
  conversation: ConversationSummary | undefined,
  renderItems: ConversationRenderItem[],
  t: ReturnType<typeof useI18n>["t"],
): RuntimeCue | null {
  if (!conversation) {
    return null;
  }
  if (conversation.status.toLowerCase() !== "running") {
    return null;
  }
  const normalizedPhase = normalizedSessionRunPhase(conversation.runPhase);
  const lastMeaningfulBlock = lastMeaningfulAssistantBlock(renderItems);
  const phaseCue = sessionRunPhaseCue(conversation.runPhase, t);
  if (phaseCue) {
    return phaseCue;
  }
  if (normalizedPhase && !sessionRunPhaseCanUseFallbackCue(normalizedPhase)) {
    return null;
  }
  if (!lastMeaningfulBlock) {
    return { kind: "default", text: t("runtime.memoryRecall") };
  }
  if (blockLooksLikeMemoryRecall(lastMeaningfulBlock)) {
    return { kind: "default", text: t("runtime.memoryRecall") };
  }
  return null;
}

// conversationIsRunning derives the send-button stop state from backend session status.
// conversationIsRunning 从后端会话状态推导发送按钮的停止状态。
function conversationIsRunning(conversation: ConversationSummary | undefined): boolean {
  if (!conversation) {
    return false;
  }
  const phase = normalizedSessionRunPhase(conversation.runPhase);
  if (phase && ["idle", "ready", "failed", "stopped"].includes(phase)) {
    return false;
  }
  if (phase) {
    return true;
  }
  return conversation.status.trim().toLowerCase() === "running";
}

// sessionRunPhaseCue maps the backend per-session phase to a reusable active cue.
// sessionRunPhaseCue 将后端会话级阶段映射为可复用的活动提示。
function sessionRunPhaseCue(
  runPhase: string | null | undefined,
  t: ReturnType<typeof useI18n>["t"],
): RuntimeCue | null {
  const normalized = normalizedSessionRunPhase(runPhase);
  if (!normalized || normalized === "idle" || normalized === "ready" || normalized === "failed" || normalized === "stopped") {
    return null;
  }
  if (normalized === "memoryrecall") {
    return { kind: "default", text: t("runtime.memoryRecall") };
  }
  if (normalized === "thinking") {
    return null;
  }
  if (normalized === "toolrunning") {
    return null;
  }
  return { kind: "default", text: t("runtime.requesting") };
}

// sessionRunPhaseCanUseFallbackCue decides whether legacy running sessions may infer a main-window cue.
// sessionRunPhaseCanUseFallbackCue 判断旧式 running 会话是否允许推断主窗口运行提示。
function sessionRunPhaseCanUseFallbackCue(normalizedPhase: string): boolean {
  return normalizedPhase === "running";
}

// buildStatusbarRuntimeActivities derives footer-only tool and subagent activities for the selected session.
// buildStatusbarRuntimeActivities 为当前选中会话推导仅显示在底部栏的工具与子代理活动。
function buildStatusbarRuntimeActivities(
  conversation: ConversationSummary | undefined,
  projectedItems: ConversationRenderItem[],
  subagentNavigation: WorkbenchSnapshot["subagentNavigation"] | null | undefined,
  t: ReturnType<typeof useI18n>["t"],
): StatusbarRuntimeActivity[] {
  if (!conversation) {
    return [];
  }
  const normalized = normalizedSessionRunPhase(conversation.runPhase);
  const conversationRunning = conversationIsRunning(conversation);
  const activeToolItems = projectedItems.filter((item) => item.blocks.some(isMergeableToolBlock)
    && toolExecutionDisplayState(item.blocks, conversationRunning) === "running");
  const hasRunningSubagents = subagentNavigation?.tasks?.some((task) => task.isRunning) ?? false;
  if (!conversationRunning && !hasRunningSubagents && activeToolItems.length === 0) {
    return [];
  }
  const lastMeaningfulBlock = [...projectedItems]
    .reverse()
    .flatMap((item) => [...item.blocks].reverse())
    .find((block) => block.kind !== "turn-stats") ?? null;
  const hasActiveSubagentTool = activeToolItems.some((item) => item.blocks.some(blockLooksLikeSubagentTool));
  const hasActiveNormalTool = activeToolItems.some((item) => !item.blocks.some(blockLooksLikeSubagentTool));
  const phaseLooksLikeToolRunning = normalized === "toolrunning";
  const phaseLooksLikeSubagentTool = Boolean(lastMeaningfulBlock && blockLooksLikeSubagentTool(lastMeaningfulBlock));
  const activities: StatusbarRuntimeActivity[] = [];
  if (hasActiveNormalTool || (phaseLooksLikeToolRunning && !hasRunningSubagents && !hasActiveSubagentTool && !phaseLooksLikeSubagentTool)) {
    activities.push({ kind: "tool", text: t("runtime.toolActivity") });
  }
  if (hasRunningSubagents || hasActiveSubagentTool || (phaseLooksLikeToolRunning && phaseLooksLikeSubagentTool)) {
    activities.push({ kind: "subagent", text: t("runtime.subagentRunning") });
  }
  return activities;
}

// lastMeaningfulAssistantBlock returns the newest non-stat assistant/system block for state inference.
// lastMeaningfulAssistantBlock 返回用于状态推断的最新非统计助手或系统块。
function lastMeaningfulAssistantBlock(renderItems: ConversationRenderItem[]): ConversationBlock | null {
  const blocks = renderItems
    .filter((item) => item.role !== "user")
    .flatMap((item) => item.blocks);
  return [...blocks].reverse().find((block) => block.kind !== "turn-stats") ?? null;
}

// buildConversationStreamSignature returns a compact content signature for tail-follow decisions.
// buildConversationStreamSignature 返回用于尾部追随判定的紧凑内容签名。
function buildConversationStreamSignature(messages: ConversationMessage[]): string {
  return messages
    .slice(-STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT)
    .map((message) => {
      const bodyTail = message.body.slice(-48);
      return `${message.id}:${message.recordKind ?? ""}:${message.body.length}:${bodyTail}`;
    })
    .join("|");
}

// conversationViewportAllowsTailFollow returns whether the timeline is at or near the bottom.
// conversationViewportAllowsTailFollow 返回时间线是否位于底部或接近底部。
function conversationViewportAllowsTailFollow(element: HTMLElement): boolean {
  const distanceFromBottom = conversationDistanceFromBottom(element);
  return element.scrollHeight <= element.clientHeight + 24 || distanceFromBottom <= 24;
}

// normalizedSessionRunPhase normalizes backend enum text and future route-style phase keys.
// normalizedSessionRunPhase 规范化后端枚举文本以及未来路由式阶段键。
function normalizedSessionRunPhase(runPhase?: string | null): string | null {
  const normalized = runPhase?.trim().toLowerCase().replace(/[-_]/g, "");
  return normalized && normalized.length > 0 ? normalized : null;
}

// blockLooksLikeMemoryRecall detects VMM and memory-oriented runtime blocks.
// blockLooksLikeMemoryRecall 检测 VMM 与记忆相关的运行时块。
function blockLooksLikeMemoryRecall(block: ConversationBlock): boolean {
  const source = `${block.title}\n${block.summary}\n${block.toolName ?? ""}\n${block.body}`.toLowerCase();
  return source.includes("vmm")
    || source.includes("memory")
    || source.includes("记忆")
    || source.includes("画像")
    || source.includes("recall");
}

// blockLooksLikeSubagentTool detects session child-agent coordination tools.
// blockLooksLikeSubagentTool 检测会话子代理协调工具。
function blockLooksLikeSubagentTool(block: ConversationBlock): boolean {
  const toolName = block.toolName?.trim().toLowerCase();
  return toolName === "spawn_agent"
    || toolName === "send_message"
    || toolName === "followup_task"
    || toolName === "wait_agent"
    || toolName === "list_agents"
    || toolName === "close_agent";
}

// ComposerDock renders composer-related floating controls.
// ComposerDock 渲染输入区相关的浮动控件。
function ComposerDock(props: {
  // AI capability route summaries used by the composer capability tree.
  // 输入区能力树使用的 AI 能力路由摘要。
  aiCapabilities: AiCapabilitySummary[];
  // Attachment capability gates used by composer attachment controls.
  // 输入区附件控件使用的附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[];
  // Owning project id used for attachment staging.
  // 附件暂存使用的所属项目标识。
  projectId: string | null;
  // Owning conversation id used for attachment staging.
  // 附件暂存使用的所属会话标识。
  conversationId: string | null;
  // Context usage snapshot shown by the composer orb.
  // 输入区圆盘展示的上下文占用快照。
  contextUsage: ContextUsageSnapshot | null;
  // Whether the dock is visually centered for an empty conversation.
  // 输入区浮层是否在空会话中居中显示。
  emptyMode: boolean;
  // Slash commands projected by the shared workbench service.
  // 共享工作台服务投影出的 slash 指令。
  slashCommands: ComposerSlashCommandSummary[];
  // Visible pending prompts retained for the selected conversation.
  // 当前选中会话保留的可见待发送提示词。
  pendingPrompts: PendingPromptSummary[];
  // Whether the selected conversation has an active backend run.
  // 当前选中会话是否存在活跃后端运行。
  isConversationRunning: boolean;
  // Whether streaming thinking should use the floating thinking surface.
  // 流式思考是否使用浮动思考面板。
  thinkingOverlayEnabled: boolean;
  // showScrollToBottom controls whether the bottom-jump button is visible.
  // showScrollToBottom 控制是否显示跳转到底部按钮。
  showScrollToBottom: boolean;
  // onScrollToBottom moves the current conversation to its latest content.
  // onScrollToBottom 将当前会话移动到最新内容。
  onScrollToBottom: () => void;
  // onSubmitPrompt sends one composer prompt through the workbench runtime.
  // onSubmitPrompt 通过工作台运行时发送一条输入区提示词。
  providers: ProviderSummary[];
  // onSelectWorkbenchModel persists a selected workbench model.
  // onSelectWorkbenchModel 持久化选中的工作台模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // onSelectWorkbenchReasoningEffort persists a selected reasoning effort.
  // onSelectWorkbenchReasoningEffort 持久化选中的思考档位。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // onSlashCommand routes one projected slash command.
  // onSlashCommand 路由单个投影 slash 指令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Reports the measured floating composer stack height.
  // 上报浮动输入区堆栈的实测高度。
  onComposerDockHeightChange: (height: number) => void;
  // onSubmitPrompt sends one composer prompt through the workbench runtime.
  // onSubmitPrompt 通过工作台运行时发送一条输入区提示词。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // onStopConversationRun stops the selected backend conversation stream.
  // onStopConversationRun 停止当前选中的后端会话流。
  onStopConversationRun: () => Promise<boolean>;
  // Restores one pending prompt into the composer.
  // 将单条待发送提示词恢复到输入区。
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  // Routes one pending prompt into the shared insertion queue.
  // 将单条待发送提示词路由进共享引导队列。
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  // Removes one pending prompt from the visible queue.
  // 从可见队列移除单条待发送提示词。
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  // Persists the pending prompt order.
  // 持久化待发送提示词顺序。
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
  // Updates the WebUI-only thinking display preference.
  // 更新仅 WebUI 使用的思考展示偏好。
  onThinkingOverlayEnabledChange: (enabled: boolean) => void;
  // Backend-resolved reasoning selector view for the active provider and model.
  // 后端根据当前激活 Provider 与模型解析出的思考选择器视图。
  reasoningSelection: ReasoningSelectionSummary | null;
  // runtimeCue shows the current session in-progress activity near the composer.
  // runtimeCue 在输入区附近显示当前会话的进行中活动。
  runtimeCue: RuntimeCue | null;
}) {
  // dockRef measures the full composer stack, including pending queue rows and runtime cues.
  // dockRef 测量完整输入区堆栈，包括待发送队列行与运行提示。
  const dockRef = React.useRef<HTMLDivElement | null>(null);
  const onComposerDockHeightChange = props.onComposerDockHeightChange;

  React.useLayoutEffect(() => {
    const element = dockRef.current;
    if (!element) {
      return;
    }
    const reportHeight = () => onComposerDockHeightChange(Math.ceil(element.getBoundingClientRect().height));
    reportHeight();
    if (typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(reportHeight);
    observer.observe(element);
    return () => observer.disconnect();
  }, [onComposerDockHeightChange]);

  return (
    <div className={cn("composer-dock", props.emptyMode && "empty-mode")} ref={dockRef}>
      {props.showScrollToBottom ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              aria-label="滚动到底部"
              className="scroll-to-bottom-button"
              onClick={props.onScrollToBottom}
              size="icon"
              type="button"
              variant="outline"
            >
              <ArrowDown size={18} />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">滚动到底部</TooltipContent>
        </Tooltip>
      ) : null}
      {props.runtimeCue ? (
        <div className="composer-runtime-cue" aria-live="polite" aria-busy="true">
          <ShinyText text={props.runtimeCue.text} />
        </div>
      ) : null}
      <ComposerPanel
        aiCapabilities={props.aiCapabilities}
        attachmentCapabilities={props.attachmentCapabilities}
        projectId={props.projectId}
        conversationId={props.conversationId}
        contextUsage={props.contextUsage}
        slashCommands={props.slashCommands}
        pendingPrompts={props.pendingPrompts}
        isConversationRunning={props.isConversationRunning}
        thinkingOverlayEnabled={props.thinkingOverlayEnabled}
        providers={props.providers}
        onSelectWorkbenchModel={props.onSelectWorkbenchModel}
        onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
        onSlashCommand={props.onSlashCommand}
        onSubmitPrompt={props.onSubmitPrompt}
        onStopConversationRun={props.onStopConversationRun}
        onEditPendingPrompt={props.onEditPendingPrompt}
        onInsertPendingPrompt={props.onInsertPendingPrompt}
        onRemovePendingPrompt={props.onRemovePendingPrompt}
        onReorderPendingPrompts={props.onReorderPendingPrompts}
        onThinkingOverlayEnabledChange={props.onThinkingOverlayEnabledChange}
        reasoningSelection={props.reasoningSelection}
      />
    </div>
  );
}

// ComposerPanel renders the first-version message input surface.
// ComposerPanel 渲染第一版消息输入区域。
function ComposerPanel(props: {
  // AI capability route summaries used by the quick capability tree.
  // 快捷能力树使用的 AI 能力路由摘要。
  aiCapabilities: AiCapabilitySummary[];
  // Attachment capability gates used by add/drop/paste.
  // 添加、拖拽与粘贴使用的附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[];
  // Owning project id used for attachment staging.
  // 附件暂存使用的所属项目标识。
  projectId: string | null;
  // Owning conversation id used for attachment staging.
  // 附件暂存使用的所属会话标识。
  conversationId: string | null;
  // Context usage snapshot shown by the composer orb.
  // 输入区圆盘展示的上下文占用快照。
  contextUsage: ContextUsageSnapshot | null;
  // Slash commands projected by the shared workbench service.
  // 共享工作台服务投影出的 slash 指令。
  slashCommands: ComposerSlashCommandSummary[];
  // Visible pending prompts retained for the selected conversation.
  // 当前选中会话保留的可见待发送提示词。
  pendingPrompts: PendingPromptSummary[];
  // Whether the selected conversation has an active backend run.
  // 当前选中会话是否存在活跃后端运行。
  isConversationRunning: boolean;
  // Whether streaming thinking should use the floating thinking surface.
  // 流式思考是否使用浮动思考面板。
  thinkingOverlayEnabled: boolean;
  // Providers exposed by the shared service API for model picking.
  // 共享 service API 暴露给模型选择器的 Provider 列表。
  providers: ProviderSummary[];
  // Persists one selected workbench model.
  // 持久化单个选中的工作台模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Persists one selected workbench reasoning effort.
  // 持久化单个选中的工作台思考档位。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Routes one projected slash command through the workbench shell.
  // 通过工作台外壳路由单个投影 slash 指令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Sends one validated composer submission through the caller runtime.
  // 通过调用方运行时发送一条已校验输入区提交。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Stops the selected backend conversation stream.
  // 停止当前选中的后端会话流。
  onStopConversationRun: () => Promise<boolean>;
  // Restores one pending prompt into the composer.
  // 将单条待发送提示词恢复到输入区。
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  // Routes one pending prompt into the shared insertion queue.
  // 将单条待发送提示词路由进共享引导队列。
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  // Removes one pending prompt from the visible queue.
  // 从可见队列移除单条待发送提示词。
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  // Persists the pending prompt order.
  // 持久化待发送提示词顺序。
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
  // Updates the WebUI-only thinking display preference.
  // 更新仅 WebUI 使用的思考展示偏好。
  onThinkingOverlayEnabledChange: (enabled: boolean) => void;
  // Backend-resolved reasoning selector view for the active provider and model.
  // 后端根据当前激活 Provider 与模型解析出的思考选择器视图。
  reasoningSelection: ReasoningSelectionSummary | null;
}) {
  // t translates stable composer labels while model names remain provider data.
  // t 翻译稳定输入区标签，模型名称仍保留为 provider 数据。
  const { locale, t } = useI18n();
  // draftInput stores the current unsent composer text.
  // draftInput 保存当前尚未发送的输入区文本。
  const [draftInput, setDraftInput] = React.useState("");
  // slashHighlightedIndex stores the highlighted command row while the slash panel is visible.
  // slashHighlightedIndex 保存 slash 面板可见时的高亮指令行。
  const [slashHighlightedIndex, setSlashHighlightedIndex] = React.useState(0);
  // slashDismissedInput stores the exact input dismissed by Escape.
  // slashDismissedInput 保存被 Escape 隐藏的精确输入内容。
  const [slashDismissedInput, setSlashDismissedInput] = React.useState<string | null>(null);
  // modelPickerOpenSignal opens the existing model picker from /model.
  // modelPickerOpenSignal 通过 /model 打开现有模型筛选器。
  const [modelPickerOpenSignal, setModelPickerOpenSignal] = React.useState(0);
  // reasoningPickerOpenSignal opens the existing reasoning selector from /reasoning.
  // reasoningPickerOpenSignal 通过 /reasoning 打开现有思考档位选择器。
  const [reasoningPickerOpenSignal, setReasoningPickerOpenSignal] = React.useState(0);
  // attachments stores staged composer attachments submitted with the next prompt.
  // attachments 保存下一条提示词将携带的已暂存输入区附件。
  const [attachments, setAttachments] = React.useState<ComposerAttachmentSummary[]>([]);
  // isAttachmentDragOver highlights the composer when valid files are dragged over it.
  // isAttachmentDragOver 在有效文件拖入输入区时高亮边框。
  const [isAttachmentDragOver, setIsAttachmentDragOver] = React.useState(false);
  // attachmentPickerKind stores the current hidden file input filter.
  // attachmentPickerKind 保存当前隐藏文件输入框过滤类型。
  const [attachmentPickerKind, setAttachmentPickerKind] = React.useState<string | null>(null);
  // composerMediaPlayer stores an attachment preview opened from the composer strip.
  // composerMediaPlayer 保存从输入区附件条打开的附件预览。
  const [composerMediaPlayer, setComposerMediaPlayer] = React.useState<GeneratedMediaAttachment | null>(null);
  // attachmentFileInputRef opens the browser file picker for manual attachment selection.
  // attachmentFileInputRef 打开浏览器文件选择器用于手动选择附件。
  const attachmentFileInputRef = React.useRef<HTMLInputElement | null>(null);
  // composerTextareaRef focuses the prompt field from global workspace typing.
  // composerTextareaRef 用于从工作台全局输入聚焦提示词输入框。
  const composerTextareaRef = React.useRef<HTMLTextAreaElement | null>(null);
  // executionMode stores the local preview execution-mode selection.
  // executionMode 保存本地预览用的执行模式选择。
  const [executionMode, setExecutionMode] = React.useState("build");
  // modelChoice stores the selected provider/model key shown by the shared model picker.
  // modelChoice 保存共享模型筛选器展示的 provider/model 选择键。
  const [modelChoice, setModelChoice] = React.useState("");
  // disabledAiCapabilityKinds stores current-message quick-toggle disables.
  // disabledAiCapabilityKinds 保存当前消息级快捷开关禁用项。
  const [disabledAiCapabilityKinds, setDisabledAiCapabilityKinds] = React.useState<Set<AiCapabilityKind>>(() => new Set());
  // disabledAiCapabilityTargetSelectors stores current-message target-route disables.
  // disabledAiCapabilityTargetSelectors 保存当前消息级目标路由禁用项。
  const [disabledAiCapabilityTargetSelectors, setDisabledAiCapabilityTargetSelectors] = React.useState<Set<string>>(() => new Set());
  // aiCapabilityTargetOrder stores current-message target route ordering.
  // aiCapabilityTargetOrder 保存当前消息级目标路由顺序。
  const [aiCapabilityTargetOrder, setAiCapabilityTargetOrder] = React.useState<Map<AiCapabilityKind, string[]>>(() => new Map());
  // reasoningEffort stores the backend-synchronized reasoning-effort selection.
  // reasoningEffort 保存与后端同步的思考档位选择。
  const [reasoningEffort, setReasoningEffort] = React.useState("x-high");
  // isComposerRunning stores the local send/stop preview state.
  // isComposerRunning 保存输入区发送/停止的本地预览状态。
  const [isComposerRunning, setIsComposerRunning] = React.useState(false);
  // availableModelChoices flattens provider data into selectable workbench models.
  // availableModelChoices 将 Provider 数据扁平化为可选择的工作台模型。
  const availableModelChoices = React.useMemo(
    () => modelPickerChoicesFromProviders(props.providers),
    [props.providers],
  );
  // activeModelChoiceKey follows the service-reported active model when available.
  // activeModelChoiceKey 在可用时跟随 service 上报的激活模型。
  const activeModelChoiceKey = React.useMemo(() => {
    const activeChoice = availableModelChoices.find((choice) => choice.active) ?? availableModelChoices[0];
    return modelPickerChoiceKey(activeChoice);
  }, [availableModelChoices]);
  // capabilitySignature prevents unrelated workbench streaming snapshots from resetting prompt overrides.
  // capabilitySignature 防止无关的工作台流式快照重置提示词级能力覆盖。
  const capabilitySignature = React.useMemo(
    () => aiCapabilitySignature(props.aiCapabilities),
    [props.aiCapabilities],
  );
  // enabledAttachmentCapabilities resolves the kinds currently accepted by runtime capability gates.
  // enabledAttachmentCapabilities 解析当前运行时能力门控接受的附件类型。
  const enabledAttachmentCapabilities = React.useMemo(
    () => props.attachmentCapabilities.filter((capability) => capability.enabled),
    [props.attachmentCapabilities],
  );
  // enabledAttachmentKinds speeds up local drop and paste validation.
  // enabledAttachmentKinds 加速本地拖拽与粘贴校验。
  const enabledAttachmentKinds = React.useMemo(
    () => new Set(enabledAttachmentCapabilities.map((capability) => capability.kind)),
    [enabledAttachmentCapabilities],
  );

  React.useEffect(() => {
    if (!activeModelChoiceKey) {
      return;
    }
    const currentStillAvailable = availableModelChoices.some(
      (choice) => modelPickerChoiceKey(choice) === modelChoice,
    );
    if (!modelChoice || !currentStillAvailable) {
      setModelChoice(activeModelChoiceKey);
    }
  }, [activeModelChoiceKey, availableModelChoices, modelChoice]);

  React.useEffect(() => {
    setDisabledAiCapabilityKinds(defaultDisabledAiCapabilityKinds(props.aiCapabilities));
    setDisabledAiCapabilityTargetSelectors(defaultDisabledAiCapabilityTargetSelectors(props.aiCapabilities));
    setAiCapabilityTargetOrder(defaultAiCapabilityTargetOrder(props.aiCapabilities));
  }, [capabilitySignature]);
  // executionModeOptions mirrors iced ExecutionMode::ALL using current language labels.
  // executionModeOptions 使用当前语言标签对齐 iced 的 ExecutionMode::ALL。
  const executionModeOptions = React.useMemo<ComposerSelectOption[]>(() => [
    { value: "build", label: t("composer.mode.execute") },
    { value: "plan", label: t("composer.mode.plan") },
  ], [t]);
  // fallbackReasoningEffortOptions is only used when no backend selector view is available.
  // fallbackReasoningEffortOptions 仅在没有后端选择器视图时使用。
  const fallbackReasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => [
    { value: "off", label: t("composer.reasoning.off") },
    { value: "low", label: t("composer.reasoning.low") },
    { value: "medium", label: t("composer.reasoning.medium") },
    { value: "high", label: t("composer.reasoning.high") },
    { value: "x-high", label: t("composer.reasoning.max") },
  ], [t]);
  // reasoningEffortOptions uses the shared backend selector view before falling back to local preview labels.
  // reasoningEffortOptions 优先使用共享后端选择器视图，再回退到本地预览标签。
  const reasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => {
    if (props.reasoningSelection?.options.length) {
      return props.reasoningSelection.options.map((option) => ({
        value: option.value,
        label: option.label,
      }));
    }
    return fallbackReasoningEffortOptions;
  }, [fallbackReasoningEffortOptions, props.reasoningSelection, t]);
  // serviceSelectedReasoningEffort mirrors the active backend-selected effort when available.
  // serviceSelectedReasoningEffort 在可用时镜像后端选中的生效思考档位。
  const serviceSelectedReasoningEffort = props.reasoningSelection?.selectedEffort;

  React.useEffect(() => {
    if (serviceSelectedReasoningEffort && serviceSelectedReasoningEffort !== reasoningEffort) {
      setReasoningEffort(serviceSelectedReasoningEffort);
      return;
    }
    if (!reasoningEffortOptions.some((option) => option.value === reasoningEffort)) {
      setReasoningEffort(reasoningEffortOptions[0]?.value ?? "medium");
    }
  }, [reasoningEffort, reasoningEffortOptions, serviceSelectedReasoningEffort]);
  // placeholder describes the platform-specific send and newline keyboard behavior.
  // placeholder 描述平台相关的发送与换行键盘行为。
  const placeholder = React.useMemo(() => {
    return t("composer.placeholder").replace("{shortcut}", composerShortcutHint(t("composer.shortcutHint")));
  }, [t]);
  // visibleSlashCommands mirrors iced prefix filtering for the slash command panel.
  // visibleSlashCommands 对齐 iced 前缀过滤逻辑，供 slash 指令面板展示。
  const visibleSlashCommands = React.useMemo(() => {
    if (!draftInput.startsWith("/") || slashDismissedInput === draftInput) {
      return [];
    }
    return props.slashCommands.filter((command) => composerSlashCommandMatchesInput(command, draftInput));
  }, [draftInput, props.slashCommands, slashDismissedInput]);
  // slashPanelVisible controls whether the current slash input has at least one match.
  // slashPanelVisible 控制当前 slash 输入是否存在至少一个匹配项。
  const slashPanelVisible = visibleSlashCommands.length > 0;
  // highlightedSlashCommand resolves the currently highlighted command row.
  // highlightedSlashCommand 解析当前高亮的指令行。
  const highlightedSlashCommand = visibleSlashCommands[Math.min(slashHighlightedIndex, Math.max(visibleSlashCommands.length - 1, 0))] ?? null;
  // exactSlashCommand mirrors iced exact trigger activation before the highlighted row.
  // exactSlashCommand 对齐 iced 精确触发优先于高亮项的激活规则。
  const exactSlashCommand = React.useMemo(
    () => exactComposerSlashCommand(visibleSlashCommands, draftInput),
    [draftInput, visibleSlashCommands],
  );

  React.useEffect(() => {
    setSlashHighlightedIndex((current) => {
      if (!visibleSlashCommands.length) {
        return 0;
      }
      return Math.min(current, visibleSlashCommands.length - 1);
    });
  }, [visibleSlashCommands.length]);

  // handleDraftInputChange tracks typed content and re-enables slash matching after edits.
  // handleDraftInputChange 跟踪输入内容，并在编辑后重新启用 slash 匹配。
  const handleDraftInputChange = React.useCallback((value: string) => {
    setDraftInput(value);
    if (slashDismissedInput !== null && value !== slashDismissedInput) {
      setSlashDismissedInput(null);
    }
  }, [slashDismissedInput]);

  React.useEffect(() => {
    // handleGlobalWorkspaceTyping redirects ordinary workspace typing into the composer.
    // handleGlobalWorkspaceTyping 将工作台中的普通键入重定向到输入区。
    const handleGlobalWorkspaceTyping = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !isComposerPrintableKey(event)) {
        return;
      }
      const textarea = composerTextareaRef.current;
      if (!textarea || document.activeElement === textarea) {
        return;
      }
      if (hasBlockingComposerPopup() || isComposerTextEntryElement(document.activeElement)) {
        return;
      }
      event.preventDefault();
      textarea.focus({ preventScroll: true });
      setDraftInput((current) => `${current}${event.key}`);
      if (slashDismissedInput !== null) {
        setSlashDismissedInput(null);
      }
    };
    document.addEventListener("keydown", handleGlobalWorkspaceTyping);
    return () => document.removeEventListener("keydown", handleGlobalWorkspaceTyping);
  }, [slashDismissedInput]);

  // activateSlashCommand dispatches one selected command and applies its UI route result.
  // activateSlashCommand 分派单个选中的指令并应用其 UI 路由结果。
  const activateSlashCommand = React.useCallback((command: ComposerSlashCommandSummary) => {
    const result = props.onSlashCommand(command);
    if (!result.handled) {
      return;
    }
    setDraftInput("");
    setSlashDismissedInput(null);
    setSlashHighlightedIndex(0);
    if (result.replacementText !== undefined) {
      setDraftInput(result.replacementText);
    }
    if (result.openTarget === "model") {
      setModelPickerOpenSignal((current) => current + 1);
    }
    if (result.openTarget === "reasoning") {
      setReasoningPickerOpenSignal((current) => current + 1);
    }
  }, [props]);

  // handleComposerSend validates input and forwards it to the real backend runtime.
  // handleComposerSend 校验输入并转发到真实后端运行时。
  const handleComposerSend = React.useCallback(async () => {
    const prompt = draftInput.trim();
    const hasSubmittableContent = Boolean(prompt || attachments.length > 0);
    webuiClickTrace("composer.send_or_stop", {
      isConversationRunning: props.isConversationRunning,
      hasSubmittableContent,
      promptChars: prompt.length,
      attachmentCount: attachments.length,
    });
    if (props.isConversationRunning && !hasSubmittableContent) {
      const stopped = await props.onStopConversationRun();
      if (!stopped) {
        toast.error("停止当前会话失败。");
      }
      return;
    }
    if (isComposerRunning) {
      return;
    }
    if (!hasSubmittableContent) {
      toast.info("请输入任务指令。");
      return;
    }
    // submittedInput preserves the exact textarea content for a failure-only restore.
    // submittedInput 保存精确输入内容，仅在发送失败时用于恢复。
    const submittedInput = draftInput;
    // submittedAttachments preserves staged files for a failure-only restore.
    // submittedAttachments 保存已暂存文件，仅在发送失败时用于恢复。
    const submittedAttachments = attachments;
    setDraftInput("");
    setAttachments([]);
    setIsComposerRunning(true);
    try {
      const sent = await props.onSubmitPrompt({
        prompt,
        attachments: submittedAttachments,
        executionMode,
        reasoningEffort,
        disabledAiCapabilityKinds: Array.from(disabledAiCapabilityKinds),
        disabledAiCapabilityTargetSelectors: Array.from(disabledAiCapabilityTargetSelectors),
        aiCapabilityTargetOrderOverrides: serializeAiCapabilityTargetOrder(aiCapabilityTargetOrder),
      });
      if (!sent) {
        setDraftInput((current) => current || submittedInput);
        setAttachments((current) => current.length ? current : submittedAttachments);
      }
    } finally {
      setIsComposerRunning(false);
    }
  }, [aiCapabilityTargetOrder, attachments, disabledAiCapabilityKinds, disabledAiCapabilityTargetSelectors, draftInput, executionMode, isComposerRunning, props, reasoningEffort]);

  // handleComposerStop stops the active backend conversation without consuming draft text.
  // handleComposerStop 在不消费草稿文本的前提下停止活跃后端会话。
  const handleComposerStop = React.useCallback(async () => {
    webuiClickTrace("composer.stop", {
      isConversationRunning: props.isConversationRunning,
      draftChars: draftInput.trim().length,
      attachmentCount: attachments.length,
    });
    const stopped = await props.onStopConversationRun();
    if (!stopped) {
      toast.error("停止当前会话失败。");
    }
  }, [attachments.length, draftInput, props]);

  // handlePendingPromptEdit restores one queued prompt into the composer draft.
  // handlePendingPromptEdit 将单条排队提示词恢复到输入区草稿。
  const handlePendingPromptEdit = React.useCallback(async (entryId: string) => {
    webuiClickTrace("pending_prompt.edit", { entryId });
    const entry = await props.onEditPendingPrompt(entryId);
    if (!entry) {
      return;
    }
    setDraftInput(entry.prompt);
    setAttachments(entry.attachments ?? []);
    window.requestAnimationFrame(() => composerTextareaRef.current?.focus({ preventScroll: true }));
  }, [props]);

  // stageComposerFiles validates and stages files selected, dropped, or pasted into the composer.
  // stageComposerFiles 校验并暂存通过选择、拖拽或粘贴进入输入区的文件。
  const stageComposerFiles = React.useCallback(async (files: File[], preferredKind?: string | null) => {
    const candidates = files.filter((file) => {
      const kind = inferComposerAttachmentKindFromFile(file);
      return Boolean(kind && (!preferredKind || kind === preferredKind) && enabledAttachmentKinds.has(kind));
    });
    if (!candidates.length) {
      toast.info("当前模型或扩展能力不支持这些附件。");
      return;
    }
    const availableSlots = MAX_COMPOSER_ATTACHMENTS - attachments.length;
    if (availableSlots <= 0) {
      toast.info(`最多只能添加 ${MAX_COMPOSER_ATTACHMENTS} 个附件。`);
      return;
    }
    const acceptedFiles = candidates.slice(0, availableSlots);
    if (candidates.length > acceptedFiles.length) {
      toast.info(`已达到 ${MAX_COMPOSER_ATTACHMENTS} 个附件上限，部分文件未添加。`);
    }
    const staged: ComposerAttachmentSummary[] = [];
    for (const file of acceptedFiles) {
      try {
        const attachment = await stageComposerAttachmentBytes(props.projectId, props.conversationId, file);
        if (attachment) {
          staged.push(attachment);
        }
      } catch (error) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
    }
    if (!staged.length) {
      return;
    }
    setAttachments((current) => {
      const seen = new Set(current.map((attachment) => `${attachment.localPath}\n${attachment.fileName}`));
      const unique = staged.filter((attachment) => {
        const key = `${attachment.localPath}\n${attachment.fileName}`;
        if (seen.has(key)) {
          return false;
        }
        seen.add(key);
        return true;
      });
      return [...current, ...unique].slice(0, MAX_COMPOSER_ATTACHMENTS);
    });
  }, [attachments.length, enabledAttachmentKinds, props.conversationId, props.projectId]);

  // openAttachmentPicker opens the browser file picker with a kind-specific accept filter.
  // openAttachmentPicker 使用类型特定 accept 过滤器打开浏览器文件选择器。
  const openAttachmentPicker = React.useCallback((kind: string | null) => {
    setAttachmentPickerKind(kind);
    window.requestAnimationFrame(() => attachmentFileInputRef.current?.click());
  }, []);

  // handleAttachmentInputChange stages files selected through the hidden input.
  // handleAttachmentInputChange 暂存通过隐藏输入框选择的文件。
  const handleAttachmentInputChange = React.useCallback((event: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = "";
    if (files.length) {
      void stageComposerFiles(files, attachmentPickerKind);
    }
  }, [attachmentPickerKind, stageComposerFiles]);

  // handleComposerPaste gives file and image clipboard payloads priority over plain text.
  // handleComposerPaste 让文件与图片剪贴板载荷优先于普通文本处理。
  const handleComposerPaste = React.useCallback((event: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const fileList = Array.from(event.clipboardData.files ?? []);
    const itemFiles = Array.from(event.clipboardData.items ?? [])
      .map((item) => item.kind === "file" ? item.getAsFile() : null)
      .filter((file): file is File => Boolean(file));
    const files = fileList.length ? fileList : itemFiles;
    if (!files.length) {
      return;
    }
    const hasSupportedFile = files.some((file) => {
      const kind = inferComposerAttachmentKindFromFile(file);
      return Boolean(kind && enabledAttachmentKinds.has(kind));
    });
    if (!hasSupportedFile) {
      return;
    }
    event.preventDefault();
    void stageComposerFiles(files);
  }, [enabledAttachmentKinds, stageComposerFiles]);

  // handleComposerDragOver highlights the composer only when dragged files are acceptable.
  // handleComposerDragOver 仅在拖入文件可接受时高亮输入区。
  const handleComposerDragOver = React.useCallback((event: React.DragEvent<HTMLElement>) => {
    const hasFiles = Array.from(event.dataTransfer.types).includes("Files");
    if (!hasFiles || enabledAttachmentCapabilities.length === 0) {
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    setIsAttachmentDragOver(true);
  }, [enabledAttachmentCapabilities.length]);

  // handleComposerDrop stages dropped files and clears drag highlighting.
  // handleComposerDrop 暂存拖入文件并清理拖拽高亮。
  const handleComposerDrop = React.useCallback((event: React.DragEvent<HTMLElement>) => {
    const files = Array.from(event.dataTransfer.files ?? []);
    if (!files.length) {
      setIsAttachmentDragOver(false);
      return;
    }
    event.preventDefault();
    setIsAttachmentDragOver(false);
    void stageComposerFiles(files);
  }, [stageComposerFiles]);

  // removeComposerAttachment removes one staged attachment by index.
  // removeComposerAttachment 按索引移除单个已暂存附件。
  const removeComposerAttachment = React.useCallback((index: number) => {
    setAttachments((current) => current.filter((_, currentIndex) => currentIndex !== index));
  }, []);

  // handleAiCapabilityToggle updates the current-message disabled capability set.
  // handleAiCapabilityToggle 更新当前消息级禁用能力集合。
  const handleAiCapabilityToggle = React.useCallback((kind: AiCapabilityKind, enabled: boolean) => {
    setDisabledAiCapabilityKinds((current) => {
      const next = new Set(current);
      if (enabled) {
        next.delete(kind);
      } else {
        next.add(kind);
      }
      return next;
    });
  }, []);

  // handleAiCapabilityTargetToggle updates the current-message disabled target selector set.
  // handleAiCapabilityTargetToggle 更新当前消息级禁用目标选择器集合。
  const handleAiCapabilityTargetToggle = React.useCallback((targetSelector: string, enabled: boolean) => {
    setDisabledAiCapabilityTargetSelectors((current) => {
      const next = new Set(current);
      if (enabled) {
        next.delete(targetSelector);
      } else {
        next.add(targetSelector);
      }
      return next;
    });
  }, []);

  // handleAiCapabilityTargetReorder replaces the prompt-level route order for one capability.
  // handleAiCapabilityTargetReorder 替换单个能力的提示词级路由顺序。
  const handleAiCapabilityTargetReorder = React.useCallback((kind: AiCapabilityKind, targetSelectors: string[]) => {
    setAiCapabilityTargetOrder((current) => {
      const next = new Map(current);
      const currentOrder = next.get(kind) ?? [];
      if (currentOrder.length === targetSelectors.length && currentOrder.every((selector, index) => selector === targetSelectors[index])) {
        return current;
      }
      next.set(kind, targetSelectors);
      return next;
    });
  }, []);

  // handleModelChoiceChange applies one model choice locally and through the shared backend.
  // handleModelChoiceChange 在本地与共享后端应用单个模型选择。
  const handleModelChoiceChange = React.useCallback(async (choice: ModelPickerChoice) => {
    const previousChoice = modelChoice;
    const nextChoice = modelPickerChoiceKey(choice);
    setModelChoice(nextChoice);
    const ok = await props.onSelectWorkbenchModel(choice);
    if (!ok) {
      setModelChoice(previousChoice);
    }
  }, [modelChoice, props]);

  // handleReasoningEffortChange applies one backend-routed reasoning selection with rollback.
  // handleReasoningEffortChange 应用单个后端路由的思考档位选择并支持失败回滚。
  const handleReasoningEffortChange = React.useCallback(async (value: string) => {
    const previousEffort = reasoningEffort;
    setReasoningEffort(value);
    const ok = await props.onSelectWorkbenchReasoningEffort(value);
    if (!ok) {
      setReasoningEffort(previousEffort);
    }
  }, [props, reasoningEffort]);

  // handleComposerKeyDown maps Enter to send while keeping Shift+Enter as a newline.
  // handleComposerKeyDown 将 Enter 映射为发送，并保留 Shift+Enter 换行。
  const handleComposerKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (slashPanelVisible) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setSlashHighlightedIndex((current) => (current + 1) % visibleSlashCommands.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setSlashHighlightedIndex((current) => (current - 1 + visibleSlashCommands.length) % visibleSlashCommands.length);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setSlashDismissedInput(draftInput);
        return;
      }
      if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
        const command = exactSlashCommand ?? highlightedSlashCommand;
        if (command) {
          event.preventDefault();
          activateSlashCommand(command);
          return;
        }
      }
    }
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) {
      return;
    }
    event.preventDefault();
    void handleComposerSend();
  }, [
    activateSlashCommand,
    draftInput,
    exactSlashCommand,
    handleComposerSend,
    highlightedSlashCommand,
    slashPanelVisible,
    visibleSlashCommands.length,
  ]);

  return (
    <>
      {props.pendingPrompts.length ? (
        <PendingPromptQueueStrip
          entries={props.pendingPrompts}
          onEdit={(entryId) => void handlePendingPromptEdit(entryId)}
          onInsert={(entryId) => void props.onInsertPendingPrompt(entryId)}
          onRemove={(entryId) => void props.onRemovePendingPrompt(entryId)}
          onReorder={(entryIds, draggedEntryId, targetEntryId) => void props.onReorderPendingPrompts(entryIds, draggedEntryId, targetEntryId)}
        />
      ) : null}
      <section
        className={cn("composer-panel", isAttachmentDragOver && "attachment-drag-over")}
        onDragLeave={() => setIsAttachmentDragOver(false)}
        onDragOver={handleComposerDragOver}
        onDrop={handleComposerDrop}
      >
        <input
          accept={attachmentPickerKind
            ? enabledAttachmentCapabilities.find((capability) => capability.kind === attachmentPickerKind)?.accept
            : composerAttachmentAccept(enabledAttachmentCapabilities)}
          className="composer-attachment-input"
          multiple
          onChange={handleAttachmentInputChange}
          ref={attachmentFileInputRef}
          type="file"
        />
        {attachments.length ? (
          <ComposerAttachmentStrip
            attachments={attachments}
            onOpenMedia={(attachment) => {
              const media = composerAttachmentMedia(attachment);
              if (media) {
                setComposerMediaPlayer(media);
              }
            }}
            onRemove={removeComposerAttachment}
          />
        ) : null}
        {slashPanelVisible ? (
          <ComposerSlashCommandPanel
            commands={visibleSlashCommands}
            highlightedIndex={Math.min(slashHighlightedIndex, visibleSlashCommands.length - 1)}
            locale={locale}
            onActivate={activateSlashCommand}
            onHighlight={setSlashHighlightedIndex}
          />
        ) : null}
        <Textarea
          aria-label={t("composer.aria.input")}
          onChange={(event) => handleDraftInputChange(event.currentTarget.value)}
          onKeyDown={handleComposerKeyDown}
          onPaste={handleComposerPaste}
          placeholder={placeholder}
          ref={composerTextareaRef}
          value={draftInput}
        />
        <div className="composer-toolbar">
          <div className="composer-toolbar-left">
            <ComposerAttachmentMenu
              capabilities={enabledAttachmentCapabilities}
              thinkingOverlayEnabled={props.thinkingOverlayEnabled}
              onPick={openAttachmentPicker}
              onThinkingOverlayEnabledChange={props.onThinkingOverlayEnabledChange}
            />
            <ComposerSelectControl
              ariaLabel={t("composer.aria.mode")}
              className="mode no-indicator"
              fallbackLabel={t("composer.selectFallback")}
              onValueChange={setExecutionMode}
              options={executionModeOptions}
              value={executionMode}
            />
            <ModelPickerControl
              ariaLabel={t("composer.aria.model")}
              fallbackLabel={t("composer.selectFallback")}
              filterPlaceholder={t("composer.modelPicker.filter")}
              noMatchesLabel={t("composer.modelPicker.noMatches")}
              onValueChange={(choice) => void handleModelChoiceChange(choice)}
              openSignal={modelPickerOpenSignal}
              providers={props.providers}
              value={modelChoice}
            />
            <ComposerAiCapabilityControl
              ariaLabel={t("composer.aria.capability")}
              capabilities={props.aiCapabilities}
              disabledKinds={disabledAiCapabilityKinds}
              disabledTargetSelectors={disabledAiCapabilityTargetSelectors}
              targetOrder={aiCapabilityTargetOrder}
              locale={locale}
              onToggle={handleAiCapabilityToggle}
              onTargetReorder={handleAiCapabilityTargetReorder}
              onTargetToggle={handleAiCapabilityTargetToggle}
            />
            <ContextUsageOrb snapshot={props.contextUsage} />
          </div>
          <div className="composer-toolbar-right">
            <ComposerSelectControl
              ariaLabel={t("composer.aria.reasoning")}
              className="reasoning no-indicator"
              fallbackLabel={t("composer.selectFallback")}
              onValueChange={(value) => void handleReasoningEffortChange(value)}
              openSignal={reasoningPickerOpenSignal}
              options={reasoningEffortOptions}
              triggerSubtitle={t("composer.reasoning.subtitle")}
              value={reasoningEffort}
            />
            {props.isConversationRunning && (draftInput.trim() || attachments.length > 0) ? (
              <Button
                aria-label={t("composer.aria.stop")}
                className="send-button running"
                onClick={() => void handleComposerStop()}
                size="icon"
                type="button"
              >
                <Square fill="currentColor" size={15} strokeWidth={0} />
              </Button>
            ) : null}
            <Button
              aria-label={props.isConversationRunning && !draftInput.trim() && attachments.length === 0 ? t("composer.aria.stop") : t("composer.aria.send")}
              className={cn("send-button", props.isConversationRunning && !draftInput.trim() && attachments.length === 0 && "running")}
              onClick={() => void handleComposerSend()}
              size="icon"
              type="button"
            >
              {props.isConversationRunning && !draftInput.trim() && attachments.length === 0 ? <Square fill="currentColor" size={15} strokeWidth={0} /> : <Send size={18} />}
            </Button>
          </div>
        </div>
        {composerMediaPlayer ? (
          <GeneratedMediaPlayerWindow attachment={composerMediaPlayer} onClose={() => setComposerMediaPlayer(null)} />
        ) : null}
      </section>
    </>
  );
}

// PendingPromptQueueStrip renders the service-backed visible send queue above the composer.
// PendingPromptQueueStrip 渲染输入区上方由服务端持久化的可见发送队列。
function PendingPromptQueueStrip(props: {
  // Queue entries returned by the selected conversation snapshot.
  // 当前选中会话快照返回的队列条目。
  entries: PendingPromptSummary[];
  // Restores one entry into the composer draft.
  // 将单条队列恢复到输入区草稿。
  onEdit: (entryId: string) => void;
  // Routes one entry into the shared insertion queue.
  // 将单条队列项路由进共享引导队列。
  onInsert: (entryId: string) => void;
  // Removes one entry from the queue.
  // 从队列中移除单条条目。
  onRemove: (entryId: string) => void;
  // Persists a reordered entry id list.
  // 持久化重排后的条目标识列表。
  onReorder: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => void;
}) {
  // sensors keeps accidental clicks from turning into drag operations.
  // sensors 防止普通点击误触发拖拽。
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));
  // entryIds is the stable dnd item list derived from the backend queue.
  // entryIds 是从后端队列派生出的稳定拖拽条目列表。
  const entryIds = React.useMemo(() => props.entries.map((entry) => entry.id), [props.entries]);
  // handleDragEnd persists the iced-compatible drop target derived from the visual dnd order.
  // handleDragEnd 从可视拖拽顺序推导与 iced 兼容的落点并持久化。
  const handleDragEnd = React.useCallback((event: DragEndEvent) => {
    const activeId = String(event.active.id);
    const overId = event.over ? String(event.over.id) : null;
    if (!overId || activeId === overId) {
      return;
    }
    const oldIndex = entryIds.indexOf(activeId);
    const newIndex = entryIds.indexOf(overId);
    if (oldIndex < 0 || newIndex < 0) {
      return;
    }
    const nextEntryIds = arrayMove(entryIds, oldIndex, newIndex);
    const movedIndex = nextEntryIds.indexOf(activeId);
    const targetEntryId = movedIndex >= 0 ? nextEntryIds[movedIndex + 1] ?? null : null;
    props.onReorder(nextEntryIds, activeId, targetEntryId);
  }, [entryIds, props]);

  return (
    <div className="pending-prompt-strip" aria-label="待发送队列">
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={entryIds} strategy={verticalListSortingStrategy}>
          <div className="pending-prompt-list">
            {props.entries.map((entry) => (
              <PendingPromptQueueRow
                entry={entry}
                key={entry.id}
                onEdit={props.onEdit}
                onInsert={props.onInsert}
                onRemove={props.onRemove}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>
    </div>
  );
}

// PendingPromptQueueRow renders one sortable queued prompt row.
// PendingPromptQueueRow 渲染单条可排序的排队提示词。
function PendingPromptQueueRow(props: {
  // Queue entry rendered by this row.
  // 当前行渲染的队列条目。
  entry: PendingPromptSummary;
  // Restores this entry into the composer draft.
  // 将当前条目恢复到输入区草稿。
  onEdit: (entryId: string) => void;
  // Routes this entry into the shared insertion queue.
  // 将当前条目路由进共享引导队列。
  onInsert: (entryId: string) => void;
  // Removes this entry from the queue.
  // 从队列移除当前条目。
  onRemove: (entryId: string) => void;
}) {
  const sortable = useVerticalSortableItem(props.entry.id, props.entry.state === "inserted");
  const attachmentText = props.entry.attachments.length ? `  · ${props.entry.attachments.length} 个附件` : "";
  const previewText = `${props.entry.preview || props.entry.prompt || "附件消息"}${attachmentText}`;
  return (
    <div
      className={cn("pending-prompt-row", sortable.isDragging && "dragging", props.entry.state === "inserted" && "inserted")}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
    >
      <div
        aria-label="拖动待发送提示词"
        className="pending-prompt-row-handle"
        {...sortable.attributes}
        {...sortable.listeners}
      >
        <GripVertical size={13} />
      </div>
      <div className="pending-prompt-row-main">
        <span className="pending-prompt-preview">{previewText}</span>
      </div>
      <div
        className="pending-prompt-row-actions"
        onClick={(event) => event.stopPropagation()}
        onPointerDown={(event) => event.stopPropagation()}
      >
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              className={cn("pending-prompt-guide-button", props.entry.state === "inserted" && "inserted")}
              aria-label={props.entry.state === "inserted" ? "已引导待发送提示词" : "引导待发送提示词"}
              disabled={props.entry.state === "inserted"}
          onClick={() => {
            webuiClickTrace("pending_prompt.row.insert", {
              entryId: props.entry.id,
              state: props.entry.state,
            });
            props.onInsert(props.entry.id);
          }}
              size="xs"
              type="button"
              variant="ghost"
            >
              <CornerDownRight size={13} />
              <span>{props.entry.state === "inserted" ? "已引导" : "引导"}</span>
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">{props.entry.state === "inserted" ? "已引导" : "引导"}</TooltipContent>
        </Tooltip>
        {props.entry.state === "inserted" ? null : (
          <>
            <Button
              aria-label="删除待发送提示词"
              onClick={() => {
                webuiClickTrace("pending_prompt.row.remove", { entryId: props.entry.id });
                props.onRemove(props.entry.id);
              }}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <Trash2 size={13} />
            </Button>
            <Button
              aria-label="编辑待发送提示词"
              onClick={() => {
                webuiClickTrace("pending_prompt.row.edit", { entryId: props.entry.id });
                props.onEdit(props.entry.id);
              }}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <Pencil size={13} />
            </Button>
          </>
        )}
      </div>
    </div>
  );
}

// ComposerSlashCommandPanel renders the iced-style slash command candidate list.
// ComposerSlashCommandPanel 渲染对齐 iced 的 slash 指令候选列表。
function ComposerSlashCommandPanel(props: {
  // Visible commands after prefix filtering.
  // 前缀过滤后的可见指令。
  commands: ComposerSlashCommandSummary[];
  // Highlighted command row index.
  // 当前高亮的指令行索引。
  highlightedIndex: number;
  // Current interface language.
  // 当前界面语言。
  locale: LocaleCode;
  // Activates one command row.
  // 激活单个指令行。
  onActivate: (command: ComposerSlashCommandSummary) => void;
  // Updates highlighted command row.
  // 更新高亮指令行。
  onHighlight: (index: number) => void;
}) {
  return (
    <div className="composer-slash-panel" role="listbox" aria-label="Slash commands">
      {props.commands.map((command, index) => (
        <button
          aria-selected={index === props.highlightedIndex}
          className={cn("composer-slash-row", index === props.highlightedIndex && "active")}
          key={command.id || command.command}
          onClick={() => props.onActivate(command)}
          onMouseEnter={() => props.onHighlight(index)}
          role="option"
          type="button"
        >
          <span className="composer-slash-command">{command.command}</span>
          <span className="composer-slash-description">{composerSlashCommandDescription(command, props.locale)}</span>
        </button>
      ))}
    </div>
  );
}

// ComposerAttachmentMenu renders the plus button and attachment submenu.
// ComposerAttachmentMenu 渲染加号按钮与附件子菜单。
function ComposerAttachmentMenu(props: {
  // Enabled attachment capability gates available for this prompt.
  // 当前提示词可用的已启用附件能力门控。
  capabilities: ComposerAttachmentCapabilitySummary[];
  // Whether streaming thinking currently uses the floating thinking surface.
  // 当前流式思考是否使用浮动思考面板。
  thinkingOverlayEnabled: boolean;
  // Opens a file picker for one attachment kind or all kinds.
  // 按附件类型或全部类型打开文件选择器。
  onPick: (kind: string | null) => void;
  // Updates the thinking display preference from the plus menu.
  // 从加号菜单更新思考展示偏好。
  onThinkingOverlayEnabledChange: (enabled: boolean) => void;
}) {
  const menuCapabilities = props.capabilities.filter((capability) => ["image", "video", "audio", "pdf"].includes(capability.kind));
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          aria-label="添加附件"
          className="composer-add-button"
          size="icon"
          type="button"
          variant="secondary"
        >
          <Plus size={17} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="composer-attachment-menu" side="top" sideOffset={8}>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <PaperclipIcon />
            <span>附件</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            {menuCapabilities.length ? (
              menuCapabilities.map((capability) => (
                <DropdownMenuItem
                  key={capability.kind}
                  onSelect={() => props.onPick(capability.kind)}
                >
                  <ComposerAttachmentKindIcon kind={capability.kind} />
                  <span>{capability.label}</span>
                </DropdownMenuItem>
              ))
            ) : (
              <DropdownMenuItem disabled>
                <span>当前无可用附件类型</span>
              </DropdownMenuItem>
            )}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <BrainCircuit size={15} />
            <span>思考弹窗</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="composer-thinking-menu">
            <DropdownMenuItem onSelect={() => props.onThinkingOverlayEnabledChange(true)}>
              <span>开启</span>
              {props.thinkingOverlayEnabled ? <CheckCircle2 size={14} /> : null}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => props.onThinkingOverlayEnabledChange(false)}>
              <span>关闭</span>
              {!props.thinkingOverlayEnabled ? <CheckCircle2 size={14} /> : null}
            </DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// PaperclipIcon keeps the attachment menu decoupled from lucide naming changes.
// PaperclipIcon 让附件菜单不直接依赖 lucide 的命名变化。
function PaperclipIcon() {
  const Icon = (LucideIcons as unknown as Record<string, React.ComponentType<{ size?: number }>>).Paperclip;
  return Icon ? <Icon size={15} /> : <FileText size={15} />;
}

// ComposerAttachmentKindIcon renders a compact icon for one attachment type.
// ComposerAttachmentKindIcon 渲染单种附件类型的紧凑图标。
function ComposerAttachmentKindIcon(props: { kind: string }) {
  if (props.kind === "image") return <ImageIcon size={15} />;
  if (props.kind === "video") return <Video size={15} />;
  if (props.kind === "audio") return <Music2 size={15} />;
  return <FileText size={15} />;
}

// ComposerAttachmentStrip renders staged attachments before the prompt text.
// ComposerAttachmentStrip 在提示词文本之前渲染已暂存附件。
function ComposerAttachmentStrip(props: {
  // Staged attachments shown in the strip.
  // 附件条中展示的已暂存附件。
  attachments: ComposerAttachmentSummary[];
  // Opens an attachment preview.
  // 打开附件预览。
  onOpenMedia: (attachment: ComposerAttachmentSummary) => void;
  // Removes one attachment by index.
  // 按索引移除单个附件。
  onRemove?: (index: number) => void;
}) {
  return (
    <div className="composer-attachment-strip">
      {props.attachments.map((attachment, index) => (
        <button
          className={cn("composer-attachment-tile", {
            "image-attachment": attachment.kind === "image",
          })}
          key={`${attachment.localPath}-${index}`}
          onClick={() => props.onOpenMedia(attachment)}
          type="button"
        >
          <ComposerAttachmentPreview attachment={attachment} />
          {attachment.kind === "image" ? null : (
            <span className="composer-attachment-name">{attachment.fileName}</span>
          )}
          {props.onRemove ? (
            <span
              aria-label="移除附件"
              className="composer-attachment-remove"
              onClick={(event) => {
                event.stopPropagation();
                props.onRemove?.(index);
              }}
              role="button"
              tabIndex={0}
            >
              <X size={12} />
            </span>
          ) : null}
        </button>
      ))}
    </div>
  );
}

// ComposerAttachmentPreview renders one staged attachment thumbnail or media icon.
// ComposerAttachmentPreview 渲染单个已暂存附件缩略图或媒体图标。
function ComposerAttachmentPreview(props: { attachment: ComposerAttachmentSummary }) {
  const media = composerAttachmentMedia(props.attachment);
  if (media?.kind === "image") {
    return <img alt={props.attachment.fileName} src={mediaSourceUrl(media.localPath)} />;
  }
  if (media?.kind === "video") {
    return <Video size={20} />;
  }
  if (media?.kind === "audio") {
    return <Music2 size={20} />;
  }
  return <FileText size={20} />;
}

// ContextUsageOrb renders the iced-style context usage diagnostic control.
// ContextUsageOrb 渲染 iced 风格的上下文占用诊断控件。
function ContextUsageOrb(props: {
  // Context usage snapshot projected by the service.
  // 服务投影出的上下文占用快照。
  snapshot: ContextUsageSnapshot | null;
}) {
  // t translates fallback labels when the runtime has not prepared a budget yet.
  // t 在运行时尚未准备预算时翻译回退标签。
  const { t } = useI18n();
  const percent = Math.max(0, Math.min(100, Math.round(props.snapshot?.percent ?? 0)));
  const severity = props.snapshot?.severity ?? "unknown";
  const title = props.snapshot?.tooltipTitle || t("contextUsage.title");
  const rows = props.snapshot?.tooltipRows.length ? props.snapshot.tooltipRows : [t("contextUsage.waiting")];
  const style = {
    "--context-usage-progress": `${percent * 3.6}deg`,
  } as React.CSSProperties;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          aria-label={title}
          className={cn("context-usage-orb", `severity-${severity}`)}
          style={style}
          type="button"
        >
          <span className="context-usage-orb-label">{percent}%</span>
        </button>
      </TooltipTrigger>
      <TooltipContent align="end" className="context-usage-tooltip" side="top">
        <strong>{title}</strong>
        {rows.map((row, index) => (
          <span key={`${row}-${index}`}>{row}</span>
        ))}
      </TooltipContent>
    </Tooltip>
  );
}

// ComposerCapabilityIcon renders one semantic capability icon with enabled or grayscale treatment.
// ComposerCapabilityIcon 使用启用彩色或未启用灰度样式渲染单个语义能力图标。
function ComposerCapabilityIcon(props: {
  // Whether this capability is enabled for the current prompt.
  // 当前能力是否在本次提示词中启用。
  enabled: boolean;
  // Optional accessible label for the icon wrapper.
  // 图标外层可选无障碍标签。
  label?: string;
  // Capability option that provides the icon component and accent color.
  // 提供图标组件与强调色的能力选项。
  option: ComposerAiCapabilityOption;
  // Rendered icon size in pixels.
  // 图标渲染像素尺寸。
  size: number;
}) {
  const Icon = props.option.Icon;
  return (
    <span
      aria-label={props.label}
      className={cn("composer-capability-colored-icon", props.enabled ? "enabled" : "disabled")}
      style={{ "--capability-color": props.option.color } as React.CSSProperties}
    >
      <Icon size={props.size} />
    </span>
  );
}

// ComposerAiCapabilityControl renders the iced-aligned capability dialog.
// ComposerAiCapabilityControl 渲染对齐 iced 的能力设置对话框。
function ComposerAiCapabilityControl(props: {
  // Accessible label for the dialog trigger.
  // 对话框触发器的无障碍标签。
  ariaLabel: string;
  // Service-projected capability tree for the composer.
  // 服务投影给输入区的能力树。
  capabilities: AiCapabilitySummary[];
  // Current WebUI locale used for menu labels.
  // 菜单标签使用的当前 WebUI 语言。
  locale: LocaleCode;
  // Runtime-disabled capability keys for the current prompt.
  // 当前提示词运行时禁用的能力键。
  disabledKinds: Set<AiCapabilityKind>;
  // Runtime-disabled target selectors for the current prompt.
  // 当前提示词运行时禁用的目标选择器。
  disabledTargetSelectors: Set<string>;
  // Runtime target order grouped by capability kind.
  // 按能力类型分组的运行时目标顺序。
  targetOrder: Map<AiCapabilityKind, string[]>;
  // Toggles one capability on or off.
  // 开关单个能力。
  onToggle: (kind: AiCapabilityKind, enabled: boolean) => void;
  // Replaces the target route order within one capability.
  // 替换单个能力中的目标路由顺序。
  onTargetReorder: (kind: AiCapabilityKind, targetSelectors: string[]) => void;
  // Toggles one capability target on or off.
  // 开关单个能力目标。
  onTargetToggle: (targetSelector: string, enabled: boolean) => void;
}) {
  // open tracks whether the full capability dialog is visible.
  // open 跟踪完整能力对话框是否可见。
  const [open, setOpen] = React.useState(false);
  // selectedKind stores the capability currently shown in the right pane.
  // selectedKind 保存右侧面板当前展示的能力。
  const [selectedKind, setSelectedKind] = React.useState<AiCapabilityKind>("web_search");
  const capabilityByKind = React.useMemo(() => {
    return new Map(props.capabilities.map((capability) => [capability.kind, capability]));
  }, [props.capabilities]);
  const visibleCapabilities = React.useMemo(() => {
    return COMPOSER_AI_CAPABILITY_OPTIONS.map((option) => ({
      option,
      capability: capabilityByKind.get(option.value),
    }));
  }, [capabilityByKind]);
  const enabledCapabilityEntries = visibleCapabilities.filter(({ option, capability }) => {
    return Boolean(capability?.available) && !props.disabledKinds.has(option.value);
  });
  const enabledCount = enabledCapabilityEntries.length;
  const triggerSubtitle = props.locale === "zh-CN"
    ? `${enabledCount}/${visibleCapabilities.length} 已启用`
    : `${enabledCount}/${visibleCapabilities.length} enabled`;
  const selectedEntry = visibleCapabilities.find(({ option }) => option.value === selectedKind) ?? visibleCapabilities[0];
  const selectedCapability = selectedEntry?.capability;
  const selectedTargets = orderedAiCapabilityTargets(selectedCapability, props.targetOrder);
  const selectedEnabled = Boolean(selectedCapability?.available) && !props.disabledKinds.has(selectedEntry.option.value);
  const targetDragSensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
  const targetSortableIds = React.useMemo(
    () => selectedTargets.map((target) => target.targetSelector),
    [selectedTargets],
  );
  // handleTargetDragEnd applies the dnd-kit order directly to current-message route overrides.
  // handleTargetDragEnd 将 dnd-kit 顺序直接应用到当前消息级路由覆盖。
  const handleTargetDragEnd = React.useCallback((event: DragEndEvent) => {
    const activeId = String(event.active.id);
    const overId = event.over ? String(event.over.id) : null;
    if (!overId || activeId === overId) {
      return;
    }
    const oldIndex = targetSortableIds.indexOf(activeId);
    const newIndex = targetSortableIds.indexOf(overId);
    if (oldIndex < 0 || newIndex < 0) {
      return;
    }
    props.onTargetReorder(selectedEntry.option.value, arrayMove(targetSortableIds, oldIndex, newIndex));
  }, [props, selectedEntry.option.value, targetSortableIds]);

  React.useEffect(() => {
    if (visibleCapabilities.some(({ option }) => option.value === selectedKind)) {
      return;
    }
    const firstCapability = visibleCapabilities[0]?.option.value;
    if (firstCapability) {
      setSelectedKind(firstCapability);
    }
  }, [selectedKind, visibleCapabilities]);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <button
          aria-label={props.ariaLabel}
          className="composer-capability-trigger composer-select-trigger capability"
          type="button"
        >
          <span className="composer-select-icon"><SlidersHorizontal size={15} /></span>
            <span className="composer-select-copy">
              <span className="composer-select-label">{props.locale === "zh-CN" ? "扩展能力" : "Extended Capability"}</span>
              <span className="composer-capability-icon-row" aria-label={triggerSubtitle}>
                {visibleCapabilities.map(({ option, capability }) => (
                  <ComposerCapabilityIcon
                    enabled={Boolean(capability?.available) && !props.disabledKinds.has(option.value)}
                    key={option.value}
                    label={props.locale === "zh-CN" ? option.labelZh : option.labelEn}
                    option={option}
                    size={11}
                  />
                ))}
              </span>
            </span>
        </button>
      </DialogTrigger>
      <DialogContent className="composer-capability-dialog">
        <DialogHeader>
          <DialogTitle>{props.locale === "zh-CN" ? "当前消息能力设置" : "Current-message capabilities"}</DialogTitle>
          <DialogDescription className="composer-capability-dialog-note">
            {props.locale === "zh-CN"
              ? "会话进行中动态变更能力会重建上下文，并影响下一次对话的缓存命中。"
              : "Note: Changing capabilities while a session is running rebuilds context and affects the next turn's cache hit rate."}
          </DialogDescription>
        </DialogHeader>
        <div className="composer-capability-dialog-grid">
          <aside className="composer-capability-dialog-left">
            {visibleCapabilities.map(({ option, capability }) => {
              const available = Boolean(capability?.available);
              const enabled = available && !props.disabledKinds.has(option.value);
              const targetCount = capability?.targets.length ?? 0;
              return (
                <div
                  className={cn(
                    "composer-capability-kind-row",
                    selectedKind === option.value && "selected",
                    !available && "unavailable",
                  )}
                  key={option.value}
                  onClick={() => setSelectedKind(option.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      setSelectedKind(option.value);
                    }
                  }}
                  role="button"
                  tabIndex={0}
                >
                  <input
                    checked={enabled}
                    disabled={!available}
                    onChange={(event) => props.onToggle(option.value, event.currentTarget.checked)}
                    onClick={(event) => event.stopPropagation()}
                    type="checkbox"
                  />
                  <span className="composer-capability-kind-select">
                    <ComposerCapabilityIcon
                      enabled={enabled}
                      option={option}
                      size={16}
                    />
                    <span className="composer-capability-kind-copy">
                      <span>{props.locale === "zh-CN" ? option.labelZh : option.labelEn}</span>
                      <small>
                        {available
                          ? props.locale === "zh-CN"
                            ? `${targetCount} 个目标`
                            : `${targetCount} targets`
                          : props.locale === "zh-CN"
                            ? "暂无可用目标"
                        : "No targets"}
                      </small>
                    </span>
                  </span>
                </div>
              );
            })}
          </aside>
          <section className="composer-capability-dialog-right">
            <div className="composer-capability-detail-header">
              <span className="composer-capability-detail-icon">
                <ComposerCapabilityIcon
                  enabled={selectedEnabled}
                  option={selectedEntry.option}
                  size={18}
                />
              </span>
              <span className="composer-capability-detail-copy">
                <strong>{props.locale === "zh-CN" ? selectedEntry.option.labelZh : selectedEntry.option.labelEn}</strong>
                <small>{props.locale === "zh-CN" ? selectedEntry.option.subtitleZh : selectedEntry.option.subtitleEn}</small>
              </span>
            </div>
            {selectedTargets.length > 0 ? (
              <DndContext sensors={targetDragSensors} collisionDetection={closestCenter} onDragEnd={handleTargetDragEnd}>
                <SortableContext items={targetSortableIds} strategy={verticalListSortingStrategy}>
                  <div className="composer-capability-target-table">
                    {selectedTargets.map((target) => {
                      const targetEnabled = selectedEnabled && target.enabled && !props.disabledTargetSelectors.has(target.targetSelector);
                      return (
                        <ComposerCapabilityTargetRow
                          disabled={!target.enabled}
                          enabled={targetEnabled}
                          key={target.targetSelector}
                          selectedEnabled={selectedEnabled}
                          target={target}
                          onToggle={props.onTargetToggle}
                        />
                      );
                    })}
                  </div>
                </SortableContext>
              </DndContext>
            ) : (
              <div className="composer-capability-dialog-empty">
                {props.locale === "zh-CN" ? "当前能力暂无已配置目标。" : "This capability has no configured targets."}
              </div>
            )}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ComposerCapabilityTargetRow renders one globally draggable capability target route row.
// ComposerCapabilityTargetRow 渲染单条整行可拖动的能力目标路由。
function ComposerCapabilityTargetRow(props: {
  // Whether the target route is globally disabled.
  // 当前目标路由是否全局不可用。
  disabled: boolean;
  // Whether the target route is enabled for this prompt.
  // 当前目标路由是否在本次提示词中启用。
  enabled: boolean;
  // Whether the owning capability is enabled.
  // 所属能力是否启用。
  selectedEnabled: boolean;
  // Target route summary displayed by the row.
  // 当前行展示的目标路由摘要。
  target: AiCapabilityTargetSummary;
  // Toggles the target route for this prompt.
  // 开关本次提示词中的目标路由。
  onToggle: (targetSelector: string, enabled: boolean) => void;
}) {
  const sortable = useVerticalSortableItem(props.target.targetSelector, false);
  return (
    <div
      className={cn("composer-capability-target-row", props.disabled && "disabled", sortable.isDragging && "dragging")}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
      {...sortable.attributes}
      {...sortable.listeners}
    >
      <GripVertical className="composer-capability-target-handle" size={14} />
      <input
        checked={props.enabled}
        disabled={!props.selectedEnabled || !props.target.enabled}
        onChange={(event) => props.onToggle(props.target.targetSelector, event.currentTarget.checked)}
        onPointerDown={(event) => event.stopPropagation()}
        type="checkbox"
      />
      <span className="composer-capability-target-copy">
        <span>{props.target.targetName || props.target.targetAlias}</span>
        <small>{props.target.vendorName} · {props.target.modelId || props.target.targetAlias}</small>
      </span>
    </div>
  );
}

// ComposerSelectControl renders one iced-aligned compact composer selector.
// ComposerSelectControl 渲染一个对齐 iced 的紧凑输入区选择器。
function ComposerSelectControl(props: {
  // Accessible label for the dropdown trigger.
  // 下拉触发器的无障碍标签。
  ariaLabel: string;
  // Localized fallback label shown when the current value is missing.
  // 当前值缺失时显示的本地化回退标签。
  fallbackLabel: string;
  // Optional visual icon rendered in the left accessory slot.
  // 渲染在左侧附属槽位中的可选视觉图标。
  icon?: React.ReactNode;
  // Optional subtitle rendered only inside the closed trigger.
  // 仅在闭合触发器中渲染的可选副标题。
  triggerSubtitle?: string;
  // CSS class suffix controlling the iced-aligned fixed width.
  // 控制 iced 对齐固定宽度的 CSS 类后缀。
  className: string;
  // Available local options for this selector.
  // 当前选择器可用的本地选项。
  options: ComposerSelectOption[];
  // Current selected option value.
  // 当前选中的选项值。
  value: string;
  // Optional signal that opens the selector from an external slash-command route.
  // 可选信号，用于从外部 slash 指令路由打开选择器。
  openSignal?: number;
  // Selection change handler.
  // 选项变更处理函数。
  onValueChange: (value: string) => void;
}) {
  // open controls the Radix select popover so slash commands can reuse it.
  // open 控制 Radix 选择弹窗，使 slash 指令可复用它。
  const [open, setOpen] = React.useState(false);
  const selectedOption = props.options.find((option) => option.value === props.value) ?? props.options[0];

  React.useEffect(() => {
    if (props.openSignal === undefined || props.openSignal <= 0) {
      return;
    }
    setOpen(true);
  }, [props.openSignal]);

  return (
    <Select onOpenChange={setOpen} onValueChange={props.onValueChange} open={open} value={props.value}>
      <SelectTrigger
        aria-label={props.ariaLabel}
        className={cn("composer-select-trigger", props.className)}
      >
        {props.icon ? <span className="composer-select-icon">{props.icon}</span> : null}
        <span className={cn("composer-select-copy", !props.icon && "centered")}>
          <span className="composer-select-label">{selectedOption?.label ?? props.fallbackLabel}</span>
          {props.triggerSubtitle || selectedOption?.subtitle ? (
            <span className="composer-select-subtitle">{props.triggerSubtitle ?? selectedOption?.subtitle}</span>
          ) : null}
        </span>
      </SelectTrigger>
      <SelectContent align="start" className="composer-select-content" position="popper">
        {props.options.map((option) => (
          <SelectItem className="composer-select-item" key={option.value} value={option.value}>
            <span className="composer-select-item-copy">
              <span>{option.label}</span>
              {option.subtitle ? <small>{option.subtitle}</small> : null}
            </span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

// composerShortcutHint returns a platform-specific keyboard hint for the composer placeholder.
// composerShortcutHint 返回输入区 placeholder 使用的平台化快捷键提示。
function composerShortcutHint(baseHint: string): string {
  const platformText = typeof navigator === "undefined"
    ? ""
    : `${navigator.platform} ${navigator.userAgent}`.toLowerCase();
  if (platformText.includes("mac")) {
    return baseHint;
  }
  return baseHint;
}

// EmptyConversationPrompt renders the blank-chat prompt above the shared composer dock.
// EmptyConversationPrompt 在共享输入区上方渲染空会话提示。
function EmptyConversationPrompt(props: {
  // Whether the prompt is playing its exit animation.
  // 当前提示是否正在播放退出动画。
  leaving: boolean;
  // Project name used by the empty-state prompt.
  // 空状态提示使用的项目名称。
  projectName: string;
}) {
  // t translates the empty conversation prompt while projectName remains user data.
  // t 翻译空会话引导语，projectName 仍保留为用户数据。
  const { t } = useI18n();
  // promptText inserts the selected project name into the localized empty prompt.
  // promptText 将当前项目名称插入本地化空会话提示。
  const promptText = t("console.emptyPrompt").replace("{projectName}", props.projectName);

  return (
    <section className={cn("empty-conversation-prompt", props.leaving && "leaving")} aria-hidden={props.leaving}>
      <h2>{promptText}</h2>
    </section>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

