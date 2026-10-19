import {
  createConversationEventWindowClient,
  type ConversationEventHydrationV1Response,
  type ConversationEventLiveDeltaV1,
  type ConversationEventPageV1Response,
  type ConversationEventReconnectV1Response,
  type ConversationEventWindowRpc,
} from "@/service/conversation-event-window-client";
import type { ConversationEventWindowOwner } from "@/service/conversation-event-window-store";
import type { ServiceEventHandler, ServiceUnsubscribe } from "@/service/protocol";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";

// PreviewConversationEventWindowClient serves fixed transcripts to the real event-window controller when the page
// runs in browser preview without a service, so the conversation surface renders the same way it does live.
// The fixture module loads on the first hydration only and never ships in the initial bundle.
// PreviewConversationEventWindowClient 在页面以无服务的浏览器预览运行时，向真实事件窗口控制器提供固定转录，
// 使会话界面与实时运行时以相同方式渲染。样本模块仅在首次 hydration 时加载，不进入初始包。
export class PreviewConversationEventWindowClient implements ConversationEventWindowRpc {
  // hydrate returns the session's fixture window, or an empty window for sessions without one.
  // hydrate 返回会话的样本窗口；没有样本的会话返回空窗口。
  async hydrate(sessionId: string): Promise<ConversationEventHydrationV1Response> {
    const { previewConversationWindow } = await import("@/service/preview-fixtures/preview-transcripts");
    return previewConversationWindow(sessionId);
  }

  // page never finds older history: every fixture fits in its first window.
  // page 不会找到更早历史：每份样本都容纳在首个窗口内。
  async page(owner: ConversationEventWindowOwner): Promise<ConversationEventPageV1Response> {
    const window = await this.hydrate(owner.session_id);
    return {
      session_id: owner.session_id,
      window_generation: owner.window_generation,
      events: [],
      older_before_seq: null,
      has_more: false,
      open_turn_id: window.open_turn_id,
      open_turn_start_seq: window.open_turn_start_seq,
      session_config: window.session_config,
    };
  }

  // reconnect asks for a fresh hydration; fixtures have no live frontier to resume from.
  // reconnect 要求重新 hydration；样本没有可恢复的实时前沿。
  async reconnect(owner: ConversationEventWindowOwner): Promise<ConversationEventReconnectV1Response> {
    const window = await this.hydrate(owner.session_id);
    return {
      disposition: { kind: "rehydrate_required", reason: "service_epoch_changed" },
      session_id: owner.session_id,
      window_generation: owner.window_generation,
      events: [],
      service_epoch: window.service_epoch,
      base_memory_revision: window.memory_revision,
      memory_revision: window.memory_revision,
      durable_revision: window.durable_revision,
    };
  }

  // subscribeLive never delivers: preview transcripts do not change.
  // subscribeLive 不会推送：预览转录不会变化。
  async subscribeLive(_handler: ServiceEventHandler<ConversationEventLiveDeltaV1>): Promise<ServiceUnsubscribe> {
    return () => undefined;
  }
}

// createWorkbenchConversationEventWindowClient picks the service client, or the preview client when no service runs.
// createWorkbenchConversationEventWindowClient 选择服务客户端；没有服务运行时选择预览客户端。
export function createWorkbenchConversationEventWindowClient(): ConversationEventWindowRpc {
  return hasServiceRuntime()
    ? createConversationEventWindowClient(serviceClient)
    : new PreviewConversationEventWindowClient();
}
