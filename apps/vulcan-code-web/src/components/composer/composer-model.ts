import type { AnswerMode } from "@/service/answer-mode";
import type { PermissionMode } from "@/service/permission-mode";
import type { DedicatedToolAccess } from "@/service/conversation-event-window-client";
import type { TranslationKey } from "@/i18n";
import type { ComposerSettingsSection } from "@/components/composer/composer-settings-request";
import type {
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  PublicAiMediaUnderstandingBackendSelection,
  PublicAiProviderExtensionModeSelection,
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
  // Non-default extension modes; a missing capability is single mode with no model.
  // 非默认扩展模式；缺失的能力即未选模型的单模型模式。
  extensionModes: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderExtensionModeSelection>
  >;
  // Exact session-owned route retained independently for each media-understanding capability.
  // 为每项媒体理解能力独立保留的会话自有精确路由。
  mediaUnderstandingSelections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiMediaUnderstandingBackendSelection>
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
  // Explicit answer mode for the new session; absence copies the global default.
  // 新会话的显式回答模式；缺省时复制全局默认值。
  answerMode?: AnswerMode;
  // Explicit permission mode for the new session; absence keeps the session default.
  // 新会话的显式权限模式；缺省时保留会话默认值。
  permissionMode?: PermissionMode;
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
  // Present only when the draft creates a dedicated session; it cannot be combined with a goal or Plan mode.
  // 仅当草稿创建专职会话时存在；不能与目标或计划模式同时使用。
  dedicated?: ComposerDedicatedSessionSnapshot;
  // Session skill switches set by a flow that starts a bundled on-demand skill, such as on-device fixing.
  // 启动随包按需技能的流程（如本机修复）设置的会话技能开关。
  promptSkillOverrides?: ComposerPromptSkillOverrides;
}

// ComposerPromptSkillOverrides switches skills by name for one new conversation.
// ComposerPromptSkillOverrides 按名称为一个新会话切换技能。
export type ComposerPromptSkillOverrides = Readonly<Record<string, "enabled" | "disabled">>;

// ComposerDedicatedSessionSnapshot freezes the dedicated-session choices of the creation wizard.
// ComposerDedicatedSessionSnapshot 固定创建向导的专职会话选择。
export interface ComposerDedicatedSessionSnapshot {
  // General tool access fixed at creation.
  // 创建时固定的通用工具开放方式。
  toolAccess: DedicatedToolAccess;
  // User-confirmed base prompt, sent as the session's system instruction and never changed later.
  // 用户确认的基础提示词，作为会话系统指令发送且之后不再修改。
  basePrompt: string;
  // Whether memory is locked before the first user input.
  // 是否在首条用户输入之前锁定记忆。
  lockImmediately: boolean;
}

// ComposerSubmission contains the user prompt and runtime quick-control values.
// ComposerSubmission 包含用户提示词与运行时快捷控制取值。
export interface ComposerSubmission {
  // Stable composer draft slot whose attachment lease transfers with this submission.
  // 随本次提交转移附件租约的稳定输入区草稿槽。
  draftSlotKey: string;
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
export type ComposerSlashCommandOpenTarget = "model" | "reasoning" | "goal" | "settings";

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
  // Settings submenu opened together with the settings menu; null opens only the menu.
  // 与设置菜单一同打开的子菜单；为 null 时只打开菜单。
  settingsSection?: ComposerSettingsSection | null;
}

// ComposerSlashCommandDedicatedSupport states whether a command applies to dedicated sessions.
// ComposerSlashCommandDedicatedSupport 说明指令是否适用于专职会话。
// "never" covers settings frozen at creation or features dedicated sessions do not use;
// "tools" applies only when the session was created with general tools.
// "never" 覆盖创建时冻结的设置及专职会话不使用的功能；"tools" 仅适用于以通用工具创建的会话。
type ComposerSlashCommandDedicatedSupport = "always" | "never" | "tools";

// ComposerSlashCommandDescriptor binds one routable command id to its catalog text and scope.
// ComposerSlashCommandDescriptor 把一个可路由指令标识绑定到目录文案与适用范围。
interface ComposerSlashCommandDescriptor {
  descriptionKey: TranslationKey;
  dedicated: ComposerSlashCommandDedicatedSupport;
}

// COMPOSER_SLASH_COMMAND_DESCRIPTORS lists every command id this client can route.
// COMPOSER_SLASH_COMMAND_DESCRIPTORS 列出本客户端可路由的全部指令标识。
const COMPOSER_SLASH_COMMAND_DESCRIPTORS: Readonly<Record<string, ComposerSlashCommandDescriptor>> = {
  model: { descriptionKey: "composer.slash.model", dedicated: "always" },
  reasoning: { descriptionKey: "composer.slash.reasoning", dedicated: "always" },
  compact: { descriptionKey: "composer.slash.compact", dedicated: "never" },
  plan: { descriptionKey: "composer.slash.plan", dedicated: "never" },
  goal: { descriptionKey: "composer.slash.goal", dedicated: "never" },
  settings: { descriptionKey: "composer.slash.settings", dedicated: "always" },
  style: { descriptionKey: "composer.slash.style", dedicated: "never" },
  answer: { descriptionKey: "composer.slash.answer", dedicated: "never" },
  knowledge: { descriptionKey: "composer.slash.knowledge", dedicated: "never" },
  collapse: { descriptionKey: "composer.slash.collapse", dedicated: "always" },
  thinking: { descriptionKey: "composer.slash.thinking", dedicated: "always" },
  memory: { descriptionKey: "composer.slash.memory", dedicated: "never" },
  skill: { descriptionKey: "composer.slash.skill", dedicated: "tools" },
  skills: { descriptionKey: "composer.slash.skills", dedicated: "tools" },
};

// composerSlashCommandsForSession keeps the routable commands that apply to the current session.
// composerSlashCommandsForSession 保留适用于当前会话的可路由指令。
// Parameters: commands is the service catalog; dedicatedToolAccess is null for ordinary sessions and drafts.
// 参数：commands 是服务目录；普通会话与草稿的 dedicatedToolAccess 为 null。
// Returns: commands in catalog order, without ids this client cannot route.
// 返回值：按目录顺序排列的指令，不含本客户端无法路由的标识。
export function composerSlashCommandsForSession(
  commands: ComposerSlashCommandSummary[],
  dedicatedToolAccess: DedicatedToolAccess | null,
): ComposerSlashCommandSummary[] {
  return commands.filter((command) => {
    const descriptor = COMPOSER_SLASH_COMMAND_DESCRIPTORS[normalizedComposerSlashCommandId(command)];
    if (!descriptor) {
      return false;
    }
    if (dedicatedToolAccess === null || descriptor.dedicated === "always") {
      return true;
    }
    return descriptor.dedicated === "tools" && dedicatedToolAccess === "standard";
  });
}

// SLASH_SKILL_PROMPT matches the native workbench prompt inserted by /skill.
// SLASH_SKILL_PROMPT 对齐原生工作台中 /skill 插入的提示词。
export const SLASH_SKILL_PROMPT = "Decide whether the current task needs the `skill` tool to load details for enabled prompt skills.";

// ComposerSelectOption describes one local-only composer dropdown choice.
// ComposerSelectOption 描述一个仅用于输入区本地预览的下拉选项。
export interface ComposerSelectOption {
  // Stable option value used by the kit Select state.
  // 套件 Select 状态使用的稳定选项值。
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

// composerSlashCommandDescriptionKey returns the catalog key describing one routable command.
// composerSlashCommandDescriptionKey 返回描述单个可路由指令的目录键。
// Returns null for ids this client cannot route; such commands are never listed.
// 对本客户端无法路由的标识返回 null；这类指令不会被列出。
export function composerSlashCommandDescriptionKey(command: ComposerSlashCommandSummary): TranslationKey | null {
  return COMPOSER_SLASH_COMMAND_DESCRIPTORS[normalizedComposerSlashCommandId(command)]?.descriptionKey ?? null;
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

