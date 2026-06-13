import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// conversationToolDetailPayloadCache keeps hidden tool details stable across transcript remounts.
// conversationToolDetailPayloadCache 在转录重新挂载之间稳定保存隐藏工具详情。
const conversationToolDetailPayloadCache = new Map<string, ToolDetailPayload | null>();

// conversationToolDetailPromiseCache deduplicates in-flight hidden tool detail requests.
// conversationToolDetailPromiseCache 对进行中的隐藏工具详情请求去重。
const conversationToolDetailPromiseCache = new Map<string, Promise<ToolDetailPayload | null>>();

// loadConversationToolDetail loads one hidden structured tool result detail on demand.
// loadConversationToolDetail 按需加载单条隐藏结构化工具结果详情。
async function loadConversationToolDetail(detailRef: ConversationDetailRef): Promise<ToolDetailPayload | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<ToolDetailPayload>("service_session_tool_detail", {
    sessionId: detailRef.sessionId,
    recordId: detailRef.recordId ?? null,
    toolCallId: detailRef.toolCallId ?? null,
    seq: detailRef.seq ?? null,
    messageIndex: detailRef.messageIndex ?? null,
  });
}

// conversationDetailRefCacheKey builds a stable cache key for one hidden tool detail reference.
// conversationDetailRefCacheKey 为单条隐藏工具详情引用构建稳定缓存键。
function conversationDetailRefCacheKey(detailRef: ConversationDetailRef): string {
  const locator = detailRef.recordId
    ? `record:${detailRef.recordId}`
    : detailRef.seq != null
      ? `seq:${detailRef.seq}`
      : detailRef.toolCallId
        ? `tool:${detailRef.toolCallId}`
        : detailRef.messageIndex != null
          ? `message:${detailRef.messageIndex}`
          : "unknown";
  return `${detailRef.sessionId}:${locator}`;
}

// loadCachedConversationToolDetail loads hidden tool detail once per stable reference.
// loadCachedConversationToolDetail 按稳定引用仅加载一次隐藏工具详情。
export async function loadCachedConversationToolDetail(
  detailRef: ConversationDetailRef,
): Promise<ToolDetailPayload | null> {
  const cacheKey = conversationDetailRefCacheKey(detailRef);
  if (conversationToolDetailPayloadCache.has(cacheKey)) {
    return conversationToolDetailPayloadCache.get(cacheKey) ?? null;
  }
  const existingPromise = conversationToolDetailPromiseCache.get(cacheKey);
  if (existingPromise) {
    return existingPromise;
  }
  const promise = loadConversationToolDetail(detailRef)
    .then((payload) => {
      conversationToolDetailPayloadCache.set(cacheKey, payload);
      conversationToolDetailPromiseCache.delete(cacheKey);
      return payload;
    })
    .catch((error) => {
      conversationToolDetailPromiseCache.delete(cacheKey);
      throw error;
    });
  conversationToolDetailPromiseCache.set(cacheKey, promise);
  return promise;
}
