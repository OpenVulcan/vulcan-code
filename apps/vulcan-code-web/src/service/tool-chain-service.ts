import { serviceClient } from "@/service/webui-service-runtime";

// SessionToolChainSnapshot is one session's tool_chain choice as the service resolves it.
// SessionToolChainSnapshot 是服务解析出的单个会话 tool_chain 选择。
export interface SessionToolChainSnapshot {
  // Whether the session offers tool_chain from its next request.
  // 会话从下一次请求起是否提供 tool_chain。
  enabled: boolean;
  // True while the session has no choice of its own and follows the system switch.
  // 会话没有自有选择、跟随系统开关时为 true。
  followsSystem: boolean;
  // Whether the session already sent requests, so a change breaks its cached context.
  // 会话是否已发送过请求，此时修改会打破其缓存上下文。
  requestsStarted: boolean;
}

// loadSessionToolChain reads one session's tool_chain choice.
// loadSessionToolChain 读取单个会话的 tool_chain 选择。
export async function loadSessionToolChain(conversationId: string): Promise<SessionToolChainSnapshot> {
  return serviceClient.request<SessionToolChainSnapshot>("service_session_tool_chain", { conversationId });
}

// saveSessionToolChain stores the session's own choice; it applies from the session's next request.
// saveSessionToolChain 保存会话自有选择；从会话的下一次请求起生效。
export async function saveSessionToolChain(conversationId: string, enabled: boolean): Promise<SessionToolChainSnapshot> {
  return serviceClient.request<SessionToolChainSnapshot>("service_save_session_tool_chain", { conversationId, enabled });
}
