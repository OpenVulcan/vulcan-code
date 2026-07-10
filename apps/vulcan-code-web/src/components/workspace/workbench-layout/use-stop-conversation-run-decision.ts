import React from "react";
import type {
  RequestStopConversationRunChoice,
  StopConversationRunAgentPromptItem,
  StopConversationRunChoice,
} from "@/components/workspace/workbench-layout/types";

// UseStopConversationRunDecisionOptions defines the conversation scope allowed to own the current dialog.
// UseStopConversationRunDecisionOptions 定义允许拥有当前弹窗的会话作用域。
export interface UseStopConversationRunDecisionOptions {
  // Conversation currently selected by the workbench, including local draft ids.
  // 工作台当前选中的会话，包括本地草稿标识。
  activeConversationId: string | null;
}

// StopConversationRunDecisionController exposes one scoped stop-confirmation transaction.
// StopConversationRunDecisionController 暴露单个作用域化停止确认事务。
export interface StopConversationRunDecisionController {
  // Running agents currently rendered by the decision dialog.
  // 当前由决策弹窗渲染的运行中智能体。
  agents: StopConversationRunAgentPromptItem[] | null;
  // Opens a scoped decision and resolves after explicit confirm, cancel, replacement, or scope exit.
  // 打开作用域化决策，并在确认、取消、替换或离开作用域后完成。
  request: RequestStopConversationRunChoice;
  // Resolves and closes the current decision with an explicit choice or cancellation.
  // 使用明确选择或取消结束并关闭当前决策。
  resolve: (choice: StopConversationRunChoice | null) => void;
}

// StopConversationRunDecisionState stores the dialog payload and its immutable target conversation.
// StopConversationRunDecisionState 保存弹窗载荷与其不可变目标会话。
interface StopConversationRunDecisionState {
  // Conversation captured by the stop preflight request.
  // 停止预检请求捕获的会话。
  conversationId: string;
  // Running agents reported for that exact conversation tree.
  // 当前精确会话树报告的运行中智能体。
  agents: StopConversationRunAgentPromptItem[];
}

// StopConversationRunDecisionResolver stores the single pending Promise continuation.
// StopConversationRunDecisionResolver 保存唯一待完成 Promise 续传。
interface StopConversationRunDecisionResolver {
  // Completes the awaiting stop flow.
  // 完成正在等待的停止流程。
  resolve: (choice: StopConversationRunChoice | null) => void;
}

// useStopConversationRunDecision owns replacement, scope-exit, and unmount semantics for the stop dialog.
// useStopConversationRunDecision 管理停止弹窗的替换、离开作用域与卸载语义。
// Parameters: options supplies the currently active conversation scope.
// 参数：options 提供当前活动会话作用域。
// Returns: dialog agents plus stable request and resolve operations.
// 返回值：弹窗智能体以及稳定的请求与解析操作。
export function useStopConversationRunDecision(
  options: UseStopConversationRunDecisionOptions,
): StopConversationRunDecisionController {
  // decision stores the currently visible scoped dialog transaction.
  // decision 保存当前可见的作用域化弹窗事务。
  const [decision, setDecision] = React.useState<StopConversationRunDecisionState | null>(null);
  // resolverRef stores the single Promise resolver allowed to await user input.
  // resolverRef 保存唯一允许等待用户输入的 Promise 解析器。
  const resolverRef = React.useRef<StopConversationRunDecisionResolver | null>(null);

  // resolve completes and closes the current decision exactly once.
  // resolve 仅一次完成并关闭当前决策。
  // Parameters: choice is the explicit stop behavior or null cancellation.
  // 参数：choice 是明确停止行为或 null 取消。
  // Returns: nothing; the pending Promise receives the choice.
  // 返回值：无；待完成 Promise 接收当前选择。
  const resolve = React.useCallback((choice: StopConversationRunChoice | null): void => {
    // pendingResolver is detached before callbacks run so reentrant requests cannot resolve it twice.
    // pendingResolver 在执行回调前被脱离，避免重入请求重复解析。
    const pendingResolver = resolverRef.current;
    resolverRef.current = null;
    setDecision(null);
    pendingResolver?.resolve(choice);
  }, []);

  // request replaces any older decision and binds the new Promise to one exact conversation.
  // request 替换任何旧决策，并将新 Promise 绑定到一个精确会话。
  // Parameters: conversationId is the immutable stop target; agents is its preflight child-agent list.
  // 参数：conversationId 是不可变停止目标；agents 是其预检子代理列表。
  // Returns: a Promise completed by explicit choice or lifecycle cancellation.
  // 返回值：由明确选择或生命周期取消完成的 Promise。
  const request = React.useCallback<RequestStopConversationRunChoice>((conversationId, agents) => new Promise((promiseResolve) => {
    // previousResolver is detached before cancellation so the new transaction remains the only owner.
    // previousResolver 在取消前被脱离，使新事务保持为唯一所有者。
    const previousResolver = resolverRef.current;
    resolverRef.current = null;
    previousResolver?.resolve(null);
    resolverRef.current = {
      resolve: promiseResolve,
    };
    setDecision({ agents, conversationId });
  }), []);

  React.useEffect(() => {
    if (!decision || decision.conversationId === options.activeConversationId) {
      return;
    }
    resolve(null);
  }, [decision, options.activeConversationId, resolve]);

  // Unmount cleanup releases the awaiting stop action without scheduling state updates on an unmounted owner.
  // 卸载清理释放正在等待的停止操作，且不在已卸载所有者上排期状态更新。
  React.useEffect(() => () => {
    // pendingResolver is the final resolver still owned when the component unmounts.
    // pendingResolver 是组件卸载时仍被所有的最终解析器。
    const pendingResolver = resolverRef.current;
    resolverRef.current = null;
    pendingResolver?.resolve(null);
  }, []);

  return {
    agents: decision?.agents ?? null,
    request,
    resolve,
  };
}
