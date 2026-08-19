import React from "react";
import {
  confirmedComposerPersistenceValue,
  enqueueComposerPersistence,
} from "@/components/composer/composer-persistence-queue";
import {
  modelPickerChoiceKey,
  modelPickerChoicesFromProviders,
  type ModelPickerChoice,
} from "@/components/composer/model-picker-control";
import type { ComposerSelectOption } from "@/components/composer/composer-model";
import type { ProviderSummary, ReasoningSelectionSummary } from "@/types";

// ComposerActiveProviderModel identifies the provider and model currently targeted by composer controls.
// ComposerActiveProviderModel 标识输入区控件当前指向的 Provider 与模型。
export interface ComposerActiveProviderModel {
  // Provider that owns the active model.
  // 拥有当前模型的 Provider。
  provider: ProviderSummary;
  // Active model resolved from the provider snapshot.
  // 从 Provider 快照解析出的当前模型。
  model: NonNullable<ProviderSummary["models"]>[number];
}

// UseComposerModelSelectionOptions defines authoritative snapshots and persistence callbacks for model selection.
// UseComposerModelSelectionOptions 定义模型选择的权威快照与持久化回调。
export interface UseComposerModelSelectionOptions {
  // Exact conversation scope whose persisted model and account are being rendered.
  // 当前渲染其持久化模型与账号的精确会话作用域。
  conversationScopeKey: string;
  // Whether reasoning changes may be persisted to an already materialized conversation.
  // 思考档位变更是否可以持久化到已经物化的会话。
  persistReasoningSelection: boolean;
  // Providers and models projected by the shared workbench service.
  // 共享工作台服务投影出的 Provider 与模型。
  providers: ProviderSummary[];
  // Backend-resolved reasoning selector for the active provider and model.
  // 后端为当前 Provider 与模型解析出的推理选择器。
  reasoningSelection: ReasoningSelectionSummary | null;
  // Persists one selected provider/model pair.
  // 持久化一个选中的 Provider/模型组合。
  onSelectModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Persists one selected reasoning effort through the backend router.
  // 通过后端路由持久化一个选中的推理档位。
  onSelectReasoningEffort: (effort: string) => Promise<boolean>;
}

// ComposerModelSelectionController exposes synchronized model and reasoning controls to ComposerPanel.
// ComposerModelSelectionController 向 ComposerPanel 暴露同步的模型与推理控件。
export interface ComposerModelSelectionController {
  // Selected provider/model key rendered by ModelPickerControl.
  // ModelPickerControl 渲染的 Provider/模型选择键。
  modelChoice: string;
  // Exact active model account resolved from the current snapshot.
  // 从当前快照解析出的精确当前模型账号。
  activeModelChoice: ModelPickerChoice | null;
  // All credential accounts exposing the active provider model.
  // 暴露当前 Provider 模型的全部凭据账号。
  activeModelAccounts: ModelPickerChoice[];
  // Full active provider/model row used by secondary-route controls.
  // 二级路由控件使用的完整当前 Provider/模型行。
  activeProviderModel: ComposerActiveProviderModel | null;
  // Monotonic signal used to open the model picker from a slash command.
  // slash 指令用于打开模型选择器的单调信号。
  modelPickerOpenSignal: number;
  // Selected reasoning effort rendered by the composer toolbar.
  // 输入区工具栏渲染的推理档位。
  reasoningEffort: string;
  // Reasoning options resolved from backend capabilities or local preview fallback.
  // 从后端能力或本地预览回退解析出的推理选项。
  reasoningEffortOptions: ComposerSelectOption[];
  // Backend-confirmed effort safe to include in prompt submissions.
  // 可安全包含在提示词提交中的后端确认档位。
  submittedReasoningEffort: string | null;
  // Monotonic signal used to open the reasoning picker from a slash command.
  // slash 指令用于打开推理选择器的单调信号。
  reasoningPickerOpenSignal: number;
  // Applies one optimistic model selection, reports persistence success, and rolls back failure.
  // 应用一个乐观模型选择、报告持久化结果，并在失败时回滚。
  selectModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Applies one optimistic reasoning selection and rolls back the latest failed request.
  // 应用一个乐观推理选择，并回滚最新失败请求。
  selectReasoningEffort: (effort: string) => Promise<void>;
  // Opens the model picker through its external signal.
  // 通过外部信号打开模型选择器。
  openModelPicker: () => void;
  // Opens the reasoning picker through its external signal.
  // 通过外部信号打开推理选择器。
  openReasoningPicker: () => void;
}

// shouldDeferServiceModelChoiceSync protects an in-flight optimistic choice from same-session stale snapshots.
// shouldDeferServiceModelChoiceSync 保护正在持久化的乐观选择不被同会话旧快照覆盖。
// Parameters: scopeChanged identifies navigation, and pendingModelChoice identifies an active persistence request.
// 参数：scopeChanged 标识会话导航，pendingModelChoice 标识活动中的持久化请求。
// Returns: true only when the current session must keep its optimistic model choice.
// 返回值：仅当当前会话必须保留乐观模型选择时返回 true。
export function shouldDeferServiceModelChoiceSync(
  scopeChanged: boolean,
  pendingModelChoice: string | null,
): boolean {
  return !scopeChanged && pendingModelChoice !== null;
}

// serviceModelChoiceSyncValue resolves one authoritative model snapshot without inventing a fallback.
// serviceModelChoiceSyncValue 在不虚构回退模型的前提下解析一份权威模型快照。
// Parameters: scopeChanged identifies navigation and serviceChoiceKey is the backend-selected
// exact account key.
// 参数：scopeChanged 标识会话切换，serviceChoiceKey 是后端选中的精确账号键。
// Returns: the value to synchronize, or null when a same-session transient snapshot must preserve
// the current exact selection.
// 返回值：需要同步的值；同一会话的瞬时快照应保留当前精确选择时返回 null。
export function serviceModelChoiceSyncValue(
  scopeChanged: boolean,
  serviceChoiceKey: string,
): string | null {
  if (serviceChoiceKey) {
    return serviceChoiceKey;
  }
  return scopeChanged ? "" : null;
}

// useComposerModelSelection synchronizes service snapshots with optimistic model and reasoning choices.
// useComposerModelSelection 将服务快照与乐观模型、推理选择保持同步。
// Parameters: options provides authoritative provider data, reasoning capabilities, and persistence callbacks.
// 参数：options 提供权威 Provider 数据、推理能力与持久化回调。
// Returns: stable selection state, submission-safe reasoning data, and picker actions.
// 返回值：稳定的选择状态、提交安全的推理数据与选择器操作。
export function useComposerModelSelection(options: UseComposerModelSelectionOptions): ComposerModelSelectionController {
  // modelChoice stores the optimistic provider/model key rendered by the picker.
  // modelChoice 保存模型选择器渲染的乐观 Provider/模型键。
  const [modelChoice, setModelChoice] = React.useState("");
  // modelPickerOpenSignal opens the existing model picker from a slash command.
  // modelPickerOpenSignal 通过 slash 指令打开现有模型选择器。
  const [modelPickerOpenSignal, setModelPickerOpenSignal] = React.useState(0);
  // reasoningEffort stores the backend-synchronized reasoning selection.
  // reasoningEffort 保存与后端同步的推理选择。
  const [reasoningEffort, setReasoningEffort] = React.useState("");
  // reasoningPickerOpenSignal opens the existing reasoning selector from a slash command.
  // reasoningPickerOpenSignal 通过 slash 指令打开现有推理选择器。
  const [reasoningPickerOpenSignal, setReasoningPickerOpenSignal] = React.useState(0);
  // modelRequestVersionRef prevents an older failed request from rolling back a newer model choice.
  // modelRequestVersionRef 防止较早的失败请求回滚较新的模型选择。
  const modelRequestVersionRef = React.useRef(0);
  // modelPersistenceQueueRef serializes model and account writes so an older RPC cannot finish last.
  // modelPersistenceQueueRef 串行化模型与账号写入，避免较早 RPC 最后完成并覆盖最新意图。
  const modelPersistenceQueueRef = React.useRef<Promise<void>>(Promise.resolve());
  // confirmedModelChoiceRef retains the latest authoritative or successfully persisted model value.
  // confirmedModelChoiceRef 保留最新权威或已成功持久化的模型值。
  const confirmedModelChoiceRef = React.useRef("");
  // pendingModelChoiceRef protects the latest optimistic choice until its service request settles.
  // pendingModelChoiceRef 在最新服务请求结束前保护对应的乐观选择。
  const pendingModelChoiceRef = React.useRef<string | null>(null);
  // modelChoiceScopeRef distinguishes session navigation from same-session snapshot refreshes.
  // modelChoiceScopeRef 区分会话导航与同会话快照刷新。
  const modelChoiceScopeRef = React.useRef(options.conversationScopeKey);
  // reasoningRequestVersionRef prevents an older failed request from rolling back a newer reasoning choice.
  // reasoningRequestVersionRef 防止较早的失败请求回滚较新的推理选择。
  const reasoningRequestVersionRef = React.useRef(0);
  // reasoningPersistenceQueueRef preserves user-intent order across overlapping reasoning writes.
  // reasoningPersistenceQueueRef 在重叠推理设置写入之间保持用户意图顺序。
  const reasoningPersistenceQueueRef = React.useRef<Promise<void>>(Promise.resolve());
  // pendingReasoningEffortRef protects the latest optimistic effort from a stale same-session snapshot.
  // pendingReasoningEffortRef 保护最新乐观推理档位，避免同会话旧快照覆盖。
  const pendingReasoningEffortRef = React.useRef<string | null>(null);
  // reasoningScopeRef distinguishes navigation from a same-session reasoning refresh.
  // reasoningScopeRef 区分会话导航与同会话推理状态刷新。
  const reasoningScopeRef = React.useRef(options.conversationScopeKey);
  // reasoningEffortValueRef exposes the latest optimistic value without a stale callback closure.
  // reasoningEffortValueRef 暴露最新乐观值，避免回调闭包读取陈旧状态。
  const reasoningEffortValueRef = React.useRef("");
  // confirmedReasoningEffortRef retains the latest authoritative or successfully persisted effort.
  // confirmedReasoningEffortRef 保留最新权威或已成功持久化的推理档位。
  const confirmedReasoningEffortRef = React.useRef("");
  // availableModelChoices flattens the authoritative provider snapshot into picker rows.
  // availableModelChoices 将权威 Provider 快照扁平化为选择器行。
  const availableModelChoices = React.useMemo(
    () => modelPickerChoicesFromProviders(options.providers),
    [options.providers],
  );
  // activeModelChoiceKey follows only the exact service-reported active account.
  // activeModelChoiceKey 仅跟随后端明确上报的当前精确账号。
  const activeModelChoiceKey = React.useMemo(() => {
    // activeChoice is absent while an established session catalog is being refreshed.
    // 已有会话目录刷新期间，activeChoice 可以暂时缺失。
    const activeChoice = availableModelChoices.find((choice) => choice.active);
    return modelPickerChoiceKey(activeChoice);
  }, [availableModelChoices]);
  // activeProviderModel resolves the full API rows needed by secondary-route controls.
  // activeProviderModel 解析二级路由控件所需的完整 API 行。
  const activeProviderModel = React.useMemo<ComposerActiveProviderModel | null>(() => {
    // selectedChoice resolves the exact provider, model, and credential row.
    // selectedChoice 解析精确的供应商、模型与凭据行。
    const selectedChoice = availableModelChoices.find(
      (choice) => modelPickerChoiceKey(choice) === (modelChoice || activeModelChoiceKey),
    );
    if (!selectedChoice) {
      return null;
    }
    // provider is the unique credential row owning the selected connection.
    // provider 是拥有所选连接的唯一凭据行。
    const provider = options.providers.find(
      (candidate) => candidate.connectionId === selectedChoice.connectionId,
    );
    // model is the unique model row selected within that provider.
    // model 是该 Provider 内唯一选中的模型行。
    const model = provider?.models?.find((candidate) => candidate.id === selectedChoice.modelId) ?? null;
    return provider && model ? { provider, model } : null;
  }, [activeModelChoiceKey, availableModelChoices, modelChoice, options.providers]);
  // activeModelChoice is the exact account row currently selected for the active model.
  // activeModelChoice 是当前模型精确选中的账号行。
  const activeModelChoice = React.useMemo(
    () => availableModelChoices.find(
      (choice) => modelPickerChoiceKey(choice) === (modelChoice || activeModelChoiceKey),
    ) ?? null,
    [activeModelChoiceKey, availableModelChoices, modelChoice],
  );
  // activeModelAccounts contains only accounts serving the exact active provider/model pair.
  // activeModelAccounts 仅包含服务于精确当前 Provider/模型组合的账号。
  const activeModelAccounts = React.useMemo(() => {
    if (!activeModelChoice) {
      return [];
    }
    return availableModelChoices.filter((choice) => (
      choice.providerId === activeModelChoice.providerId
      && choice.modelId === activeModelChoice.modelId
    ));
  }, [activeModelChoice, availableModelChoices]);
  // effectiveReasoningSelection uses the selected model contract for drafts and session state after materialization.
  // effectiveReasoningSelection 对草稿使用已选模型契约，物化后使用会话状态。
  const effectiveReasoningSelection = options.persistReasoningSelection
    ? options.reasoningSelection
    : activeProviderModel?.model.reasoningSelection ?? null;
  // reasoningEffortOptions exposes only levels declared by the active model catalog.
  // reasoningEffortOptions 仅暴露当前模型目录明确声明的档位。
  const reasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => {
    return effectiveReasoningSelection?.options.map((option) => ({
      value: option.value,
      label: option.label,
    })) ?? [];
  }, [effectiveReasoningSelection]);
  // reasoningEffortValuesSignature changes only when the set or order of selectable values changes.
  // reasoningEffortValuesSignature 仅在可选值集合或顺序变化时改变。
  const reasoningEffortValuesSignature = React.useMemo(
    () => JSON.stringify(reasoningEffortOptions.map((option) => option.value)),
    [reasoningEffortOptions],
  );
  // serviceSelectedReasoningEffort is the authoritative selected effort from the backend snapshot.
  // serviceSelectedReasoningEffort 是后端快照中的权威选中档位。
  const serviceSelectedReasoningEffort = effectiveReasoningSelection?.selectedEffort;
  // submittedReasoningEffort is non-null only when backend capabilities explicitly allow the local value.
  // submittedReasoningEffort 仅在后端能力明确允许本地值时非空。
  const submittedReasoningEffort = React.useMemo(() => {
    if (reasoningEffort === "fixed-on" || reasoningEffort === "fixed-off") {
      return null;
    }
    if (!effectiveReasoningSelection?.options.some((option) => option.value === reasoningEffort)) {
      return null;
    }
    return reasoningEffort;
  }, [effectiveReasoningSelection, reasoningEffort]);

  React.useLayoutEffect(() => {
    // scopeChanged makes a different conversation's persisted model immediately authoritative.
    // scopeChanged 使不同会话的持久化模型立即成为权威值。
    const scopeChanged = modelChoiceScopeRef.current !== options.conversationScopeKey;
    modelChoiceScopeRef.current = options.conversationScopeKey;
    if (shouldDeferServiceModelChoiceSync(scopeChanged, pendingModelChoiceRef.current)) {
      return;
    }
    const synchronizedChoice = serviceModelChoiceSyncValue(
      scopeChanged,
      activeModelChoiceKey,
    );
    if (synchronizedChoice === null) {
      return;
    }
    pendingModelChoiceRef.current = null;
    modelRequestVersionRef.current += 1;
    confirmedModelChoiceRef.current = synchronizedChoice;
    setModelChoice(synchronizedChoice);
  }, [activeModelChoiceKey, options.conversationScopeKey]);

  React.useLayoutEffect(() => {
    // scopeChanged makes another conversation's persisted reasoning immediately authoritative.
    // scopeChanged 使另一会话的持久化推理档位立即成为权威值。
    const scopeChanged = reasoningScopeRef.current !== options.conversationScopeKey;
    reasoningScopeRef.current = options.conversationScopeKey;
    if (!scopeChanged && pendingReasoningEffortRef.current !== null) {
      return;
    }
    pendingReasoningEffortRef.current = null;
    reasoningRequestVersionRef.current += 1;
    // currentEffort reads the immediate optimistic value rather than the render that scheduled this effect.
    // currentEffort 读取即时乐观值，而不是调度本 Effect 的渲染快照。
    const currentEffort = reasoningEffortValueRef.current;
    // nextEffort applies the same authoritative/default policy without a state-updater side effect.
    // nextEffort 在不向状态更新器写副作用的前提下应用同一权威值或默认值策略。
    const nextEffort = !options.persistReasoningSelection
      && reasoningEffortOptions.some((option) => option.value === currentEffort)
      ? currentEffort
      : serviceSelectedReasoningEffort
        || (reasoningEffortOptions.some((option) => option.value === currentEffort)
          ? currentEffort
          : reasoningEffortOptions[0]?.value ?? "");
    reasoningEffortValueRef.current = nextEffort;
    confirmedReasoningEffortRef.current = nextEffort;
    setReasoningEffort(nextEffort);
  }, [
    options.conversationScopeKey,
    options.persistReasoningSelection,
    reasoningEffortValuesSignature,
    serviceSelectedReasoningEffort,
  ]);

  // selectModel applies an optimistic model value and only lets the latest failed request roll it back.
  // selectModel 应用乐观模型值，仅允许最新失败请求执行回滚。
  const selectModel = React.useCallback(async (choice: ModelPickerChoice) => {
    // nextChoice is the canonical provider/model key rendered immediately.
    // nextChoice 是立即渲染的规范 Provider/模型键。
    const nextChoice = modelPickerChoiceKey(choice);
    // requestScope prevents an old conversation result from changing the new conversation's local baseline.
    // requestScope 防止旧会话结果修改新会话的本地基线。
    const requestScope = options.conversationScopeKey;
    // requestVersion identifies this optimistic request among overlapping selections.
    // requestVersion 在重叠选择中标识本次乐观请求。
    const requestVersion = modelRequestVersionRef.current + 1;
    modelRequestVersionRef.current = requestVersion;
    pendingModelChoiceRef.current = nextChoice;
    setModelChoice(nextChoice);
    // queuedWrite makes the latest model/account intent execute after every older write.
    // queuedWrite 使最新模型或账号意图在全部较早写入之后执行。
    const queuedWrite = enqueueComposerPersistence(
      modelPersistenceQueueRef.current,
      () => options.onSelectModel(choice),
    );
    modelPersistenceQueueRef.current = queuedWrite.tail;
    // saved records whether the service accepted this model selection.
    // saved 记录服务是否接受本次模型选择。
    const saved = await queuedWrite.result;
    if (modelChoiceScopeRef.current !== requestScope) {
      return saved;
    }
    confirmedModelChoiceRef.current = confirmedComposerPersistenceValue(
      confirmedModelChoiceRef.current,
      nextChoice,
      saved,
    );
    if (modelRequestVersionRef.current === requestVersion) {
      pendingModelChoiceRef.current = null;
      if (!saved) {
        setModelChoice(confirmedModelChoiceRef.current);
      }
    }
    return saved;
  }, [options.conversationScopeKey, options.onSelectModel]);

  // selectReasoningEffort applies an optimistic effort and only lets the latest failed request roll it back.
  // selectReasoningEffort 应用乐观推理档位，仅允许最新失败请求执行回滚。
  const selectReasoningEffort = React.useCallback(async (effort: string) => {
    // requestScope prevents an old conversation result from changing the new conversation's local baseline.
    // requestScope 防止旧会话结果修改新会话的本地基线。
    const requestScope = options.conversationScopeKey;
    // requestVersion identifies this optimistic request among overlapping selections.
    // requestVersion 在重叠选择中标识本次乐观请求。
    const requestVersion = reasoningRequestVersionRef.current + 1;
    reasoningRequestVersionRef.current = requestVersion;
    reasoningEffortValueRef.current = effort;
    setReasoningEffort(effort);
    if (!options.persistReasoningSelection) {
      return;
    }
    pendingReasoningEffortRef.current = effort;
    // queuedWrite preserves last-intent-wins semantics at the serialized backend boundary.
    // queuedWrite 在串行后端边界保持最后意图胜出语义。
    const queuedWrite = enqueueComposerPersistence(
      reasoningPersistenceQueueRef.current,
      () => options.onSelectReasoningEffort(effort),
    );
    reasoningPersistenceQueueRef.current = queuedWrite.tail;
    // saved records whether the service accepted this reasoning selection.
    // saved 记录服务是否接受本次推理选择。
    const saved = await queuedWrite.result;
    if (reasoningScopeRef.current !== requestScope) {
      return;
    }
    confirmedReasoningEffortRef.current = confirmedComposerPersistenceValue(
      confirmedReasoningEffortRef.current,
      effort,
      saved,
    );
    if (reasoningRequestVersionRef.current === requestVersion) {
      pendingReasoningEffortRef.current = null;
      if (!saved) {
        reasoningEffortValueRef.current = confirmedReasoningEffortRef.current;
        setReasoningEffort(confirmedReasoningEffortRef.current);
      }
    }
  }, [
    options.conversationScopeKey,
    options.onSelectReasoningEffort,
    options.persistReasoningSelection,
  ]);

  // openModelPicker increments the external open signal consumed by ModelPickerControl.
  // openModelPicker 递增 ModelPickerControl 使用的外部打开信号。
  const openModelPicker = React.useCallback(() => {
    setModelPickerOpenSignal((current) => current + 1);
  }, []);
  // openReasoningPicker increments the external open signal consumed by the toolbar selector.
  // openReasoningPicker 递增工具栏选择器使用的外部打开信号。
  const openReasoningPicker = React.useCallback(() => {
    setReasoningPickerOpenSignal((current) => current + 1);
  }, []);

  return {
    modelChoice,
    activeModelChoice,
    activeModelAccounts,
    activeProviderModel,
    modelPickerOpenSignal,
    reasoningEffort,
    reasoningEffortOptions,
    submittedReasoningEffort,
    reasoningPickerOpenSignal,
    selectModel,
    selectReasoningEffort,
    openModelPicker,
    openReasoningPicker,
  };
}
