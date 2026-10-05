import * as React from "react";
import { ArrowLeft, Check, ChevronDown, ChevronRight, Cpu, RotateCcw, UserRound } from "lucide-react";
import {
  ModelAccountUsageCard,
  ModelAccountUsageSummary,
  useModelAccountDetails,
  type ModelAccountDetails,
} from "@/components/composer/model-account-details";
import {
  ModelPickerList,
  modelAccountProtocolDisplayName,
  modelPickerChoicesFromProviders,
  resolveModelPickerDisplayedChoice,
  type ModelPickerChoice,
} from "@/components/composer/model-picker-control";
import type { ComposerSelectOption } from "@/components/composer/composer-model";
import { CredentialUsageOverview } from "@/components/public-ai/credential-usage-overview";
import { ProviderIcon } from "@/components/features/settings/provider-icon";
import { localizedProviderName } from "@/components/features/settings/provider-presentation";
import { Button } from "@/components/ui/button";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/components/ui/hover-card";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ProviderSummary } from "@/types";

// REASONING_LEVEL_LABEL_KEYS localizes the backend wire values; unknown values keep the backend label.
// REASONING_LEVEL_LABEL_KEYS 本地化后端传输值；未知值保留后端标签。
const REASONING_LEVEL_LABEL_KEYS: Readonly<Record<string, TranslationKey>> = {
  "fixed-off": "composer.reasoning.level.none",
  "fixed-on": "composer.reasoning.level.on",
  none: "composer.reasoning.level.none",
  off: "composer.reasoning.level.default",
  adaptive: "composer.reasoning.level.adaptive",
  minimal: "composer.reasoning.level.minimal",
  low: "composer.reasoning.level.low",
  medium: "composer.reasoning.level.medium",
  high: "composer.reasoning.level.high",
  "x-high": "composer.reasoning.level.xHigh",
  max: "composer.reasoning.level.max",
  ultra: "composer.reasoning.level.ultra",
};

// FixedReasoningTone classifies a model that exposes no adjustable scale.
// FixedReasoningTone 对不提供可调档位的模型进行分类。
export type FixedReasoningTone = "off" | "on" | "adaptive" | "default" | "level" | "unavailable";

// FIXED_REASONING_HINT_KEYS explains each fixed state in one sentence.
// FIXED_REASONING_HINT_KEYS 用一句话说明每种固定状态。
const FIXED_REASONING_HINT_KEYS: Readonly<Record<FixedReasoningTone, TranslationKey>> = {
  off: "composer.reasoning.hint.off",
  on: "composer.reasoning.hint.on",
  adaptive: "composer.reasoning.hint.adaptive",
  default: "composer.reasoning.hint.default",
  level: "composer.reasoning.hint.level",
  unavailable: "composer.reasoning.hint.unavailable",
};

// ReasoningSelectionView is the presentation derived from the active model's reasoning options.
// ReasoningSelectionView 是根据当前模型思考选项推导出的展示形态。
export type ReasoningSelectionView =
  | {
    // Two or more options form a left-to-right scale in backend canonical order.
    // 两个及以上选项按后端规范顺序构成从左到右的档位刻度。
    kind: "scale";
    // Localized label of the current value, or null when the value is outside the scale.
    // 当前值的本地化标签；当前值不在刻度内时为 null。
    label: string | null;
    // Index of the current value within the options, or -1 when absent.
    // 当前值在选项中的下标；不存在时为 -1。
    index: number;
  }
  | {
    // Zero or one option: nothing to adjust, only a state to explain.
    // 零或一个选项：无可调整内容，只需说明状态。
    kind: "fixed";
    // Fixed state classification used for the explanatory hint.
    // 用于说明文字的固定状态分类。
    tone: FixedReasoningTone;
    // Localized label of the single option, or null when the model exposes none.
    // 唯一选项的本地化标签；模型未提供选项时为 null。
    label: string | null;
  };

// reasoningLevelLabel localizes one reasoning wire value with the backend label as fallback.
// reasoningLevelLabel 本地化一个思考传输值，并以后端标签作为回退。
// Parameters: value is the wire value, fallback the backend label and t the active translator.
// 参数：value 是传输值，fallback 是后端标签，t 是当前翻译函数。
// Returns: the localized label.
// 返回值：本地化后的标签。
export function reasoningLevelLabel(value: string, fallback: string, t: (key: TranslationKey) => string): string {
  const key = REASONING_LEVEL_LABEL_KEYS[value];
  return key ? t(key) : fallback;
}

// describeReasoningSelection classifies options into an adjustable scale or one fixed state.
// describeReasoningSelection 将选项归类为可调刻度或单一固定状态。
// Parameters: options are the backend-declared levels in canonical order, value is the current effort.
// 参数：options 为后端按规范顺序声明的档位，value 为当前档位。
// Returns: the view the picker renders for this model.
// 返回值：选择器为该模型渲染的视图。
export function describeReasoningSelection(
  options: ComposerSelectOption[],
  value: string,
  t: (key: TranslationKey) => string,
): ReasoningSelectionView {
  if (options.length === 0) {
    return { kind: "fixed", tone: "unavailable", label: null };
  }
  if (options.length === 1) {
    const only = options[0];
    const tone: FixedReasoningTone = only.value === "fixed-off" || only.value === "none"
      ? "off"
      : only.value === "fixed-on"
        ? "on"
        : only.value === "adaptive"
          ? "adaptive"
          : only.value === "off"
            ? "default"
            : "level";
    return { kind: "fixed", tone, label: reasoningLevelLabel(only.value, only.label, t) };
  }
  const index = options.findIndex((option) => option.value === value);
  const current = options[index];
  return {
    kind: "scale",
    index,
    label: current
      ? reasoningLevelLabel(current.value, current.label, t)
      : value && REASONING_LEVEL_LABEL_KEYS[value] ? t(REASONING_LEVEL_LABEL_KEYS[value]) : null,
  };
}

// PickerView names the panes of the merged popover; "reasoning" is the home pane.
// PickerView 命名合并浮层的各个面板；"reasoning" 为首页面板。
type PickerView = "reasoning" | "models" | "account";

// ModelReasoningPickerControl merges model, account and reasoning level into one composer control.
// ModelReasoningPickerControl 将模型、账号与思考层级合并为一个输入区控件。
// Parameters: props supplies the model catalog, model accounts, reasoning options and persistence callbacks.
// 参数：props 提供模型目录、模型账号、思考选项与持久化回调。
// Returns: a trigger reading "model · level" whose popover opens on the reasoning pane; the model row
// switches to the searchable model list (choosing a model returns), and the account row opens the
// account pane with usage and account switching.
// 返回值：显示“模型 · 层级”的触发器，浮层默认打开思考面板；模型行切换到可搜索模型列表（选定后返回），
// 账号行打开包含用量与账号切换的账号面板。
export function ModelReasoningPickerControl(props: {
  // Accessible label applied to the trigger button.
  // 应用于触发按钮的无障碍标签。
  ariaLabel: string;
  // Fallback label when no model is selected.
  // 未选择模型时的回退标签。
  fallbackLabel: string;
  // Placeholder shown in the model filter input.
  // 模型过滤输入框的占位文案。
  filterPlaceholder: string;
  // Empty-state label when the filter yields no rows.
  // 过滤无匹配时的空状态文案。
  noMatchesLabel: string;
  // Providers carrying enabled model rows from the shared service API.
  // 来自共享 service API、携带已启用模型行的 Provider 列表。
  providers: ProviderSummary[];
  // Exact provider/model/connection key of the current model.
  // 当前模型的精确 Provider/模型/连接键。
  value: string;
  // Persists one selected model row.
  // 持久化一个选中的模型行。
  onModelChange: (choice: ModelPickerChoice) => void;
  // Accessible label for the account row.
  // 账号行的无障碍标签。
  accountAriaLabel: string;
  // Exact credential choices available to the active model; empty hides the account row.
  // 当前模型可用的精确凭据选项；为空时隐藏账号行。
  accounts: ModelPickerChoice[];
  // Current exact credential connection identifier.
  // 当前精确凭据连接标识。
  accountValue: string;
  // Persists one exact credential choice.
  // 持久化一个精确凭据选项。
  onAccountChange: (choice: ModelPickerChoice) => void;
  // Reasoning levels declared by the active model, in backend canonical order.
  // 当前模型声明的思考档位，按后端规范顺序排列。
  reasoningOptions: ComposerSelectOption[];
  // Current reasoning effort value.
  // 当前思考档位值。
  reasoningEffort: string;
  // Level the backend applies with no stored choice; null when the service does not report it.
  // 未保存选择时后端采用的档位；服务未上报时为 null。
  defaultReasoningEffort: string | null;
  // Persists one reasoning effort.
  // 持久化一个思考档位。
  onReasoningEffortChange: (value: string) => void;
  // Slash-command signal that opens directly on the model list.
  // 直接打开模型列表的 slash 指令信号。
  modelOpenSignal?: number;
  // Slash-command signal that opens on the reasoning pane.
  // 打开思考面板的 slash 指令信号。
  reasoningOpenSignal?: number;
  // Whether the host flow temporarily disables the control.
  // 宿主流程是否临时禁用该控件。
  disabled?: boolean;
}) {
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  const [view, setView] = React.useState<PickerView>("reasoning");
  // returnToReasoningRef records whether the model list was entered from the reasoning pane, so Escape steps back.
  // returnToReasoningRef 记录模型列表是否从思考面板进入，以便 Escape 返回上一层。
  const returnToReasoningRef = React.useRef(false);
  // observed*Signal consume each explicit slash command once, including commands arriving while unavailable.
  // observed*Signal 每条显式 slash 指令只消费一次，包括不可用期间到达的指令。
  const observedModelSignal = React.useRef(props.modelOpenSignal);
  const observedReasoningSignal = React.useRef(props.reasoningOpenSignal);
  // focusTargetRef receives focus whenever the reasoning pane is shown.
  // focusTargetRef 在思考面板显示时获得焦点。
  const focusTargetRef = React.useRef<HTMLElement | null>(null);
  const choices = React.useMemo(() => (
    modelPickerChoicesFromProviders(props.providers).map((choice) => ({
      ...choice,
      providerName: localizedProviderName(choice.providerId, choice.providerName, t),
    }))
  ), [props.providers, t]);
  const selectedChoice = resolveModelPickerDisplayedChoice(choices, props.value);
  const modelLabel = selectedChoice?.modelName ?? props.fallbackLabel;
  const reasoning = describeReasoningSelection(props.reasoningOptions, props.reasoningEffort, t);
  // unavailable closes the popover when there is neither a model to switch nor a level to adjust.
  // unavailable 在既无模型可切换、也无档位可调整时关闭浮层。
  const unavailable = Boolean(props.disabled) || (choices.length === 0 && props.reasoningOptions.length === 0);
  // triggerLevel is shown only when the level is adjustable; fixed states are explained inside the popover.
  // triggerLevel 仅在档位可调时显示；固定状态在浮层内说明。
  const triggerLevel = reasoning.kind === "scale" ? reasoning.label : null;
  // account resolves the active model account; its memory snapshot is read whenever the popover opens.
  // account 解析当前模型账号；浮层每次打开时读取其内存快照。
  const account = useModelAccountDetails(props.accounts, props.accountValue);
  const loadAccount = account.load;
  // accountMissing flags a model whose stored credential no longer exists, so the trigger asks for a choice.
  // accountMissing 标记已保存凭据不存在的模型，使触发器提示重新选择。
  const accountMissing = props.accounts.length > 0 && !account.selectedAccount;
  // backButtonRef receives focus when the account pane opens.
  // backButtonRef 在账号面板打开时获得焦点。
  const backButtonRef = React.useRef<HTMLButtonElement | null>(null);
  const popoverOpen = open && !unavailable;
  // summaryOpen shows the read-only summary card on hover; it never shows while the popover is open.
  // summaryOpen 在悬停时显示只读摘要卡片；浮层打开期间从不显示。
  const [summaryOpen, setSummaryOpen] = React.useState(false);
  // summarySuppressedRef blocks the summary card after the popover closes and focus returns to the trigger,
  // until the pointer leaves the trigger.
  // summarySuppressedRef 在浮层关闭、焦点回到触发器后阻止摘要卡片弹出，直到指针离开触发器。
  const summarySuppressedRef = React.useRef(false);

  React.useEffect(() => {
    const modelChanged = observedModelSignal.current !== props.modelOpenSignal;
    const reasoningChanged = observedReasoningSignal.current !== props.reasoningOpenSignal;
    observedModelSignal.current = props.modelOpenSignal;
    observedReasoningSignal.current = props.reasoningOpenSignal;
    if (unavailable) {
      setOpen(false);
      return;
    }
    if (modelChanged && props.modelOpenSignal !== undefined && props.modelOpenSignal > 0 && choices.length > 0) {
      returnToReasoningRef.current = false;
      setView("models");
      setOpen(true);
    } else if (reasoningChanged && props.reasoningOpenSignal !== undefined && props.reasoningOpenSignal > 0) {
      setView("reasoning");
      setOpen(true);
    }
  }, [props.modelOpenSignal, props.reasoningOpenSignal, unavailable, choices.length]);

  React.useEffect(() => {
    if (!open) {
      return;
    }
    if (view === "reasoning") {
      focusTargetRef.current?.focus({ preventScroll: true });
    } else if (view === "account") {
      backButtonRef.current?.focus({ preventScroll: true });
    }
  }, [open, view]);

  React.useEffect(() => {
    if (popoverOpen && props.accounts.length > 0) {
      void loadAccount();
    }
  }, [popoverOpen, props.accountValue, props.accounts.length, loadAccount]);

  // openPane enters a secondary pane from the reasoning pane, so leaving it steps back.
  // openPane 从思考面板进入次级面板，离开时返回上一层。
  const openPane = (next: Exclude<PickerView, "reasoning">) => {
    returnToReasoningRef.current = true;
    setView(next);
  };
  // accountSubtitle condenses plan and balance, falling back to protocol or provider.
  // accountSubtitle 压缩显示套餐与余额，回退为协议或供应商。
  const accountSubtitle = account.connection && account.usageOverviewVisible ? (
    <ModelAccountUsageSummary
      connection={account.connection}
      credentialProfile={account.credentialProfile}
      usage={account.usage}
    />
  ) : account.selectedAccount
    ? account.effectiveProtocolId
      ? modelAccountProtocolDisplayName(account.effectiveProtocolId)
      : account.selectedAccount.providerName
    : t("composer.modelPicker.selectAccount").replace("{model}", modelLabel);

  // leaveModelList steps back to the reasoning pane when it was the origin, otherwise closes the popover.
  // leaveModelList 若来源是思考面板则返回该面板，否则关闭浮层。
  const leaveModelList = () => {
    if (returnToReasoningRef.current) {
      setView("reasoning");
    } else {
      setOpen(false);
    }
  };

  return (
    <Popover
      open={popoverOpen}
      onOpenChange={(nextOpen) => {
        if (nextOpen) {
          setView("reasoning");
          setSummaryOpen(false);
        } else {
          summarySuppressedRef.current = true;
        }
        setOpen(nextOpen);
      }}
    >
      <HoverCard
        closeDelay={120}
        onOpenChange={(nextOpen) => {
          if (nextOpen && (popoverOpen || summarySuppressedRef.current)) {
            return;
          }
          setSummaryOpen(nextOpen);
          if (nextOpen && props.accounts.length > 0) {
            void loadAccount();
          }
        }}
        open={summaryOpen && !popoverOpen}
        openDelay={450}
      >
        <HoverCardTrigger asChild>
          <PopoverTrigger asChild>
            <Button
              aria-label={props.ariaLabel}
              className="composer-select-trigger model-reasoning"
              disabled={unavailable}
              onPointerLeave={() => { summarySuppressedRef.current = false; }}
              type="button"
              variant="outline"
            >
              <span className="composer-select-icon"><Cpu size={15} /></span>
              <span className="composer-model-reasoning-model">{modelLabel}</span>
              {triggerLevel ? <span className="composer-model-reasoning-level">{triggerLevel}</span> : null}
              {accountMissing ? <UserRound aria-hidden="true" className="composer-model-reasoning-alert" size={13} /> : null}
              <ChevronDown aria-hidden="true" className="composer-select-chevron" />
            </Button>
          </PopoverTrigger>
        </HoverCardTrigger>
        <HoverCardContent align="end" className="model-reasoning-summary" collisionPadding={12} side="top" sideOffset={8} width="content">
          <ModelReasoningSummary
            account={account}
            accountMissing={accountMissing}
            hasAccounts={props.accounts.length > 0}
            modelLabel={modelLabel}
            options={props.reasoningOptions}
            reasoning={reasoning}
            selectedChoice={selectedChoice}
          />
        </HoverCardContent>
      </HoverCard>
      <PopoverContent
        align="end"
        className={cn("model-reasoning-content", view === "models" ? "model-picker-content models" : view)}
        collisionPadding={12}
        onEscapeKeyDown={(event) => {
          // Escape on the account pane steps back to the reasoning pane instead of closing.
          // 在账号面板按 Escape 时返回思考面板，而不是关闭浮层。
          if (view === "account") {
            event.preventDefault();
            setView("reasoning");
          }
        }}
        onOpenAutoFocus={(event) => {
          // The reasoning pane focuses its own primary control; the model list autofocuses its filter.
          // 思考面板自行聚焦主要控件；模型列表自动聚焦过滤框。
          if (view === "reasoning") {
            event.preventDefault();
            focusTargetRef.current?.focus({ preventScroll: true });
          }
        }}
        sideOffset={8}
      >
        {view === "models" ? (
          <div className="model-reasoning-models vcn-stream-reveal">
            {returnToReasoningRef.current ? (
              <button
                className="model-reasoning-back"
                onClick={() => setView("reasoning")}
                type="button"
              >
                <ArrowLeft aria-hidden="true" size={14} />
                <span>{t("composer.modelReasoning.back")}</span>
              </button>
            ) : null}
            <ModelPickerList
              choices={choices}
              filterPlaceholder={props.filterPlaceholder}
              noMatchesLabel={props.noMatchesLabel}
              onDismiss={leaveModelList}
              onSelect={(choice) => {
                props.onModelChange(choice);
                setView("reasoning");
              }}
              selectedChoice={selectedChoice}
              value={props.value}
            />
          </div>
        ) : view === "account" ? (
          <div className="model-reasoning-account vcn-stream-reveal">
            <button
              className="model-reasoning-back"
              onClick={() => setView("reasoning")}
              ref={backButtonRef}
              type="button"
            >
              <ArrowLeft aria-hidden="true" size={14} />
              <span>{t("composer.modelReasoning.back")}</span>
            </button>
            <ModelAccountUsageCard details={account} />
            {props.accounts.length > 1 || !account.selectedAccount ? (
              <div className="model-reasoning-account-list" role="group" aria-label={t("composer.modelReasoning.switchAccount")}>
                <span className="model-reasoning-eyebrow">{t("composer.modelReasoning.switchAccount")}</span>
                {props.accounts.map((choice) => {
                  const selected = choice.connectionId === props.accountValue;
                  return (
                    <button
                      aria-pressed={selected}
                      className={cn("model-reasoning-account-option", selected && "selected")}
                      key={choice.connectionId}
                      onClick={() => {
                        if (!selected) {
                          props.onAccountChange(choice);
                        }
                      }}
                      type="button"
                    >
                      <UserRound aria-hidden="true" size={14} />
                      <span>{choice.accountName}</span>
                      {selected ? <Check aria-hidden="true" size={14} /> : null}
                    </button>
                  );
                })}
              </div>
            ) : null}
          </div>
        ) : (
          <div className="model-reasoning-pane vcn-stream-reveal">
            <button
              aria-label={t("composer.modelReasoning.switchModel").replace("{model}", modelLabel)}
              className="model-reasoning-model-row"
              disabled={choices.length === 0}
              onClick={() => openPane("models")}
              ref={reasoning.kind === "scale" ? undefined : (element) => { focusTargetRef.current = element; }}
              type="button"
            >
              <span aria-hidden="true" className="model-reasoning-model-icon">
                {selectedChoice ? <ProviderIcon className="size-6 shrink-0" providerId={selectedChoice.providerId} /> : <Cpu size={15} />}
              </span>
              <span className="model-reasoning-model-copy">
                <strong>{modelLabel}</strong>
                {selectedChoice ? <small>{selectedChoice.providerName}</small> : null}
              </span>
              <ChevronRight aria-hidden="true" className="model-reasoning-model-chevron" size={16} />
            </button>
            {props.accounts.length > 0 ? (
              <HoverCard closeDelay={150} openDelay={250}>
                <HoverCardTrigger asChild>
                  <button
                    aria-label={props.accountAriaLabel}
                    className={cn("model-reasoning-model-row account", accountMissing && "attention")}
                    onClick={() => openPane("account")}
                    type="button"
                  >
                    <span aria-hidden="true" className="model-reasoning-model-icon"><UserRound size={15} /></span>
                    <span className="model-reasoning-model-copy">
                      <strong>{account.selectedAccount?.accountName ?? t("composer.aria.account")}</strong>
                      <small>{accountSubtitle}</small>
                    </span>
                    <ChevronRight aria-hidden="true" className="model-reasoning-model-chevron" size={16} />
                  </button>
                </HoverCardTrigger>
                {account.selectedAccount ? (
                  <HoverCardContent align="start" className="model-reasoning-usage-hover" collisionPadding={12} side="left" sideOffset={10} width="content">
                    <ModelAccountUsageCard details={account} />
                  </HoverCardContent>
                ) : null}
              </HoverCard>
            ) : null}
            {reasoning.kind === "scale" ? (
              <ReasoningScale
                defaultValue={props.defaultReasoningEffort}
                focusRef={focusTargetRef}
                onChange={props.onReasoningEffortChange}
                options={props.reasoningOptions}
                value={props.reasoningEffort}
              />
            ) : (
              <div className={cn("model-reasoning-fixed", reasoning.tone)}>
                <div className="model-reasoning-heading">
                  <span className="model-reasoning-eyebrow">{t("composer.reasoning.tooltip")}</span>
                  <strong className="model-reasoning-title">
                    {reasoning.label ?? t("composer.reasoning.level.unavailable")}
                  </strong>
                </div>
                <p className="model-reasoning-hint">{t(FIXED_REASONING_HINT_KEYS[reasoning.tone])}</p>
              </div>
            )}
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}

// ReasoningScale renders the adjustable levels as a stepped left-to-right slider with a default reset.
// ReasoningScale 以从左到右的分段滑杆呈现可调档位，并提供恢复默认。
// Parameters: options are ordered levels, value the current effort, defaultValue the backend default,
// focusRef receives the slider element and onChange persists a committed level.
// 参数：options 为有序档位，value 为当前档位，defaultValue 为后端默认值，focusRef 接收滑杆元素，
// onChange 持久化提交的档位。
// Returns: the heading, slider, stop labels and reset action. Dragging previews levels and commits on release.
// 返回值：标题、滑杆、刻度标签与恢复操作；拖动时预览档位，松开后提交。
function ReasoningScale(props: {
  options: ComposerSelectOption[];
  value: string;
  defaultValue: string | null;
  focusRef: React.MutableRefObject<HTMLElement | null>;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const railRef = React.useRef<HTMLDivElement | null>(null);
  // previewIndex follows the pointer during a drag; null means the committed value is shown.
  // previewIndex 在拖动期间跟随指针；null 表示显示已提交值。
  const [previewIndex, setPreviewIndex] = React.useState<number | null>(null);
  const lastIndex = props.options.length - 1;
  const currentIndex = props.options.findIndex((option) => option.value === props.value);
  const shownIndex = previewIndex ?? currentIndex;
  const shownOption = props.options[shownIndex];
  const defaultIndex = props.defaultValue
    ? props.options.findIndex((option) => option.value === props.defaultValue)
    : -1;
  const defaultOption = props.options[defaultIndex];
  const labelOf = (option: ComposerSelectOption) => reasoningLevelLabel(option.value, option.label, t);
  const title = shownOption
    ? labelOf(shownOption)
    : props.value && REASONING_LEVEL_LABEL_KEYS[props.value]
      ? t(REASONING_LEVEL_LABEL_KEYS[props.value])
      : t("composer.reasoning.level.unavailable");
  // position converts a stop index into its percentage along the rail.
  // position 将刻度下标换算为沿滑轨的百分比。
  const position = (index: number) => (lastIndex > 0 ? (index / lastIndex) * 100 : 50);
  const commit = (index: number) => {
    const option = props.options[Math.min(Math.max(index, 0), lastIndex)];
    if (option && option.value !== props.value) {
      props.onChange(option.value);
    }
  };
  // indexAt snaps a pointer x-coordinate to the nearest stop.
  // indexAt 将指针横坐标吸附到最近的刻度。
  const indexAt = (clientX: number) => {
    const rect = railRef.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) {
      return Math.max(currentIndex, 0);
    }
    const ratio = Math.min(Math.max((clientX - rect.left) / rect.width, 0), 1);
    return Math.round(ratio * lastIndex);
  };
  const fillPercent = shownIndex >= 0 ? position(shownIndex) : 0;
  const restoreLabel = defaultOption
    ? t("composer.reasoning.restoreDefault").replace("{level}", labelOf(defaultOption))
    : t("composer.reasoning.restoreDefault.unknown");

  return (
    <div className="model-reasoning-scale-block">
      <div className="model-reasoning-heading-row">
        <div className="model-reasoning-heading">
          <span className="model-reasoning-eyebrow">{t("composer.reasoning.tooltip")}</span>
          <strong className="model-reasoning-title" aria-live="polite">
            {title}
            {shownIndex >= 0 && shownIndex === defaultIndex ? (
              <span className="model-reasoning-default-badge">{t("composer.reasoning.level.default")}</span>
            ) : null}
          </strong>
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              aria-label={restoreLabel}
              className="model-reasoning-reset"
              disabled={!defaultOption || defaultIndex === currentIndex}
              onClick={() => commit(defaultIndex)}
              size="icon"
              type="button"
              variant="ghost"
            >
              <RotateCcw aria-hidden="true" size={14} />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">{restoreLabel}</TooltipContent>
        </Tooltip>
      </div>
      <div
        aria-label={t("composer.reasoning.tooltip")}
        aria-valuemax={lastIndex}
        aria-valuemin={0}
        aria-valuenow={Math.max(shownIndex, 0)}
        aria-valuetext={title}
        className={cn("model-reasoning-slider", previewIndex !== null && "dragging")}
        onKeyDown={(event) => {
          const from = Math.max(currentIndex, 0);
          const next = event.key === "ArrowRight" || event.key === "ArrowUp"
            ? from + 1
            : event.key === "ArrowLeft" || event.key === "ArrowDown"
              ? from - 1
              : event.key === "Home"
                ? 0
                : event.key === "End"
                  ? lastIndex
                  : null;
          if (next === null) {
            return;
          }
          event.preventDefault();
          commit(next);
        }}
        onLostPointerCapture={() => setPreviewIndex(null)}
        onPointerDown={(event) => {
          if (event.button !== 0) {
            return;
          }
          event.currentTarget.setPointerCapture(event.pointerId);
          setPreviewIndex(indexAt(event.clientX));
        }}
        onPointerMove={(event) => {
          if (previewIndex !== null) {
            setPreviewIndex(indexAt(event.clientX));
          }
        }}
        onPointerUp={(event) => {
          if (previewIndex !== null) {
            commit(indexAt(event.clientX));
          }
          setPreviewIndex(null);
        }}
        ref={(element) => { props.focusRef.current = element; }}
        role="slider"
        tabIndex={0}
      >
        <div className="model-reasoning-rail" ref={railRef}>
          <span className="model-reasoning-fill" style={{ width: `${fillPercent}%` }} />
          {props.options.map((option, index) => (
            <span
              aria-hidden="true"
              className={cn(
                "model-reasoning-stop",
                shownIndex >= 0 && index <= shownIndex && "reached",
                index === defaultIndex && "default",
              )}
              key={option.value}
              style={{ left: `${position(index)}%` }}
            />
          ))}
          {shownIndex >= 0 ? (
            <span aria-hidden="true" className="model-reasoning-thumb" style={{ left: `${fillPercent}%` }} />
          ) : null}
        </div>
      </div>
      <div aria-hidden="true" className="model-reasoning-stop-labels">
        {props.options.map((option, index) => (
          <span
            className={cn(
              "model-reasoning-stop-label",
              index === shownIndex && "current",
              index === defaultIndex && "default",
            )}
            key={option.value}
            onClick={() => commit(index)}
            style={{ left: `${position(index)}%` }}
          >
            {labelOf(option)}
          </span>
        ))}
      </div>
    </div>
  );
}

// ModelReasoningSummary is the read-only hover card of the trigger: model, account, reasoning level and plan usage.
// ModelReasoningSummary 是触发器的只读悬浮卡片：展示模型、账号、思考层级与方案用量。
// Parameters: the resolved model choice, account details and reasoning view of the control.
// 参数：控件解析后的模型选项、账号详情与思考视图。
// Returns: a compact summary; every change still happens through the click-opened popover.
// 返回值：紧凑摘要；所有修改仍通过点击打开的浮层完成。
function ModelReasoningSummary(props: {
  modelLabel: string;
  selectedChoice: ModelPickerChoice | undefined;
  account: ModelAccountDetails;
  accountMissing: boolean;
  hasAccounts: boolean;
  reasoning: ReasoningSelectionView;
  options: ComposerSelectOption[];
}) {
  const { t } = useI18n();
  const { account } = props;
  const accountName = account.selectedAccount?.accountName
    ?? (props.accountMissing ? t("composer.modelPicker.selectAccount").replace("{model}", props.modelLabel) : null);
  const providerLine = [props.selectedChoice?.providerName, accountName].filter(Boolean).join(" · ");
  const scaleIndex = props.reasoning.kind === "scale" ? props.reasoning.index : -1;
  return (
    <div className="model-reasoning-summary-body">
      <div className="model-reasoning-summary-head">
        <span aria-hidden="true" className="model-reasoning-model-icon">
          {props.selectedChoice ? <ProviderIcon className="size-6 shrink-0" providerId={props.selectedChoice.providerId} /> : <Cpu size={15} />}
        </span>
        <span className="model-reasoning-model-copy">
          <strong>{props.modelLabel}</strong>
          {providerLine ? <small className={cn(props.accountMissing && "attention")}>{providerLine}</small> : null}
        </span>
      </div>
      <div className="model-reasoning-summary-row">
        <span className="model-reasoning-eyebrow">{t("composer.reasoning.tooltip")}</span>
        <span className="model-reasoning-summary-level">
          {props.reasoning.kind === "scale" ? (
            <span aria-hidden="true" className="model-reasoning-summary-pips">
              {props.options.map((option, index) => (
                <span className={cn(index <= scaleIndex && "on")} key={option.value} />
              ))}
            </span>
          ) : null}
          <strong>{props.reasoning.label ?? t("composer.reasoning.level.unavailable")}</strong>
        </span>
      </div>
      {props.hasAccounts && account.selectedAccount ? (
        <div className="model-reasoning-summary-usage">
          <span className="model-reasoning-eyebrow">{t("composer.modelReasoning.planUsage")}</span>
          {account.connection && account.usageOverviewVisible ? (
            <CredentialUsageOverview
              connection={account.connection}
              credentialProfile={account.credentialProfile}
              showQuotaSection={account.quotaWindowsSupported}
              usage={account.usage}
            />
          ) : !account.connection && account.loading ? (
            <StreamPlaceholder className="py-1" label={t("composer.accountCard.loading")} shape="tiles" count={1} />
          ) : (
            <p className="model-reasoning-hint">
              {account.connection ? t("composer.accountCard.usageNotQueried") : t("composer.accountCard.metadataUnavailable")}
            </p>
          )}
        </div>
      ) : null}
      <p className="model-reasoning-summary-foot">{t("composer.modelReasoning.clickHint")}</p>
    </div>
  );
}
