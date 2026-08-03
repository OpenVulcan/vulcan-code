// streamingThinkingIdentity binds one thinking block to its exact live stream and mount identity.
// streamingThinkingIdentity 将单个思考块绑定到精确的实时流与挂载身份。
// Parameters: conversationId, sourceMessageId, and blockId are the authoritative identity fields.
// 参数：conversationId、sourceMessageId 与 blockId 是权威身份字段。
// Returns: a stable compound identity when all live-stream fields are present, otherwise null.
// 返回值：实时流字段齐全时返回稳定复合身份，否则返回 null。
export function streamingThinkingIdentity(
  conversationId: string | null,
  sourceMessageId: string | undefined,
  blockId: string,
): string | null {
  if (!conversationId || !sourceMessageId) {
    return null;
  }
  return `${conversationId}\u0000${sourceMessageId}\u0000${blockId}`;
}

// shouldUseStreamingThinkingMarkdown decides whether a thinking block stays on the segmented renderer.
// shouldUseStreamingThinkingMarkdown 判断思考块是否继续使用分段渲染器。
// Parameters: liveIdentity is the current exact identity, streaming marks active input, and latchedIdentity records prior entry.
// 参数：liveIdentity 是当前精确身份，streaming 标记活动输入，latchedIdentity 记录此前是否已进入该路径。
// Returns: true only for an identified active stream or its same-mount sealed handoff.
// 返回值：仅在身份完整的活动流或同一挂载的 sealed 交接期间返回 true。
export function shouldUseStreamingThinkingMarkdown(
  liveIdentity: string | null,
  streaming: boolean,
  latchedIdentity: string | null,
): boolean {
  return liveIdentity !== null && (streaming || latchedIdentity === liveIdentity);
}
