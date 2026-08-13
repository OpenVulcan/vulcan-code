import type { LocaleCode } from "@/i18n";
import type {
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTargetSelection,
} from "@/types";

// ComposerProviderToolSettingsSnapshot freezes capability switches and exact routes separately.
// ComposerProviderToolSettingsSnapshot 分别固定能力开关与精确路由。
export interface ComposerProviderToolSettingsSnapshot {
  // Capabilities enabled only for the target conversation.
  // 仅为目标会话启用的能力。
  enabledCapabilities: PublicAiProviderToolCapabilityKind[];
  // Exact model and account retained for each configured capability.
  // 每项已配置能力保留的精确模型与账号。
  selections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
  >;
  // Native model-extension targets retained for every generative capability.
  // 每项生成类能力保留的原生模型扩展目标集合。
  extensionTargetSets: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection[]>
  >;
}

// ComposerModelSelectionSnapshot freezes the exact provider, model, and credential used by one submission.
// ComposerModelSelectionSnapshot 固定单次提交使用的精确供应商、模型与凭据。
export interface ComposerModelSelectionSnapshot {
  // Stable provider identifier selected in the composer.
  // 输入区中选择的稳定供应商标识。
  providerId: string;
  // Stable model identifier selected in the composer.
  // 输入区中选择的稳定模型标识。
  modelId: string;
  // Exact credential connection selected in the composer.
  // 输入区中选择的精确凭据连接。
  connectionId: string;
}

// ComposerVmmPolicySnapshot freezes the draft conversation's two tri-state memory policies.
// ComposerVmmPolicySnapshot 固定草稿会话的两项三态记忆策略。
export interface ComposerVmmPolicySnapshot {
  // Precheck-memory injection mode.
  // 预检记忆注入模式。
  precheckMode: "inherit" | "enabled" | "disabled";
  // Post-turn memory writeback mode.
  // 回合后记忆回写模式。
  memoryWritebackMode: "inherit" | "enabled" | "disabled";
}

// ComposerKnowledgeSelectionSnapshot freezes a project-authorized draft knowledge selection.
// ComposerKnowledgeSelectionSnapshot 固定一份通过项目授权的草稿知识库选择。
export interface ComposerKnowledgeSelectionSnapshot {
  // Inherited or explicit selection mode.
  // 继承或显式选择模式。
  mode: "inherit" | "explicit";
  // Explicit library identifiers; inherited mode always uses an empty list.
  // 显式知识库标识；继承模式始终使用空列表。
  libraryIds: string[];
}

// ComposerInitialGoalSnapshot freezes an optional goal created before the first prompt.
// ComposerInitialGoalSnapshot 固定首次提示词前创建的可选目标。
export interface ComposerInitialGoalSnapshot {
  // Human-authored objective.
  // 用户编写的目标文本。
  objective: string;
  // Optional positive token budget.
  // 可选正数 Token 预算。
  tokenBudget: number | null;
}

// ComposerSessionSettingsSnapshot contains draft-only session overrides applied during materialization.
// ComposerSessionSettingsSnapshot 包含在物化期间应用的草稿专属会话覆盖项。
export interface ComposerSessionSettingsSnapshot {
  // Optional output-style override captured only when creating a new conversation.
  // 仅在创建新会话时捕获的可选输出风格覆盖。
  outputStyleId?: string;
  // Optional explicit VMM policy pair.
  // 可选显式 VMM 策略对。
  vmmPolicy?: ComposerVmmPolicySnapshot;
  // Optional complete provider-tool snapshot; absence materializes system defaults.
  // 可选完整供应商工具快照；缺省时物化系统默认值。
  providerToolSettings?: ComposerProviderToolSettingsSnapshot;
  // Optional knowledge-library override.
  // 可选知识库覆盖。
  knowledgeSelection?: ComposerKnowledgeSelectionSnapshot;
  // Optional initial goal.
  // 可选初始目标。
  initialGoal?: ComposerInitialGoalSnapshot;
}

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
  // Exact model selection captured atomically with this submission.
  // 与本次提交原子捕获的精确模型选择。
  modelSelection: ComposerModelSelectionSnapshot | null;
  // Draft-only session settings captured atomically with the first submission.
  // 与首次提交原子捕获的草稿专属会话设置。
  sessionSettings: ComposerSessionSettingsSnapshot | null;
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
  if (element.closest('[data-terminal-input-scope="true"]')) {
    return true;
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

