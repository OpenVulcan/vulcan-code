import type { ComposerAttachmentSummary } from "./composer-skills";

export interface ConversationSummary {
  // Stable conversation identifier.
  // 稳定会话标识。
  id: string;
  // Conversation title.
  // 会话标题。
  title: string;
  // Conversation status.
  // 会话状态。
  status: string;
  // Current per-session runtime phase.
  // 当前会话级运行阶段。
  runPhase?: string;
  // Relative last-updated label.
  // 相对最后更新时间标签。
  lastUpdatedLabel?: string;
  // Raw last-updated timestamp as Unix milliseconds text.
  // 以 Unix 毫秒文本表示的原始最近更新时间。
  lastUpdatedAt?: string;
  // Whether the conversation has unread activity.
  // 会话是否存在未读活动。
  unread?: boolean;
  // Optional owning project identifier.
  // 可选所属项目标识。
  projectId?: string | null;
  // Display-only fork source marker.
  // 纯展示用分叉来源标记。
  forkSource?: ConversationForkSourceSummary | null;
}

// ConversationForkSourceSummary describes where a forked conversation was derived from.
// ConversationForkSourceSummary 描述分叉会话从哪里派生。
export interface ConversationForkSourceSummary {
  // Source project tag used to locate the original conversation.
  // 用于定位原始会话的源项目标签。
  sourceProjectTag: string;
  // Source conversation identifier for navigation.
  // 用于导航的源会话标识。
  sourceConversationId: string;
  // Source conversation title captured at fork time.
  // 分叉时捕获的源会话标题。
  sourceTitle: string;
  // Last retained message id at the fork boundary.
  // 分叉边界处最后保留的消息标识。
  anchorMessageId?: string | null;
  // Fork creation timestamp.
  // 分叉创建时间戳。
  createdAt: string;
}

// ConversationMessage describes one rendered message in the timeline.
// ConversationMessage 描述时间线中的单条渲染消息。
export interface ConversationMessage {
  // Stable message identifier.
  // 稳定消息标识。
  id: string;
  // Stable conversation identifier that owns this message.
  // 拥有该消息的稳定会话标识。
  conversationId: string;
  // Unix-millisecond creation timestamp used for turn duration labels.
  // 用于轮次耗时标签的 Unix 毫秒创建时间戳。
  createdAt?: string;
  // Message role.
  // 消息角色。
  role: "user" | "assistant" | "system";
  // Optional transcript title from the persisted session entry.
  // 来自持久化会话项的可选转录标题。
  title?: string | null;
  // Optional durable transcript record kind.
  // 可选持久化转录记录类型。
  recordKind?: string | null;
  // Message body.
  // 消息正文。
  body: string;
  // Optional structured transcript metadata.
  // 可选结构化转录元数据。
  metadata?: unknown;
  // Structured user attachments projected from transcript metadata.
  // 从转录元数据投影出的结构化用户附件。
  attachments?: ComposerAttachmentSummary[];
  // Whether the backend allows editing and replaying this user prompt.
  // 后端是否允许编辑并重新执行该用户提示词。
  editableUserPrompt?: boolean;
}

// AppendMessageBodyDelta appends streamed text to one visible conversation message.
// AppendMessageBodyDelta 向单条可见会话消息追加流式文本。
export interface AppendMessageBodyDelta {
  // Stable conversation identifier that owns the message.
  // 拥有该消息的稳定会话标识。
  conversationId: string;
  // Stable message identifier receiving the appended text.
  // 接收追加文本的稳定消息标识。
  messageId: string;
  // Text appended by this event only.
  // 本事件单独追加的文本片段。
  bodyDelta: string;
  // Full message body after this delta, preferred over local append reconstruction.
  // 本次增量后的完整消息正文，优先于本地追加复原。
  currentBody?: string | null;
  // Initial full message metadata when the row does not exist yet.
  // 当消息行尚不存在时使用的初始完整消息元数据。
  initialMessage?: ConversationMessage | null;
  // Current backend run phase after this delta.
  // 本次增量后的后端运行阶段。
  runPhase?: string | null;
}

// WorkbenchDelta describes one partial workbench update emitted on hot paths.
// WorkbenchDelta 描述热路径上发出的单条局部工作台更新。
export type WorkbenchDelta =
  | { kind: "appendMessageBody"; payload: AppendMessageBodyDelta };

// WorkbenchDeltaEnvelope carries one ordered partial workbench update.
// WorkbenchDeltaEnvelope 携带一条有序的局部工作台更新。
export interface WorkbenchDeltaEnvelope {
  // Monotonic backend generation used for diagnostics and ordering.
  // 用于诊断与排序的后端单调版本号。
  generation: number;
  // Partial workbench update payload.
  // 局部工作台更新载荷。
  delta: WorkbenchDelta;
}

// SessionAddonChangedEvent describes one persisted session addon-state namespace change.
// SessionAddonChangedEvent 描述一次已持久化的会话附属状态命名空间变化。
export interface SessionAddonChangedEvent {
  // Session identifier whose addon state changed.
  // 附属状态发生变化的会话标识。
  sessionId: string;
  // Changed addon namespace keys.
  // 已变化的附属状态命名空间键。
  namespaces: string[];
  // Latest addon document revision after the change.
  // 变化后的最新附属状态文档版本。
  revision: number;
  // Unix-millis text representing the latest addon update time.
  // 表示最近附属状态更新时间的 Unix 毫秒文本。
  updatedAt: string;
  // Optional local instance id that originated the change.
  // 发起该变化的可选本地实例标识。
  sourceInstanceId?: string | null;
}
