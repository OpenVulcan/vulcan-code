// NotificationCategory identifies one independently configurable notification class.
// NotificationCategory 标识一种可独立配置的通知类别。
export type NotificationCategory = "conversation_completed" | "conversation_failed";

// NotificationEndpointKind reserves the stable local-versus-remote delivery boundary.
// NotificationEndpointKind 预留稳定的本地与远程投递边界。
export type NotificationEndpointKind = "local" | "remote";

// NotificationDeliveryStatus describes one endpoint delivery result without throwing into conversation flow.
// NotificationDeliveryStatus 描述单个端点投递结果，且不会向会话流程抛出错误。
export type NotificationDeliveryStatus =
  | "delivered"
  | "suppressed"
  | "permissionDenied"
  | "unsupported"
  | "failed";

// NotificationRouteV1 describes a platform-neutral conversation navigation target.
// NotificationRouteV1 描述一个与平台无关的会话导航目标。
export interface NotificationRouteV1 {
  // kind identifies the stable route contract.
  // kind 标识稳定路由契约。
  kind: "conversation";
  // projectId scopes the route when the service resolved an owning project.
  // projectId 在服务端解析到所属项目时限定路由范围。
  projectId: string | null;
  // conversationId identifies the conversation to open.
  // conversationId 标识需要打开的会话。
  conversationId: string;
}

// NotificationIntentV1 is the serializable, versioned input shared by delivery endpoints.
// NotificationIntentV1 是由各投递端点共享的可序列化版本化输入。
export interface NotificationIntentV1 {
  // version identifies the serialized intent schema.
  // version 标识序列化意图结构版本。
  version: 1;
  // intentId uniquely identifies this derived notification intent.
  // intentId 唯一标识本次派生通知意图。
  intentId: string;
  // sourceEventId links delivery back to the lifecycle event.
  // sourceEventId 将投递关联回生命周期事件。
  sourceEventId: string;
  // category selects independent user preferences.
  // category 选择独立用户偏好。
  category: NotificationCategory;
  // conversationId identifies the affected conversation.
  // conversationId 标识受影响的会话。
  conversationId: string;
  // projectId identifies the owning project when available.
  // projectId 标识可用时的所属项目。
  projectId: string | null;
  // dedupeKey collapses exact repeated delivery attempts.
  // dedupeKey 折叠完全重复的投递尝试。
  dedupeKey: string;
  // collapseKey groups successive state updates for one conversation.
  // collapseKey 对同一会话的连续状态更新进行分组。
  collapseKey: string;
  // priority distinguishes normal completion from failure attention.
  // priority 区分普通完成与失败提醒。
  priority: "normal" | "high";
  // expiresAt is a Unix-millisecond timestamp text for stale-delivery rejection.
  // expiresAt 是用于拒绝过期投递的 Unix 毫秒时间戳文本。
  expiresAt: string;
  // route carries the platform-neutral activation target.
  // route 携带与平台无关的激活目标。
  route: NotificationRouteV1;
  // privacyLevel declares whether the intent includes a conversation title.
  // privacyLevel 声明意图是否包含会话标题。
  privacyLevel: "generic" | "conversation_title";
  // localizationKey identifies the message template without embedding platform text.
  // localizationKey 标识消息模板，且不嵌入平台文案。
  localizationKey: "notifications.completed" | "notifications.failed";
  // localizationArgs carries template values permitted by the active privacy preference.
  // localizationArgs 携带当前隐私偏好允许的模板值。
  localizationArgs: Record<string, string>;
}

// NotificationPreferencesV1 stores per-client local notification choices.
// NotificationPreferencesV1 保存每个客户端的本地通知选择。
export interface NotificationPreferencesV1 {
  // version identifies the local preference schema.
  // version 标识本地偏好结构版本。
  version: 1;
  // channels controls delivery transports independently.
  // channels 独立控制各投递通道。
  channels: {
    // local controls the current shell endpoint.
    // local 控制当前壳端点。
    local: {
      // enabled turns local shell delivery on or off.
      // enabled 开启或关闭本地壳投递。
      enabled: boolean;
    };
  };
  // categories controls lifecycle classes independently.
  // categories 独立控制生命周期类别。
  categories: {
    // conversationCompleted controls successful completion notifications.
    // conversationCompleted 控制成功完成通知。
    conversationCompleted: boolean;
    // conversationFailed controls failed run notifications.
    // conversationFailed 控制失败运行通知。
    conversationFailed: boolean;
  };
  // privacy controls information shown outside the application surface.
  // privacy 控制应用界面之外显示的信息。
  privacy: {
    // showConversationTitle permits the conversation title in notification text.
    // showConversationTitle 允许在通知文案中显示会话标题。
    showConversationTitle: boolean;
  };
}

// NotificationPresentationContext describes the exact transcript surface visible in this client.
// NotificationPresentationContext 描述当前客户端真实可见的转录界面。
export interface NotificationPresentationContext {
  // conversationViewActive indicates whether a real persisted transcript is the active surface.
  // conversationViewActive 表示真实持久化转录是否为当前活动界面。
  conversationViewActive: boolean;
  // visibleConversationId identifies that exact transcript, including a visible subagent transcript.
  // visibleConversationId 标识该精确转录，包括当前可见的子代理转录。
  visibleConversationId: string | null;
}

// NotificationDeliveryResult records one endpoint outcome for diagnostics and tests.
// NotificationDeliveryResult 记录单个端点结果，供诊断与测试使用。
export interface NotificationDeliveryResult {
  // endpointKind identifies the delivery boundary.
  // endpointKind 标识投递边界。
  endpointKind: NotificationEndpointKind;
  // status is the normalized endpoint outcome.
  // status 是标准化后的端点结果。
  status: NotificationDeliveryStatus;
  // reason is a stable diagnostic code.
  // reason 是稳定的诊断代码。
  reason: string;
}

// NotificationText contains localized presentation text for one local shell request.
// NotificationText 包含单个本地壳请求的本地化展示文案。
export interface NotificationText {
  // title is the localized system notification title.
  // title 是本地化系统通知标题。
  title: string;
  // body is the localized system notification body.
  // body 是本地化系统通知正文。
  body: string;
}

// NotificationTextResolver localizes one platform-neutral intent at the client boundary.
// NotificationTextResolver 在客户端边界本地化一个与平台无关的意图。
export type NotificationTextResolver = (intent: NotificationIntentV1) => NotificationText;

// NotificationDeliveryEndpoint defines one independently executable notification transport.
// NotificationDeliveryEndpoint 定义一个可独立执行的通知投递通道。
export interface NotificationDeliveryEndpoint {
  // kind identifies whether the endpoint is local or remote.
  // kind 标识端点属于本地还是远程。
  readonly kind: NotificationEndpointKind;
  // deliver evaluates and executes one intent against current client context.
  // deliver 根据当前客户端上下文评估并执行一个意图。
  deliver(
    // intent is the versioned platform-neutral delivery input.
    // intent 是版本化且与平台无关的投递输入。
    intent: NotificationIntentV1,
    // preferences contains the current client-local choices.
    // preferences 包含当前客户端本地选择。
    preferences: NotificationPreferencesV1,
    // presentation contains the exact current transcript surface.
    // presentation 包含当前精确转录界面。
    presentation: NotificationPresentationContext,
    // resolveText localizes presentation text at delivery time.
    // resolveText 在投递时本地化展示文案。
    resolveText: NotificationTextResolver,
  ): Promise<NotificationDeliveryResult>;
}
