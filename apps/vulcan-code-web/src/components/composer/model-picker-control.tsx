import * as React from "react";
import { Check, Cpu, RefreshCw, Search, UserRound } from "lucide-react";
import { ProviderIcon } from "@/components/features/settings/provider-icon";
import { localizedProviderName } from "@/components/features/settings/provider-presentation";
import { ModelBuiltinCapabilityIcons } from "@/components/shared/model-builtin-capabilities";
import {
  CredentialUsageOverview,
  usageCapabilitySupportsQuotaWindows,
} from "@/components/public-ai/credential-usage-overview";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/components/ui/hover-card";
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
import {
  loadPublicAiSettings,
  queryPublicAiConnectionUsage,
} from "@/service/settings-actions-service";
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
  // choices localizes built-in provider labels without changing exact provider or account identities.
  // choices 本地化内置供应商标签，同时不改变精确供应商或账号身份。
  const choices = React.useMemo(() => (
    modelPickerChoicesFromProviders(props.providers).map((choice) => ({
      ...choice,
      providerName: localizedProviderName(choice.providerId, choice.providerName, t),
    }))
  ), [props.providers, t]);
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
                    <span className="model-picker-row-side">
                      <span className="model-picker-row-id">{group.modelId}</span>
                      <ModelBuiltinCapabilityIcons
                        className="model-picker-row-capabilities"
                        inputResources={resolvedGroupChoice.inputModalities}
                        providerNativeTools={resolvedGroupChoice.providerNativeTools}
                      />
                    </span>
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
// Returns: a compact account control whenever the active model has an available account.
// 返回值：当前模型存在可用账号时返回紧凑账号控件。
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
  // t resolves account-card copy in the active locale.
  // t 使用当前语言解析账号卡片文案。
  const { t } = useI18n();
  // selectorOpen prevents the account details card from covering the account menu.
  // selectorOpen 防止账号详情卡片遮挡账号菜单。
  const [selectorOpen, setSelectorOpen] = React.useState(false);
  // detailsOpen controls the hoverable account information card.
  // detailsOpen 控制可驻留的账号信息悬浮卡片。
  const [detailsOpen, setDetailsOpen] = React.useState(false);
  // detailsSnapshot contains the latest process-memory credential and usage snapshot.
  // detailsSnapshot 包含最新的进程内凭据与用量快照。
  const [detailsSnapshot, setDetailsSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  // detailsLoading reports the memory-snapshot read in progress.
  // detailsLoading 表示正在读取内存快照。
  const [detailsLoading, setDetailsLoading] = React.useState(false);
  // usageRefreshing reports an explicit provider usage refresh in progress.
  // usageRefreshing 表示正在执行显式供应商用量刷新。
  const [usageRefreshing, setUsageRefreshing] = React.useState(false);
  // detailsError retains the latest account-card operation failure.
  // detailsError 保留最近一次账号卡片操作失败。
  const [detailsError, setDetailsError] = React.useState<string | null>(null);
  // selectedAccount resolves the exact current connection with the established first-account degradation.
  // selectedAccount 使用既定的首账号降级规则解析精确当前连接。
  const selectedAccount = props.accounts.find((account) => account.connectionId === props.value)
    ?? props.accounts[0];
  // connection is the authoritative redacted credential metadata for this account.
  // connection 是该账号的权威脱敏凭据元数据。
  const connection = detailsSnapshot?.connections.find((candidate) => candidate.id === selectedAccount?.connectionId);
  // effectiveProtocolId comes only from the backend-resolved provider/connection/model route.
  // effectiveProtocolId 仅来自后端解析的供应商、连接与模型精确路由。
  const effectiveProtocolId = modelAccountEffectiveProtocolId(detailsSnapshot, selectedAccount);
  // provider is the registered adapter that owns the exact credential.
  // provider 是拥有该精确凭据的已注册适配器。
  const provider = detailsSnapshot?.providers.find((candidate) => candidate.id === connection?.provider_id);
  // entry is the exact regional or plan service entry selected by the credential.
  // entry 是该凭据选择的精确区域或套餐服务入口。
  const entry = provider?.service_entries.find((candidate) => candidate.id === connection?.service_entry_id);
  // credentialProfile is the exact acquisition and usage contract selected by the credential.
  // credentialProfile 是该凭据选择的精确获取与用量契约。
  const credentialProfile = entry?.credential_profiles.find((candidate) => candidate.id === connection?.credential_profile_id);
  // usage is the latest cached or explicitly refreshed account observation.
  // usage 是最新缓存或显式刷新的账号观测。
  const usage = connection ? detailsSnapshot?.usageCache[connection.id] : undefined;
  // quotaWindowsSupported follows the backend-declared account usage implementation.
  // quotaWindowsSupported 遵循后端声明的账号用量实现。
  const quotaWindowsSupported = connection
    ? usageCapabilitySupportsQuotaWindows(connection.usage_capability)
    : false;
  // usageOverviewVisible keeps quota-capable accounts visible while showing balance-only accounts after observation.
  // usageOverviewVisible 始终展示额度账号，并在余额账号取得观测后展示余额。
  const usageOverviewVisible = quotaWindowsSupported || Boolean(usage);
  // usageSupported follows the backend-declared credential requirement for this usage primitive.
  // usageSupported 遵循后端为当前用量原语声明的凭据要求。
  const usageSupported = Boolean(
    connection
    && credentialProfile?.account_usage_query !== "unsupported"
    && (!connection.usage_requires_management_key || connection.has_management_key),
  );

  // loadAccountDetails reads only the latest Core-managed memory snapshot.
  // loadAccountDetails 仅读取最新的 Core 托管内存快照。
  const loadAccountDetails = React.useCallback(async () => {
    setDetailsLoading(true);
    setDetailsError(null);
    try {
      // snapshot is the authoritative process-memory account and usage view.
      // snapshot 是权威的进程内账号与用量视图。
      const snapshot = await loadPublicAiSettings();
      setDetailsSnapshot(snapshot);
    } catch (reason) {
      setDetailsError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setDetailsLoading(false);
    }
  }, []);

  // refreshAccountUsage performs the only provider-facing action available inside the card.
  // refreshAccountUsage 执行卡片内唯一允许访问供应商的操作。
  const refreshAccountUsage = React.useCallback(async () => {
    if (!connection || !usageSupported) {
      return;
    }
    setUsageRefreshing(true);
    setDetailsError(null);
    try {
      // refreshedUsage is the exact account-scoped provider observation.
      // refreshedUsage 是精确账号作用域的供应商观测。
      const refreshedUsage = await queryPublicAiConnectionUsage(connection.id);
      setDetailsSnapshot((current) => current ? {
        ...current,
        usageCache: {
          ...current.usageCache,
          [connection.id]: refreshedUsage,
        },
      } : current);
    } catch (reason) {
      setDetailsError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setUsageRefreshing(false);
    }
  }, [connection, usageSupported]);

  React.useEffect(() => {
    setDetailsOpen(false);
    setDetailsError(null);
  }, [props.value]);

  if (!selectedAccount) {
    return null;
  }
  // hasAlternativeAccounts controls whether the account control opens a selector.
  // hasAlternativeAccounts 控制账号控件是否打开选择器。
  const hasAlternativeAccounts = props.accounts.length > 1;
  return (
    <HoverCard
      closeDelay={250}
      onOpenChange={(open) => {
        if (open && selectorOpen) {
          return;
        }
        setDetailsOpen(open);
        if (open) {
          void loadAccountDetails();
        }
      }}
      open={detailsOpen}
      openDelay={500}
    >
      {hasAlternativeAccounts ? (
        <Select
          disabled={props.disabled}
          onOpenChange={(open) => {
            setSelectorOpen(open);
            if (open) {
              setDetailsOpen(false);
            }
          }}
          onValueChange={(connectionId) => {
            // account is the exact model account selected by its stable connection id.
            // account 是通过稳定连接标识选中的精确模型账号。
            const account = props.accounts.find((choice) => choice.connectionId === connectionId);
            if (account) {
              props.onValueChange(account);
            }
          }}
          open={selectorOpen}
          value={props.value}
        >
          <HoverCardTrigger asChild>
            <SelectTrigger
              aria-label={props.ariaLabel}
              className="composer-select-trigger model-account"
              size="sm"
            >
              <span className="composer-select-icon"><UserRound size={15} /></span>
              <SelectValue />
            </SelectTrigger>
          </HoverCardTrigger>
          <SelectContent className="model-picker-account-content">
            {props.accounts.map((account) => (
              <SelectItem key={account.connectionId} value={account.connectionId}>
                {account.accountName}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ) : (
        <HoverCardTrigger asChild>
          <Button
            aria-label={props.ariaLabel}
            className="composer-select-trigger model-account"
            disabled={props.disabled}
            size="sm"
            type="button"
            variant="ghost"
          >
            <span className="composer-select-icon"><UserRound size={15} /></span>
            <span data-slot="select-value">{selectedAccount.accountName}</span>
          </Button>
        </HoverCardTrigger>
      )}
      <HoverCardContent align="start" className="w-[22rem] p-0" side="top" sideOffset={8}>
        <Card className="w-full gap-0 border-0 ring-0 shadow-none" size="sm">
          <CardHeader>
            <div className="flex items-start gap-3">
              <ProviderIcon
                category={provider?.category}
                className="size-8 shrink-0"
                providerId={connection?.provider_id ?? selectedAccount.providerId}
                serviceEntryId={connection?.service_entry_id}
              />
              <div className="min-w-0 flex-1">
                <CardTitle className="truncate">{selectedAccount.accountName}</CardTitle>
                <CardDescription className="truncate text-xs">
                  {provider
                    ? localizedProviderName(provider.id, provider.display_name, t)
                    : selectedAccount.providerName}
                </CardDescription>
              </div>
              {usageSupported ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      aria-label={t("composer.accountCard.refresh")}
                      disabled={usageRefreshing}
                      onClick={() => void refreshAccountUsage()}
                      size="icon-sm"
                      type="button"
                      variant="outline"
                    >
                      <RefreshCw className={`size-4 ${usageRefreshing ? "animate-spin" : ""}`} />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="top">{t("composer.accountCard.refresh")}</TooltipContent>
                </Tooltip>
              ) : null}
            </div>
          </CardHeader>
          <CardContent className="mt-3 flex flex-col gap-3">
            <div className="border-t pt-3 text-xs">
              <p className="text-muted-foreground">{t("composer.accountCard.protocolType")}</p>
              <p className="mt-0.5 truncate font-medium">
                {effectiveProtocolId ? modelAccountProtocolDisplayName(effectiveProtocolId) : "—"}
              </p>
            </div>
            {connection && usageOverviewVisible ? (
              <CredentialUsageOverview
                connection={connection}
                credentialProfile={credentialProfile}
                showQuotaSection={quotaWindowsSupported}
                usage={usage}
              />
            ) : !connection && detailsLoading ? (
              <p className="py-3 text-center text-xs text-muted-foreground">{t("composer.accountCard.loading")}</p>
            ) : !connection ? (
              <p className="py-3 text-center text-xs text-muted-foreground">{t("composer.accountCard.metadataUnavailable")}</p>
            ) : null}
            {detailsError ? <p className="text-xs text-destructive" role="status">{detailsError}</p> : null}
          </CardContent>
        </Card>
      </HoverCardContent>
    </HoverCard>
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

