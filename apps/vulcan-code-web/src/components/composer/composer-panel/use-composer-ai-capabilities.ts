import React from "react";
import {
  aiCapabilitySignature,
  defaultAiCapabilityTargetBindings,
  defaultDisabledAiCapabilityKinds,
  defaultDisabledAiCapabilityTargetSelectors,
  serializeAiCapabilityTargetBindings,
  type AiCapabilityKind,
  type ComposerAiCapabilityConfigurationSave,
} from "@/components/composer/composer-model";
import { defaultAiCapabilityTargetBindingsFromRecord } from "@/components/composer/composer-panel/helpers";
import type { AiCapabilitySummary } from "@/types";

// UseComposerAiCapabilitiesOptions defines the authoritative snapshot and persistence boundary for quick controls.
// UseComposerAiCapabilitiesOptions 定义快捷控件的权威快照与持久化边界。
export interface UseComposerAiCapabilitiesOptions {
  // Stable conversation or new-conversation draft slot that owns local preview state.
  // 持有本地预览状态的稳定会话或新建会话草稿槽。
  scopeKey: string;
  // Session-resolved AI capability routes projected by the workbench service.
  // 工作台服务投影出的会话级 AI 能力路由。
  capabilities: AiCapabilitySummary[];
  // Persists one complete capability configuration from the editor dialog.
  // 持久化能力编辑弹窗中的一份完整配置。
  onSaveConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
}

// ComposerAiCapabilitiesController exposes synchronized quick-control state and submission configuration.
// ComposerAiCapabilitiesController 暴露同步的快捷控件状态与提交配置。
export interface ComposerAiCapabilitiesController {
  // Capability kinds disabled for the next prompt.
  // 下一条提示词禁用的能力类型。
  disabledKinds: Set<AiCapabilityKind>;
  // Capability target selectors disabled for the next prompt.
  // 下一条提示词禁用的能力目标选择器。
  disabledTargetSelectors: Set<string>;
  // Exact target selectors grouped by capability kind.
  // 按能力类型分组的精确目标选择器。
  targetBindings: Map<AiCapabilityKind, string>;
  // Complete serialized configuration included in every prompt submission.
  // 每次提示词提交包含的完整序列化配置。
  submissionConfiguration: ComposerAiCapabilityConfigurationSave;
  // Persists a dialog draft and mirrors it locally after success.
  // 持久化弹窗草稿，并在成功后同步到本地。
  saveConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
}

// useComposerAiCapabilities synchronizes backend capability snapshots with editor saves and prompt submissions.
// useComposerAiCapabilities 将后端能力快照与编辑器保存、提示词提交保持同步。
// Parameters: options provides the authoritative capability snapshot and complete-save callback.
// 参数：options 提供权威能力快照与完整保存回调。
// Returns: Set/Map view state, serialized submission data, and a stable save action.
// 返回值：Set/Map 视图状态、序列化提交数据与稳定保存操作。
export function useComposerAiCapabilities(options: UseComposerAiCapabilitiesOptions): ComposerAiCapabilitiesController {
  // activeScopeKeyRef identifies the scope allowed to receive an in-flight save result.
  // activeScopeKeyRef 标识允许接收飞行中保存结果的作用域。
  const activeScopeKeyRef = React.useRef(options.scopeKey);
  activeScopeKeyRef.current = options.scopeKey;
  // disabledKinds stores capability kinds disabled by the current session snapshot or latest save.
  // disabledKinds 保存当前会话快照或最近保存所禁用的能力类型。
  const [disabledKinds, setDisabledKinds] = React.useState<Set<AiCapabilityKind>>(() => (
    defaultDisabledAiCapabilityKinds(options.capabilities)
  ));
  // disabledTargetSelectors stores target routes disabled by the current session snapshot or latest save.
  // disabledTargetSelectors 保存当前会话快照或最近保存所禁用的目标路由。
  const [disabledTargetSelectors, setDisabledTargetSelectors] = React.useState<Set<string>>(() => (
    defaultDisabledAiCapabilityTargetSelectors(options.capabilities)
  ));
  // targetBindings stores exact capability targets from the current session snapshot or latest save.
  // targetBindings 保存当前会话快照或最近保存中的能力精确目标。
  const [targetBindings, setTargetBindings] = React.useState<Map<AiCapabilityKind, string>>(() => (
    defaultAiCapabilityTargetBindings(options.capabilities)
  ));
  // capabilitySignature changes only when submission-relevant capability routing data changes.
  // capabilitySignature 仅在与提交相关的能力路由数据变化时改变。
  const capabilitySignature = React.useMemo(
    () => aiCapabilitySignature(options.capabilities),
    [options.capabilities],
  );
  // submissionConfiguration serializes local Set/Map state into the shared save and submission protocol.
  // submissionConfiguration 将本地 Set/Map 状态序列化为共享保存与提交协议。
  const submissionConfiguration = React.useMemo<ComposerAiCapabilityConfigurationSave>(() => ({
    disabledAiCapabilityKinds: Array.from(disabledKinds),
    disabledAiCapabilityTargetSelectors: Array.from(disabledTargetSelectors),
    aiCapabilityTargetBindings: serializeAiCapabilityTargetBindings(targetBindings),
  }), [disabledKinds, disabledTargetSelectors, targetBindings]);

  React.useEffect(() => {
    setDisabledKinds(defaultDisabledAiCapabilityKinds(options.capabilities));
    setDisabledTargetSelectors(defaultDisabledAiCapabilityTargetSelectors(options.capabilities));
    setTargetBindings(defaultAiCapabilityTargetBindings(options.capabilities));
  }, [capabilitySignature, options.scopeKey]);

  // saveConfiguration persists a complete dialog draft before making it the local submission source.
  // saveConfiguration 先持久化完整弹窗草稿，再将其作为本地提交来源。
  const saveConfiguration = React.useCallback(async (request: ComposerAiCapabilityConfigurationSave) => {
    // requestScopeKey binds this async save to the session visible when it started.
    // requestScopeKey 将本次异步保存绑定到启动时可见的会话。
    const requestScopeKey = options.scopeKey;
    // saved records whether the owning session accepted the complete capability configuration.
    // saved 记录所属会话是否接受完整能力配置。
    const saved = await options.onSaveConfiguration(request);
    if (!saved || activeScopeKeyRef.current !== requestScopeKey) {
      return false;
    }
    setDisabledKinds(new Set(request.disabledAiCapabilityKinds));
    setDisabledTargetSelectors(new Set(request.disabledAiCapabilityTargetSelectors));
    setTargetBindings(defaultAiCapabilityTargetBindingsFromRecord(request.aiCapabilityTargetBindings));
    return true;
  }, [options.onSaveConfiguration, options.scopeKey]);

  return {
    disabledKinds,
    disabledTargetSelectors,
    targetBindings,
    submissionConfiguration,
    saveConfiguration,
  };
}
