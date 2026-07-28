import * as React from "react";
import { Check, Cpu, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
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
  // Stable model identifier persisted with provider id.
  // 与 Provider 标识一起持久化的稳定模型标识。
  modelId: string;
  // Human-readable model label.
  // 人类可读的模型标签。
  modelName: string;
  // Whether this row is the active workbench model.
  // 该行是否为当前激活的工作台模型。
  active: boolean;
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
  const choices = React.useMemo(() => modelPickerChoicesFromProviders(props.providers), [props.providers]);
  const exactChoice = choices.find((choice) => modelPickerChoiceKey(choice) === props.value);
  const selectedChoice = props.value ? exactChoice : choices.find((choice) => choice.active) ?? choices[0];
  const selectedModelLabel = selectedChoice?.modelName ?? props.unavailableValueLabel ?? props.fallbackLabel;
  const selectedProviderLabel = selectedChoice?.providerName ?? (props.value ? props.value.split("/")[0] : "-");
  const filteredChoices = React.useMemo(
    () => filterModelPickerChoices(choices, filter),
    [choices, filter],
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
              filteredChoices.map((choice) => {
                const selected = modelPickerChoiceKey(choice) === modelPickerChoiceKey(selectedChoice);
                return (
                  <button
                    className={cn("model-picker-row", selected && "selected")}
                    key={modelPickerChoiceKey(choice)}
                    onClick={() => {
                      props.onValueChange(choice);
                      setOpen(false);
                    }}
                    type="button"
                  >
                    <span className="model-picker-selected-slot">
                      {selected ? <Check size={15} /> : null}
                    </span>
                    <span className="model-picker-row-copy">
                      <span className="model-picker-row-title">{choice.modelName}</span>
                      <span className="model-picker-row-meta">{choice.providerName}</span>
                    </span>
                    <span className="model-picker-row-id">{choice.modelId}</span>
                  </button>
                );
              })
            )}
          </div>
        </ScrollArea>
      </DropdownMenuContent>
    </DropdownMenu>
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
      providerName: provider.name,
      modelId: model.id,
      modelName: model.name,
      active: Boolean(model.active),
    }));
  }).sort((left, right) => (
    left.modelName.localeCompare(right.modelName)
    || left.providerName.localeCompare(right.providerName)
    || left.modelId.localeCompare(right.modelId)
  ));
}

// modelPickerChoiceKey builds the stable provider/model value used by controls.
// modelPickerChoiceKey 构建控件使用的稳定 provider/model 取值。
export function modelPickerChoiceKey(choice: ModelPickerChoice | undefined): string {
  return choice ? JSON.stringify([choice.providerId, choice.modelId, choice.connectionId]) : "";
}

// filterModelPickerChoices applies the WebUI-style free-text model filter.
// filterModelPickerChoices 应用与 WebUI 一致的自由文本模型过滤。
function filterModelPickerChoices(choices: ModelPickerChoice[], filter: string): ModelPickerChoice[] {
  const normalizedFilter = filter.trim().toLowerCase();
  if (!normalizedFilter) {
    return choices;
  }
  return choices.filter((choice) => (
    choice.modelName.toLowerCase().includes(normalizedFilter)
    || choice.modelId.toLowerCase().includes(normalizedFilter)
    || choice.providerName.toLowerCase().includes(normalizedFilter)
    || choice.providerId.toLowerCase().includes(normalizedFilter)
  ));
}
