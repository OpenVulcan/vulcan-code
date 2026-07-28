import * as React from "react";
import { Check, Cpu, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ProviderSummary } from "@/types";

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

// ModelPickerControl renders the shared workbench model filter trigger and list.
// ModelPickerControl 渲染共享的工作台模型筛选触发器与列表。
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
  // Current selected model key in provider/model form.
  // 当前选中的 provider/model 形式模型键。
  value: string;
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
  const [filter, setFilter] = React.useState("");
  // selectedRowRef anchors the exact active model row after the dropdown portal finishes layout.
  // selectedRowRef 在下拉门户完成布局后锚定精确的当前模型行。
  const selectedRowRef = React.useRef<HTMLElement | null>(null);
  const choices = React.useMemo(() => modelPickerChoicesFromProviders(props.providers), [props.providers]);
  const exactChoice = choices.find((choice) => modelPickerChoiceKey(choice) === props.value);
  // selectedChoice degrades from an obsolete exact account to the backend-resolved active row.
  // selectedChoice 从已失效的精确账号降级到后端解析出的当前行。
  const selectedChoice = exactChoice ?? choices.find((choice) => choice.active) ?? choices[0];
  const selectedModelLabel = selectedChoice?.modelName ?? props.unavailableValueLabel ?? props.fallbackLabel;
  const selectedProviderLabel = selectedChoice?.providerName ?? (props.value ? props.value.split("/")[0] : "-");
  // selectedAccountLabel is the exact credential label owned by the selected connection.
  // selectedAccountLabel 是所选连接拥有的精确凭据标签。
  const selectedAccountLabel = selectedChoice?.accountName ?? "-";
  const filteredChoices = React.useMemo(
    () => filterModelPickerChoices(choices, filter),
    [choices, filter],
  );
  // filteredGroups collapses credential duplicates only for the composer session picker.
  // filteredGroups 仅为会话输入区模型选择器折叠凭据重复项。
  const filteredGroups = React.useMemo(
    () => groupModelPickerChoices(filteredChoices),
    [filteredChoices],
  );

  React.useEffect(() => {
    if (!open) {
      setFilter("");
    }
  }, [open]);

  React.useEffect(() => {
    if (props.openSignal === undefined || props.openSignal <= 0) {
      return;
    }
    setOpen(true);
  }, [props.openSignal]);

  React.useEffect(() => {
    if (!open) {
      return;
    }
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
  }, [open]);

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button
              aria-label={props.ariaLabel}
              className="composer-select-trigger model"
              disabled={props.disabled || choices.length === 0}
              type="button"
              variant="outline"
            >
              <span className="composer-select-icon"><Cpu size={15} /></span>
              <span className="composer-select-copy">
                <span className="composer-select-label">{selectedModelLabel}</span>
              </span>
            </Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent className="composer-control-tooltip" side="top">
          <strong>{t("composer.modelPicker.tooltipTitle")}</strong>
          <span>{formatModelPickerTranslation(t, "composer.modelPicker.currentModel", { model: selectedModelLabel })}</span>
          <span>{formatModelPickerTranslation(t, "composer.modelPicker.currentProvider", { provider: selectedProviderLabel || "-" })}</span>
          <span>{formatModelPickerTranslation(t, "composer.modelPicker.currentAccount", { account: selectedAccountLabel })}</span>
        </TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start" className="model-picker-content" sideOffset={8}>
        <div className="model-picker-filter">
          <Search size={14} />
          <Input
            aria-label={props.filterPlaceholder}
            autoFocus
            onChange={(event) => setFilter(event.currentTarget.value)}
            placeholder={props.filterPlaceholder}
            value={filter}
          />
        </div>
        <ScrollArea className="model-picker-scroll">
          <div className="model-picker-list">
            {filteredChoices.length === 0 ? (
              <div className="model-picker-empty">{props.noMatchesLabel}</div>
            ) : (
              filteredGroups.map((group) => {
                // exactGroupChoice is present only when this grouped model is currently selected.
                // exactGroupChoice 仅在当前聚合模型已被选中时存在。
                const exactGroupChoice = selectedChoice
                  && selectedChoice.providerId === group.providerId
                  && selectedChoice.modelId === group.modelId
                  ? selectedChoice
                  : undefined;
                // resolvedGroupChoice follows active, backend-preferred, then stable first-account order.
                // resolvedGroupChoice 依次遵循当前激活、后端首选与稳定首账号顺序。
                const resolvedGroupChoice = resolveModelPickerGroupChoice(group, exactGroupChoice);
                // selected marks the grouped model independently from its chosen account.
                // selected 独立于所选账号标记当前聚合模型。
                const selected = Boolean(exactGroupChoice);
                return (
                  <div
                    className={cn(
                      "model-picker-group-row",
                      selected && "selected",
                    )}
                    key={modelPickerGroupKey(group)}
                    ref={selected ? (element) => {
                      selectedRowRef.current = element;
                    } : undefined}
                  >
                    <Button
                      className="model-picker-group-main"
                      onClick={() => {
                        props.onValueChange(resolvedGroupChoice);
                        setOpen(false);
                      }}
                      type="button"
                      variant="ghost"
                    >
                      <span className="model-picker-selected-slot">
                        {selected ? <Check size={15} /> : null}
                      </span>
                      <span className="model-picker-row-copy">
                        <span className="model-picker-row-title">{group.modelName}</span>
                        <span className="model-picker-row-meta">{group.providerName}</span>
                      </span>
                    </Button>
                    <span className="model-picker-row-id">{group.modelId}</span>
                  </div>
                );
              })
            )}
          </div>
        </ScrollArea>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// ModelAccountPickerControl renders a separate credential selector for the active model.
// ModelAccountPickerControl 为当前模型渲染独立的凭据选择器。
// Parameters: ariaLabel describes the control, accounts contains exact credential choices,
// value identifies the selected connection, and onValueChange persists the chosen account.
// 参数：ariaLabel 描述控件，accounts 包含精确凭据选项，value 标识当前连接，
// onValueChange 持久化选中的账号。
// Returns: a compact account selector, or null when the model has no alternative account.
// 返回值：紧凑账号选择器；模型没有备选账号时返回 null。
export function ModelAccountPickerControl(props: {
  // Accessible label for the credential selector.
  // 凭据选择器的无障碍标签。
  ariaLabel: string;
  // Exact credential choices available to the active provider model.
  // 当前 Provider 模型可用的精确凭据选项。
  accounts: ModelPickerChoice[];
  // Current exact credential connection identifier.
  // 当前精确凭据连接标识。
  value: string;
  // Persists one exact credential choice.
  // 持久化一个精确凭据选项。
  onValueChange: (choice: ModelPickerChoice) => void;
  // Whether the host flow temporarily disables account switching.
  // 宿主流程是否临时禁用账号切换。
  disabled?: boolean;
}) {
  if (props.accounts.length <= 1) {
    return null;
  }
  return (
    <Select
      disabled={props.disabled}
      onValueChange={(connectionId) => {
        // account is the exact model account selected by its stable connection id.
        // account 是通过稳定连接标识选中的精确模型账号。
        const account = props.accounts.find((choice) => choice.connectionId === connectionId);
        if (account) {
          props.onValueChange(account);
        }
      }}
      value={props.value}
    >
      <SelectTrigger
        aria-label={props.ariaLabel}
        className="composer-select-trigger model-account"
        size="sm"
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent align="start" className="model-picker-account-content" position="popper">
        {props.accounts.map((account) => (
          <SelectItem key={account.connectionId} value={account.connectionId}>
            {account.accountName}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
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
export function modelPickerGroupKey(group: ModelPickerModelGroup): string {
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

// resolveModelPickerGroupChoice selects the exact, active, preferred, or stable first account.
// resolveModelPickerGroupChoice 依次选择精确、激活、首选或稳定首账号。
export function resolveModelPickerGroupChoice(
  group: ModelPickerModelGroup,
  selectedChoice: ModelPickerChoice | undefined,
): ModelPickerChoice {
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
    ?? group.accounts.find((choice) => choice.preferredConnection)
    ?? group.accounts[0];
  if (!resolved) {
    throw new Error("Model picker groups must contain at least one account.");
  }
  return resolved;
}

// filterModelPickerChoices applies a model-name and model-id-only free-text filter.
// filterModelPickerChoices 仅按模型名称和模型 ID 应用自由文本过滤。
export function filterModelPickerChoices(choices: ModelPickerChoice[], filter: string): ModelPickerChoice[] {
  const normalizedFilter = filter.trim().toLowerCase();
  if (!normalizedFilter) {
    return choices;
  }
  return choices.filter((choice) => (
    choice.modelName.toLowerCase().includes(normalizedFilter)
    || choice.modelId.toLowerCase().includes(normalizedFilter)
  ));
}
