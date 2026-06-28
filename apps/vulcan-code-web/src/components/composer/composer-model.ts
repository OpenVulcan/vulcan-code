import React from "react";
import { Globe, Image as ImageIcon, ImagePlus, MicVocal, Music2, Video, Volume2 } from "lucide-react";
import type { LocaleCode } from "@/i18n";
import type {
  AiCapabilitySummary,
  AiCapabilityTargetSummary,
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
} from "@/types";

// ComposerSubmission contains the user prompt and runtime quick-control values.
// ComposerSubmission 包含用户提示词与运行时快捷控制取值。
export interface ComposerSubmission {
  // User-entered prompt text to append into the active conversation.
  // 需要追加到当前会话中的用户输入提示词。
  prompt: string;
  // Optional hidden host guidance appended only to the model-facing prompt.
  // 仅追加到模型可见提示词中的可选宿主隐藏引导。
  hiddenModelGuidance?: string;
  // Staged attachments submitted before the prompt text.
  // 在提示词文本之前提交的已暂存附件。
  attachments: ComposerAttachmentSummary[];
  // Composer execution mode selected by the user.
  // 用户选择的输入区执行模式。
  executionMode: string;
  // Backend-confirmed reasoning effort selected by the user.
  // 用户选择且已由后端确认的思考档位。
  reasoningEffort: string | null;
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

// ComposerAiCapabilityConfigurationSave describes one session-scoped capability dialog save.
// ComposerAiCapabilityConfigurationSave 描述一次会话级能力弹窗保存。
export interface ComposerAiCapabilityConfigurationSave {
  // Complete disabled semantic capability kind list.
  // 完整禁用语义能力类型列表。
  disabledAiCapabilityKinds: AiCapabilityKind[];
  // Complete disabled target selector list.
  // 完整禁用目标选择器列表。
  disabledAiCapabilityTargetSelectors: string[];
  // Complete target order overrides grouped by capability kind.
  // 按能力类型分组的完整目标顺序覆盖。
  aiCapabilityTargetOrderOverrides: Record<string, string[]>;
}

// ComposerSlashCommandOpenTarget names existing composer popups opened by slash commands.
// ComposerSlashCommandOpenTarget 命名 slash 指令可打开的现有输入区弹窗。
export type ComposerSlashCommandOpenTarget = "model" | "reasoning";

// ComposerSlashCommandResult describes the routed effect of one slash command activation.
// ComposerSlashCommandResult 描述单个 slash 指令激活后的路由效果。
export interface ComposerSlashCommandResult {
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
export const SLASH_SKILL_PROMPT = "Decide whether the current task needs the `skill` tool to load details for enabled prompt skills.";

// ComposerSelectOption describes one local-only composer dropdown choice.
// ComposerSelectOption 描述一个仅用于输入区本地预览的下拉选项。
export interface ComposerSelectOption {
  // Stable option value used by the shadcn Select state.
  // shadcn Select 状态使用的稳定选项值。
  value: string;
  // Primary label rendered in the compact trigger.
  // 紧凑触发器中渲染的主标签。
  label: string;
  // Optional secondary line matching the WebUI dual-line trigger subtitle.
  // 可选副标题，对齐 WebUI 双行触发器的第二行。
  subtitle?: string;
}

// AiCapabilityKind mirrors provider-config AiToolKind snake_case keys supported by WebUI quick toggles.
// AiCapabilityKind 对齐 WebUI 快捷开关支持的 provider-config AiToolKind snake_case 键。
export type AiCapabilityKind =
  | "web_search"
  | "understand_image"
  | "image_generation"
  | "video_generation"
  | "speech_synthesis"
  | "music_generation"
  | "music_cover";

// ComposerAiCapabilityOption describes one quick-toggle row in the composer capability menu.
// ComposerAiCapabilityOption 描述输入区能力菜单中的单个快捷开关行。
export interface ComposerAiCapabilityOption {
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

// COMPOSER_AI_CAPABILITY_OPTIONS keeps the WebUI quick menu in the same semantic order as the WebUI workbench.
// COMPOSER_AI_CAPABILITY_OPTIONS 保持 WebUI 快捷菜单与 WebUI 工作台相同的语义顺序。
export const COMPOSER_AI_CAPABILITY_OPTIONS: ComposerAiCapabilityOption[] = [
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
export function defaultDisabledAiCapabilityKinds(capabilities: AiCapabilitySummary[]): Set<AiCapabilityKind> {
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
export function defaultDisabledAiCapabilityTargetSelectors(capabilities: AiCapabilitySummary[]): Set<string> {
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
export function defaultAiCapabilityTargetOrder(capabilities: AiCapabilitySummary[]): Map<AiCapabilityKind, string[]> {
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
export function serializeAiCapabilityTargetOrder(order: Map<AiCapabilityKind, string[]>): Record<string, string[]> {
  return Object.fromEntries(Array.from(order.entries()).map(([kind, selectors]) => [kind, selectors]));
}

// orderedAiCapabilityTargets returns targets in the current prompt-level display order.
// orderedAiCapabilityTargets 按当前提示词级显示顺序返回目标列表。
export function orderedAiCapabilityTargets(
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
export function aiCapabilitySignature(capabilities: AiCapabilitySummary[]): string {
  return capabilities
    .map((capability) => [
      capability.kind,
      capability.available ? "1" : "0",
      capability.enabled ? "1" : "0",
      capability.native ? "1" : "0",
      capability.bridged ? "1" : "0",
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
export function isComposerAiCapabilityKind(value: string): value is AiCapabilityKind {
  return COMPOSER_AI_CAPABILITY_OPTIONS.some((option) => option.value === value);
}

// MAX_COMPOSER_ATTACHMENTS matches the WebUI composer attachment limit.
// MAX_COMPOSER_ATTACHMENTS 对齐 WebUI 输入区附件数量上限。
export const MAX_COMPOSER_ATTACHMENTS = 10;

// normalizedComposerSlashCommandId returns the stable action id for a projected command.
// normalizedComposerSlashCommandId 返回投影指令对应的稳定动作标识。
export function normalizedComposerSlashCommandId(command: ComposerSlashCommandSummary): string {
  const id = command.id.trim().toLowerCase();
  if (id) {
    return id;
  }
  return command.command.replace(/^\/+/, "").trim().toLowerCase();
}

// composerSlashCommandDescription returns the locale-specific description for one command row.
// composerSlashCommandDescription 返回单个指令行的当前语言说明。
export function composerSlashCommandDescription(command: ComposerSlashCommandSummary, locale: LocaleCode): string {
  return locale === "en-US" ? command.descriptionEn : command.descriptionZh;
}

// composerSlashCommandMatchesInput mirrors the native slash-command prefix matching rule.
// composerSlashCommandMatchesInput 对齐原生 slash 指令前缀匹配规则。
export function composerSlashCommandMatchesInput(command: ComposerSlashCommandSummary, input: string): boolean {
  if (!input.startsWith("/")) {
    return false;
  }
  const filterText = input.slice(1).trim().toLowerCase();
  const trigger = command.command.replace(/^\/+/, "").toLowerCase();
  return trigger.startsWith(filterText);
}

// exactComposerSlashCommand finds a command whose trigger exactly equals the typed input.
// exactComposerSlashCommand 查找触发词与当前输入完全相等的指令。
export function exactComposerSlashCommand(
  commands: ComposerSlashCommandSummary[],
  input: string,
): ComposerSlashCommandSummary | null {
  const normalizedInput = input.trim();
  return commands.find((command) => command.command.localeCompare(normalizedInput, undefined, { sensitivity: "accent" }) === 0) ?? null;
}

// isComposerTextEntryElement detects native or rich text-entry targets that should keep focus.
// isComposerTextEntryElement 检测应保留焦点的原生或富文本输入目标。
export function isComposerTextEntryElement(element: Element | null): boolean {
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
export function hasBlockingComposerPopup(): boolean {
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
export function isComposerPrintableKey(event: KeyboardEvent): boolean {
  return event.key.length === 1
    && !event.ctrlKey
    && !event.metaKey
    && !event.altKey
    && !event.isComposing;
}


// composerAttachmentAccept joins browser accept strings for enabled attachment capabilities.
// composerAttachmentAccept 拼接已启用附件能力的浏览器 accept 字符串。
export function composerAttachmentAccept(capabilities: ComposerAttachmentCapabilitySummary[]): string {
  return capabilities
    .filter((capability) => capability.enabled)
    .map((capability) => capability.accept)
    .filter(Boolean)
    .join(",");
}

