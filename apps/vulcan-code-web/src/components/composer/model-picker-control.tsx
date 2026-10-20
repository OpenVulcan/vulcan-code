import * as React from "react";
import { Check, ChevronDown, Cpu } from "lucide-react";
import { localizedProviderName } from "@/components/features/settings/provider-presentation";
import { ModelBuiltinCapabilityIcons } from "@/components/shared/model-builtin-capabilities";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui-kit/primitives/popover";
import { Command, CommandInput, CommandItem, CommandList } from "@/ui-kit/primitives/command";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import type {
  ProviderSummary,
  PublicAiNativeToolKind,
  PublicAiResourceKind,
  PublicAiSettingsSnapshot,
} from "@/types";

// ModelPickerChoice describes one flattened workbench model row.
// ModelPickerChoice 描述模型筛选器中的单条扁平工作台模型行。
export interface ModelPickerChoice {
  // Stable provider identifier owning this model.
  // 拥有该模型的稳定 Provider 标识。
  providerId: string;
  // Exact credential connection selected for this row.
  // 本行选定的精确凭据连接。
  connectionId: string;
  // Human-readable provider label.
  // 人类可读的 Provider 标签。
  providerName: string;
  // Human-readable credential account label.
  // 人类可读的凭据账号标签。
  accountName: string;
  // Stable model identifier persisted with provider id.
  // 与 Provider 标识一起持久化的稳定模型标识。
  modelId: string;
  // Human-readable model label.
  // 人类可读的模型标签。
  modelName: string;
  // Non-text resources accepted by this exact credential and model route.
  // 此精确凭据与模型路由接受的非文本资源。
  inputModalities: PublicAiResourceKind[];
  // Provider-native tools available on this exact credential and model route.
  // 此精确凭据与模型路由可用的供应商原生工具。
  providerNativeTools: PublicAiNativeToolKind[];
  // Whether this row is the active workbench model.
  // 该行是否为当前激活的工作台模型。
  active: boolean;
  // Whether this connection is the backend-resolved preferred account for the model.
  // 该连接是否为后端针对当前模型解析出的首选账号。
  preferredConnection: boolean;
}

// ModelPickerModelGroup represents one provider/model row with all eligible credential accounts.
// ModelPickerModelGroup 表示包含全部可用凭据账号的单个供应商/模型行。
export interface ModelPickerModelGroup {
  // Stable provider identifier shared by every account.
  // 所有账号共享的稳定 Provider 标识。
  providerId: string;
  // Human-readable provider label shared by every account.
  // 所有账号共享的人类可读 Provider 标签。
  providerName: string;
  // Stable model identifier shared by every account.
  // 所有账号共享的稳定模型标识。
  modelId: string;
  // Human-readable model label shared by every account.
  // 所有账号共享的人类可读模型标签。
  modelName: string;
  // Exact account choices exposing this provider model.
  // 暴露该 Provider 模型的精确账号选项。
  accounts: ModelPickerChoice[];
}

// resolveModelPickerDisplayedChoice preserves an exact requested route and uses an active row only when no exact value is requested.
// resolveModelPickerDisplayedChoice 保留精确请求路由，仅在未请求精确值时使用激活行。
// Parameters: choices contains authoritative executable rows and value is the exact provider, model, and connection key.
// 参数：choices 包含权威可执行行，value 是精确 Provider、模型与连接键。
// Returns: the exact requested row, the backend-active row for an empty value, or undefined when explicit reselection is required.
// 返回值：返回精确请求行、空值时的后端激活行，或在必须显式重新选择时返回 undefined。
export function resolveModelPickerDisplayedChoice(
  choices: ModelPickerChoice[],
  value: string,
): ModelPickerChoice | undefined {
  if (value) {
    return choices.find((choice) => modelPickerChoiceKey(choice) === value);
  }
  return choices.find((choice) => choice.active);
}

// ModelPickerControl renders the shared workbench model filter trigger and list.
// ModelPickerControl 渲染共享的工作台模型筛选触发器与列表。
// Parameters: props supplies credential-scoped choices, selection and preference callbacks; returns a searchable keyboard-navigable model picker and exact account control.
// 参数：props 提供凭据作用域选项、选择与偏好回调；返回可搜索、可键盘导航的模型选择器及精确账号控件。
export function ModelPickerControl(props: {
  // Accessible label applied to the trigger button.
  // 应用于触发按钮的无障碍标签。
  ariaLabel: string;
  // Fallback label when no model is available.
  // 没有可用模型时显示的回退标签。
  fallbackLabel: string;
  // Optional label shown when the requested value is no longer in the choice list.
  // 请求值不再位于选择列表中时显示的可选标签。
  unavailableValueLabel?: string;
  // Placeholder shown in the filter input.
  // 过滤输入框中显示的占位文案。
  filterPlaceholder: string;
  // Empty-state label when the filter yields no rows.
  // 过滤后无匹配项时显示的空状态文案。
  noMatchesLabel: string;
  // Providers carrying enabled model rows from the shared service API.
  // 来自共享 service API、携带已启用模型行的 Provider 列表。
  providers: ProviderSummary[];
  // Exact JSON tuple: provider/model/connection by default, or provider/model for model scope.
  // 精确 JSON 元组：默认包含供应商/模型/连接；模型作用域只包含供应商/模型。
  value: string;
  // Model scope preserves settings that intentionally do not persist a credential account.
  // 模型作用域保留明确不持久化凭据账号的设置语义。
  selectionScope?: "account" | "model";
  // Called when the user selects one model row.
  // 用户选择单个模型行时触发。
  onValueChange: (choice: ModelPickerChoice) => void;
  // Optional signal that opens the picker from an external slash-command route.
  // 可选信号，用于从外部 slash 指令路由打开筛选器。
  openSignal?: number;
  // Whether the picker trigger is temporarily disabled by the host flow.
  // 筛选器触发器是否被宿主流程临时禁用。
  disabled?: boolean;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  // observedOpenSignal consumes each explicit command once, including commands arriving while unavailable.
  // observedOpenSignal 每条显式指令只消费一次，包括不可用期间到达的指令。
  const observedOpenSignal = React.useRef(props.openSignal);
  // choices localizes built-in provider labels without changing exact provider or account identities.
  // choices 本地化内置供应商标签，同时不改变精确供应商或账号身份。
  const choices = React.useMemo(() => (
    modelPickerChoicesFromProviders(props.providers).map((choice) => ({
      ...choice,
      providerName: localizedProviderName(choice.providerId, choice.providerName, t),
    }))
  ), [props.providers, t]);
  // selectedChoice never substitutes another provider, model, or account for an unavailable exact value.
  // selectedChoice 绝不会用其他 Provider、模型或账号替代不可用的精确值。
  const selectedChoice = props.selectionScope === "model" ? undefined : resolveModelPickerDisplayedChoice(choices, props.value);
  // selectedModelGroup resolves only the declared model tuple without inventing a selected account.
  // selectedModelGroup 仅解析已声明的模型元组，不虚构已选账号。
  const selectedModelGroup = props.selectionScope === "model"
    ? groupModelPickerChoices(choices).find((group) => modelPickerGroupKey(group) === props.value)
    : undefined;
  const selectedModelLabel = selectedModelGroup?.modelName ?? selectedChoice?.modelName ?? props.unavailableValueLabel ?? props.fallbackLabel;
  const selectedProviderLabel = selectedModelGroup?.providerName ?? selectedChoice?.providerName ?? "-";
  // selectedAccountLabel is the exact credential label owned by the selected connection.
  // selectedAccountLabel 是所选连接拥有的精确凭据标签。
  const selectedAccountLabel = selectedChoice?.accountName ?? "-";
  React.useEffect(() => {
    // changed distinguishes a new slash intent from mounting or restoring the model directory.
    // changed 区分新的斜杠意图与挂载或恢复模型目录。
    const changed = observedOpenSignal.current !== props.openSignal;
    observedOpenSignal.current = props.openSignal;
    if (props.disabled || choices.length === 0) {
      setOpen(false);
      return;
    }
    if (changed && props.openSignal !== undefined && props.openSignal > 0) setOpen(true);
  }, [props.openSignal, props.disabled, choices.length]);

  return (
    <Popover open={open && !props.disabled && choices.length > 0} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <button
              aria-label={props.ariaLabel}
              className="vk-pick"
              data-kind="model"
              disabled={props.disabled || choices.length === 0}
              type="button"
            >
              <span className="vk-pick-icon"><Cpu size={15} /></span>
              <span className="vk-pick-label">{selectedModelLabel}</span>
              {/* The chevron marks the trigger as a dropdown, matching the reasoning and account selects. */}
              {/* 下拉箭头表明这是一个下拉选择，与推理强度、账号选择保持一致。 */}
              <ChevronDown aria-hidden="true" className="vk-pick-chevron" />
            </button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent className="vk-pick-tip" side="top">
          <strong>{t("composer.modelPicker.tooltipTitle")}</strong>
          <span>{formatModelPickerTranslation(t, "composer.modelPicker.currentModel", { model: selectedModelLabel })}</span>
          <span>{formatModelPickerTranslation(t, "composer.modelPicker.currentProvider", { provider: selectedProviderLabel || "-" })}</span>
          {props.selectionScope !== "model" ? <span>{formatModelPickerTranslation(t, "composer.modelPicker.currentAccount", { account: selectedAccountLabel })}</span> : null}
        </TooltipContent>
      </Tooltip>
      <PopoverContent align="start" className="vk-model-pop" sideOffset={8}>
        <ModelPickerList
          choices={choices}
          filterPlaceholder={props.filterPlaceholder}
          noMatchesLabel={props.noMatchesLabel}
          onDismiss={() => setOpen(false)}
          onSelect={(choice) => {
            props.onValueChange(choice);
            setOpen(false);
          }}
          selectedChoice={selectedChoice}
          selectionScope={props.selectionScope}
          value={props.value}
        />
      </PopoverContent>
    </Popover>
  );
}

// ModelPickerList renders the searchable grouped model list shared by the standalone and merged pickers.
// ModelPickerList 渲染独立选择器与合并选择器共用的可搜索分组模型列表。
// Parameters: props supplies localized choices, the exact selection and row callbacks; returns a cmdk list
// that starts with an empty filter on mount and scrolls the selected row into view once laid out.
// 参数：props 提供本地化选项、精确选择与行回调；返回 cmdk 列表，挂载时过滤为空并在布局后滚动到选中行。
export function ModelPickerList(props: {
  // Localized flat choices from the authoritative provider snapshot.
  // 来自权威 Provider 快照的本地化扁平选项。
  choices: ModelPickerChoice[];
  // Exact selected account row, absent for model-scope selections.
  // 精确选中的账号行；模型作用域选择时缺省。
  selectedChoice: ModelPickerChoice | undefined;
  // Model scope compares grouped keys instead of exact account rows.
  // 模型作用域比较分组键而非精确账号行。
  selectionScope?: "account" | "model";
  // Exact selected value, compared directly for model-scope selections.
  // 精确选中值；模型作用域时直接比较。
  value: string;
  // Placeholder shown in the filter input.
  // 过滤输入框中显示的占位文案。
  filterPlaceholder: string;
  // Empty-state label when the filter yields no rows.
  // 过滤后无匹配项时显示的空状态文案。
  noMatchesLabel: string;
  // Called with the resolved account choice when a model row is chosen.
  // 选择模型行时以解析后的账号选项调用。
  onSelect: (choice: ModelPickerChoice) => void;
  // Called when Tab or Escape leaves the filter input.
  // Tab 或 Escape 离开过滤输入框时调用。
  onDismiss: () => void;
}) {
  const [filter, setFilter] = React.useState("");
  // selectedRowRef anchors the exact active model row after the dropdown portal finishes layout.
  // selectedRowRef 在下拉门户完成布局后锚定精确的当前模型行。
  const selectedRowRef = React.useRef<HTMLElement | null>(null);
  const filteredChoices = React.useMemo(
    () => filterModelPickerChoices(props.choices, filter),
    [props.choices, filter],
  );
  // filteredGroups collapses credential duplicates only for the composer session picker.
  // filteredGroups 仅为会话输入区模型选择器折叠凭据重复项。
  const filteredGroups = React.useMemo(
    () => groupModelPickerChoices(filteredChoices),
    [filteredChoices],
  );

  React.useEffect(() => {
    // Two animation frames place the row only after Radix positions the portal and ScrollArea viewport.
    // 两帧动画确保 Radix 完成门户与 ScrollArea 视口定位后再滚动当前行。
    let scrollFrame = 0;
    const layoutFrame = window.requestAnimationFrame(() => {
      scrollFrame = window.requestAnimationFrame(() => {
        selectedRowRef.current?.scrollIntoView({ block: "center" });
      });
    });
    return () => {
      window.cancelAnimationFrame(layoutFrame);
      if (scrollFrame) {
        window.cancelAnimationFrame(scrollFrame);
      }
    };
  }, []);

  return (
    <Command shouldFilter={false}>
      <CommandInput
        aria-label={props.filterPlaceholder}
        autoFocus
        onValueChange={setFilter}
        onKeyDown={(event) => {
          if (event.key === "Tab" || event.key === "Escape") {
            event.stopPropagation();
            props.onDismiss();
          }
        }}
        placeholder={props.filterPlaceholder}
        value={filter}
      />
      <CommandList className="vk-model-list">
        <div className="vk-model-rows">
          {filteredChoices.length === 0 ? (
            <div className="vk-model-none">{props.noMatchesLabel}</div>
          ) : (
            filteredGroups.map((group) => {
              // exactGroupChoice is present only when this grouped model is currently selected.
              // exactGroupChoice 仅在当前聚合模型已被选中时存在。
              const exactGroupChoice = props.selectedChoice
                && props.selectedChoice.providerId === group.providerId
                && props.selectedChoice.modelId === group.modelId
                ? props.selectedChoice
                : undefined;
              // resolvedGroupChoice makes an explicit model-row click adopt the current backend
              // account preference. Exact account intent belongs to the separate account picker.
              // resolvedGroupChoice 让显式模型行点击采用当前后端账号偏好；精确账号意图由独立账号选择器承载。
              const resolvedGroupChoice = resolveModelPickerGroupChoice(
                group,
                exactGroupChoice,
                true,
              );
              // selected marks the grouped model independently from its chosen account.
              // selected 独立于所选账号标记当前聚合模型。
              const selected = props.selectionScope === "model"
                ? modelPickerGroupKey(group) === props.value
                : Boolean(exactGroupChoice);
              return (
                <CommandItem
                  className="vk-model-row"
                  data-current={selected || undefined}
                  key={modelPickerGroupKey(group)}
                  value={modelPickerGroupKey(group)}
                  onSelect={() => props.onSelect(resolvedGroupChoice)}
                  ref={selected ? (element) => {
                    selectedRowRef.current = element;
                  } : undefined}
                >
                  <span className="vk-model-check">
                    {selected ? <Check size={15} /> : null}
                  </span>
                  <span className="vk-model-copy">
                    <span className="vk-model-title" title={group.modelName}>{group.modelName}</span>
                    <span className="vk-model-meta" title={group.providerName}>{group.providerName}</span>
                  </span>
                  <span className="vk-model-side">
                    <span className="vk-model-id" title={group.modelId}>{group.modelId}</span>
                    <ModelBuiltinCapabilityIcons
                      className="vk-model-caps"
                      inputResources={resolvedGroupChoice.inputModalities}
                      providerNativeTools={resolvedGroupChoice.providerNativeTools}
                    />
                  </span>
                </CommandItem>
              );
            })
          )}
        </div>
      </CommandList>
    </Command>
  );
}

// modelAccountProtocolDisplayName returns the user-facing label for one exact wire protocol.
// modelAccountProtocolDisplayName 返回一个精确 Wire 协议的用户可见名称。
//
// Parameters
// 参数
// `protocolId` is the backend-resolved effective protocol for one exact model route.
// `protocolId` 是后端为一条精确模型路由解析出的生效协议。
//
// Returns
// 返回值
// A stable known label or the original identifier when the protocol is newly introduced.
// 返回稳定的已知名称；协议为新增类型时返回原始标识。
export function modelAccountProtocolDisplayName(protocolId: string): string {
  const labels: Record<string, string> = {
    "anthropic-messages.v1": "Anthropic Messages",
    "anthropic-messages-video.v1": "Anthropic Messages + Video",
    "claude-code-messages.v1": "Claude Code Messages",
    "openai-chat-completions.v1": "OpenAI Chat Completions",
    "openai-codex-responses.v1": "OpenAI Codex Responses",
    "openai-responses.v1": "OpenAI Responses",
  };
  return labels[protocolId] ?? protocolId;
}

// modelAccountEffectiveProtocolId resolves one account card's backend-authoritative model route.
// modelAccountEffectiveProtocolId 解析账号卡片对应的后端权威模型路由。
//
// Parameters
// 参数
// `snapshot` owns backend route records and `choice` identifies one exact provider/account/model.
// `snapshot` 持有后端路由记录，`choice` 标识精确供应商、账号与模型。
//
// Returns
// 返回值
// The exact effective protocol, or null when the backend did not publish that route.
// 返回精确生效协议；后端未发布该路由时返回 null。
export function modelAccountEffectiveProtocolId(
  snapshot: Pick<PublicAiSettingsSnapshot, "modelRouteRecords"> | null | undefined,
  choice: Pick<ModelPickerChoice, "providerId" | "connectionId" | "modelId"> | null | undefined,
): string | null {
  if (!snapshot || !choice) {
    return null;
  }
  return snapshot.modelRouteRecords?.find((record) => (
    record.providerId === choice.providerId
    && record.connectionId === choice.connectionId
    && record.modelId === choice.modelId
    && record.executable
  ))?.protocolId ?? null;
}

// formatModelPickerTranslation replaces simple named placeholders in localized model picker templates.
// formatModelPickerTranslation 替换模型选择器本地化模板中的简单具名占位符。
function formatModelPickerTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}

// modelPickerChoicesFromProviders flattens service provider summaries into picker rows.
// modelPickerChoicesFromProviders 将 service Provider 摘要扁平化为筛选器行。
export function modelPickerChoicesFromProviders(providers: ProviderSummary[]): ModelPickerChoice[] {
  return providers.flatMap((provider) => {
    const models = provider.models ?? [];
    return models.map((model) => ({
      providerId: provider.sourceProviderId ?? provider.id,
      connectionId: provider.connectionId ?? "",
      providerName: provider.providerDisplayName ?? provider.name,
      accountName: provider.accountName ?? provider.name,
      modelId: model.id,
      modelName: model.name,
      inputModalities: [...(model.inputModalities ?? [])],
      providerNativeTools: [...(model.providerNativeTools ?? [])],
      active: Boolean(model.active),
      preferredConnection: Boolean(model.preferredConnection),
    }));
  }).sort((left, right) => (
    left.modelName.localeCompare(right.modelName)
    || left.providerName.localeCompare(right.providerName)
    || left.modelId.localeCompare(right.modelId)
    || left.accountName.localeCompare(right.accountName)
  ));
}

// modelPickerChoiceKey builds the stable provider/model value used by controls.
// modelPickerChoiceKey 构建控件使用的稳定 provider/model 取值。
export function modelPickerChoiceKey<
  Choice extends Pick<ModelPickerChoice, "providerId" | "modelId" | "connectionId">,
>(choice: Choice | undefined): string {
  return choice ? JSON.stringify([choice.providerId, choice.modelId, choice.connectionId]) : "";
}

// modelPickerGroupKey builds the stable provider/model key used by grouped rows.
// modelPickerGroupKey 构建聚合行使用的稳定 Provider/模型键。
// Parameters: group carries source-provider and model identity; returns the serialized pair used for stable grouped selection.
// 参数：group 携带来源供应商与模型身份；返回用于稳定分组选择的序列化二元组。
export function modelPickerGroupKey(group: Pick<ModelPickerModelGroup, "providerId" | "modelId">): string {
  return JSON.stringify([group.providerId, group.modelId]);
}

// groupModelPickerChoices collapses account-specific choices into one provider/model row.
// groupModelPickerChoices 将账号级选项折叠为单个 Provider/模型行。
export function groupModelPickerChoices(choices: ModelPickerChoice[]): ModelPickerModelGroup[] {
  // groups preserves the already sorted model order while collecting exact accounts.
  // groups 在收集精确账号时保留已经排序的模型顺序。
  const groups = new Map<string, ModelPickerModelGroup>();
  choices.forEach((choice) => {
    // key identifies one provider/model pair independently from its credential connection.
    // key 独立于凭据连接标识一个 Provider/模型组合。
    const key = JSON.stringify([choice.providerId, choice.modelId]);
    // existing is the previously created group for this exact provider model.
    // existing 是此前为该精确 Provider 模型创建的聚合项。
    const existing = groups.get(key);
    if (existing) {
      existing.accounts.push(choice);
      return;
    }
    groups.set(key, {
      providerId: choice.providerId,
      providerName: choice.providerName,
      modelId: choice.modelId,
      modelName: choice.modelName,
      accounts: [choice],
    });
  });
  return Array.from(groups.values());
}

// resolveModelPickerGroupChoice selects the account for one grouped model according to session scope.
// resolveModelPickerGroupChoice 根据会话作用域为一个聚合模型选择账号。
// Parameters: group owns the current accounts, selectedChoice is the session-exact route, and
// preferPreferredConnection makes a new session follow the backend model-account preference.
// 参数：group 持有当前账号，selectedChoice 是会话精确路由，preferPreferredConnection 让新会话
// 遵循后端模型账号偏好。
// Returns: one exact account route that can be submitted without changing account-picker semantics.
// 返回值：一条可直接提交且不改变账号选择器语义的精确账号路由。
export function resolveModelPickerGroupChoice(
  group: ModelPickerModelGroup,
  selectedChoice: ModelPickerChoice | undefined,
  preferPreferredConnection = false,
): ModelPickerChoice {
  // preferred is the backend authority recorded for this provider/model pair.
  // preferred 是后端为该供应商与模型组合记录的权威账号。
  const preferred = group.accounts.find((choice) => choice.preferredConnection);
  if (preferPreferredConnection && preferred) {
    return preferred;
  }
  if (
    selectedChoice
    && selectedChoice.providerId === group.providerId
    && selectedChoice.modelId === group.modelId
    && group.accounts.some((account) => account.connectionId === selectedChoice.connectionId)
  ) {
    return selectedChoice;
  }
  // resolved is the first explicit account matching the backend selection order.
  // resolved 是按照后端选择顺序命中的第一个明确账号。
  const resolved = group.accounts.find((choice) => choice.active)
    ?? preferred
    ?? group.accounts[0];
  if (!resolved) {
    throw new Error("Model picker groups must contain at least one account.");
  }
  return resolved;
}

// filterModelPickerChoices searches the exact model, provider and account display identities.
// filterModelPickerChoices 搜索精确模型、供应商及账号展示身份。
// Parameters: choices are credential-scoped models and filter is free text; returns matches across model, provider and account identifiers or names.
// 参数：choices 是凭据作用域模型，filter 是自由文本；返回模型、供应商及账号标识或名称匹配项。
export function filterModelPickerChoices(choices: ModelPickerChoice[], filter: string): ModelPickerChoice[] {
  const normalizedFilter = filter.trim().toLowerCase();
  if (!normalizedFilter) {
    return choices;
  }
  return choices.filter((choice) => (
    choice.modelName.toLowerCase().includes(normalizedFilter)
    || choice.modelId.toLowerCase().includes(normalizedFilter)
    || choice.providerName.toLowerCase().includes(normalizedFilter)
    || choice.providerId.toLowerCase().includes(normalizedFilter)
    || choice.accountName.toLowerCase().includes(normalizedFilter)
    || choice.connectionId.toLowerCase().includes(normalizedFilter)
  ));
}

