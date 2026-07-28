import React from "react";
import {
  modelPickerChoiceKey,
  modelPickerChoicesFromProviders,
  type ModelPickerChoice,
} from "@/components/composer/model-picker-control";
import type { ComposerSelectOption } from "@/components/composer/composer-model";
import { useI18n } from "@/i18n";
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
  // Applies one optimistic model selection and rolls back the latest failed request.
  // 应用一个乐观模型选择，并回滚最新失败请求。
  selectModel: (choice: ModelPickerChoice) => Promise<void>;
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

// useComposerModelSelection synchronizes service snapshots with optimistic model and reasoning choices.
// useComposerModelSelection 将服务快照与乐观模型、推理选择保持同步。
// Parameters: options provides authoritative provider data, reasoning capabilities, and persistence callbacks.
// 参数：options 提供权威 Provider 数据、推理能力与持久化回调。
// Returns: stable selection state, submission-safe reasoning data, and picker actions.
// 返回值：稳定的选择状态、提交安全的推理数据与选择器操作。
export function useComposerModelSelection(options: UseComposerModelSelectionOptions): ComposerModelSelectionController {
  // t resolves fallback reasoning labels when the backend selector is unavailable.
  // t 在后端选择器不可用时解析回退推理标签。
  const { t } = useI18n();
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
  // reasoningRequestVersionRef prevents an older failed request from rolling back a newer reasoning choice.
  // reasoningRequestVersionRef 防止较早的失败请求回滚较新的推理选择。
  const reasoningRequestVersionRef = React.useRef(0);
  // availableModelChoices flattens the authoritative provider snapshot into picker rows.
  // availableModelChoices 将权威 Provider 快照扁平化为选择器行。
  const availableModelChoices = React.useMemo(
    () => modelPickerChoicesFromProviders(options.providers),
    [options.providers],
  );
  // activeModelChoiceKey follows the service-reported active model or the first available model.
  // activeModelChoiceKey 跟随服务上报的当前模型，或首个可用模型。
  const activeModelChoiceKey = React.useMemo(() => {
    // activeChoice is the service-selected model row or the deterministic first fallback.
    // activeChoice 是服务选中的模型行，或确定性的首行回退。
    const activeChoice = availableModelChoices.find((choice) => choice.active) ?? availableModelChoices[0];
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
  // fallbackReasoningEffortOptions supplies preview labels only when the backend exposes no selector view.
  // fallbackReasoningEffortOptions 仅在后端未暴露选择器视图时提供预览标签。
  const fallbackReasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => [
    { value: "off", label: t("composer.reasoning.off") },
    { value: "low", label: t("composer.reasoning.low") },
    { value: "medium", label: t("composer.reasoning.medium") },
    { value: "high", label: t("composer.reasoning.high") },
    { value: "x-high", label: t("composer.reasoning.max") },
  ], [t]);
  // reasoningEffortOptions prefers backend-routed capabilities over local preview labels.
  // reasoningEffortOptions 优先使用后端路由能力，再使用本地预览标签。
  const reasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => {
    if (options.reasoningSelection?.options.length) {
      return options.reasoningSelection.options.map((option) => ({
        value: option.value,
        label: option.label,
      }));
    }
    return fallbackReasoningEffortOptions;
  }, [fallbackReasoningEffortOptions, options.reasoningSelection]);
  // reasoningEffortValuesSignature changes only when the set or order of selectable values changes.
  // reasoningEffortValuesSignature 仅在可选值集合或顺序变化时改变。
  const reasoningEffortValuesSignature = React.useMemo(
    () => JSON.stringify(reasoningEffortOptions.map((option) => option.value)),
    [reasoningEffortOptions],
  );
  // serviceSelectedReasoningEffort is the authoritative selected effort from the backend snapshot.
  // serviceSelectedReasoningEffort 是后端快照中的权威选中档位。
  const serviceSelectedReasoningEffort = options.reasoningSelection?.selectedEffort;
  // submittedReasoningEffort is non-null only when backend capabilities explicitly allow the local value.
  // submittedReasoningEffort 仅在后端能力明确允许本地值时非空。
  const submittedReasoningEffort = React.useMemo(() => {
    if (!options.reasoningSelection?.options.some((option) => option.value === reasoningEffort)) {
      return null;
    }
    return reasoningEffort;
  }, [options.reasoningSelection, reasoningEffort]);

  React.useEffect(() => {
    if (activeModelChoiceKey) {
      setModelChoice(activeModelChoiceKey);
    }
  }, [activeModelChoiceKey]);

  React.useEffect(() => {
    setReasoningEffort((current) => {
      if (serviceSelectedReasoningEffort) {
        return serviceSelectedReasoningEffort;
      }
      if (reasoningEffortOptions.some((option) => option.value === current)) {
        return current;
      }
      return reasoningEffortOptions[0]?.value ?? "medium";
    });
  }, [reasoningEffortValuesSignature, serviceSelectedReasoningEffort]);

  // selectModel applies an optimistic model value and only lets the latest failed request roll it back.
  // selectModel 应用乐观模型值，仅允许最新失败请求执行回滚。
  const selectModel = React.useCallback(async (choice: ModelPickerChoice) => {
    // previousChoice is the local value restored when the latest persistence request fails.
    // previousChoice 是最新持久化请求失败时恢复的本地值。
    const previousChoice = modelChoice;
    // nextChoice is the canonical provider/model key rendered immediately.
    // nextChoice 是立即渲染的规范 Provider/模型键。
    const nextChoice = modelPickerChoiceKey(choice);
    // requestVersion identifies this optimistic request among overlapping selections.
    // requestVersion 在重叠选择中标识本次乐观请求。
    const requestVersion = modelRequestVersionRef.current + 1;
    modelRequestVersionRef.current = requestVersion;
    setModelChoice(nextChoice);
    // saved records whether the service accepted this model selection.
    // saved 记录服务是否接受本次模型选择。
    const saved = await options.onSelectModel(choice);
    if (!saved && modelRequestVersionRef.current === requestVersion) {
      setModelChoice(previousChoice);
    }
  }, [modelChoice, options.onSelectModel]);

  // selectReasoningEffort applies an optimistic effort and only lets the latest failed request roll it back.
  // selectReasoningEffort 应用乐观推理档位，仅允许最新失败请求执行回滚。
  const selectReasoningEffort = React.useCallback(async (effort: string) => {
    // previousEffort is the local value restored when the latest persistence request fails.
    // previousEffort 是最新持久化请求失败时恢复的本地值。
    const previousEffort = reasoningEffort;
    // requestVersion identifies this optimistic request among overlapping selections.
    // requestVersion 在重叠选择中标识本次乐观请求。
    const requestVersion = reasoningRequestVersionRef.current + 1;
    reasoningRequestVersionRef.current = requestVersion;
    setReasoningEffort(effort);
    // saved records whether the service accepted this reasoning selection.
    // saved 记录服务是否接受本次推理选择。
    const saved = await options.onSelectReasoningEffort(effort);
    if (!saved && reasoningRequestVersionRef.current === requestVersion) {
      setReasoningEffort(previousEffort);
    }
  }, [options.onSelectReasoningEffort, reasoningEffort]);

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
