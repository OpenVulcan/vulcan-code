import React from "react";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { conversationDetailRefCacheKey } from "./conversation-tool-execution-model";

// conversationToolDetailMediaCache keeps generated media cards stable across transcript remounts.
// conversationToolDetailMediaCache 在转录重新挂载之间稳定保存生成媒体卡片。
const conversationToolDetailMediaCache = new Map<string, GeneratedMediaAttachment[]>();

// useToolDetailMediaAttachments hydrates generated media from hidden tool details when the visible call row is compact.
// useToolDetailMediaAttachments 在可见工具调用行较紧凑时，从隐藏工具详情中水合生成媒体。
export function useToolDetailMediaAttachments(
  detailRef: ConversationDetailRef | undefined,
  enabled: boolean,
  loadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>,
  extractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[],
): GeneratedMediaAttachment[] {
  const detailCacheKey = detailRef ? conversationDetailRefCacheKey(detailRef) : null;
  const [attachments, setAttachments] = React.useState<GeneratedMediaAttachment[]>(
    () => detailCacheKey ? conversationToolDetailMediaCache.get(detailCacheKey) ?? [] : [],
  );
  React.useEffect(() => {
    const cachedAttachments = detailCacheKey
      ? conversationToolDetailMediaCache.get(detailCacheKey)
      : undefined;
    setAttachments(cachedAttachments ?? []);
    if (
      !enabled
      || !detailRef
      || (!detailRef.recordId && !detailRef.toolCallId && detailRef.seq == null && detailRef.messageIndex == null)
    ) {
      return;
    }
    let cancelled = false;
    void loadToolDetail(detailRef)
      .then((payload) => {
        if (!cancelled && payload) {
          const nextAttachments = extractGeneratedMediaAttachments(payload);
          if (detailCacheKey) {
            conversationToolDetailMediaCache.set(detailCacheKey, nextAttachments);
          }
          setAttachments(nextAttachments);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAttachments([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    detailCacheKey,
    detailRef?.messageIndex,
    detailRef?.recordId,
    detailRef?.seq,
    detailRef?.sessionId,
    detailRef?.toolCallId,
    enabled,
    extractGeneratedMediaAttachments,
    loadToolDetail,
  ]);
  return attachments;
}

// toolExecutionMayHaveGeneratedMedia gates detail hydration to media-producing tools.
// toolExecutionMayHaveGeneratedMedia 将详情水合限制在会生成媒体的工具上。
export function toolExecutionMayHaveGeneratedMedia(toolName: string | null): boolean {
  const normalized = toolName?.trim().toLowerCase();
  return normalized === "image_generation"
    || normalized === "video_generation"
    || normalized === "speech_synthesis"
    || normalized === "music_generation";
}

// mergeGeneratedMediaAttachments combines visible and hydrated media without duplicate cards.
// mergeGeneratedMediaAttachments 合并可见与水合媒体，并避免重复卡片。
export function mergeGeneratedMediaAttachments(
  visibleAttachments: GeneratedMediaAttachment[],
  hydratedAttachments: GeneratedMediaAttachment[],
): GeneratedMediaAttachment[] {
  const seen = new Set<string>();
  const merged: GeneratedMediaAttachment[] = [];
  for (const attachment of [...visibleAttachments, ...hydratedAttachments]) {
    const key = `${attachment.kind}:${attachment.localPath}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    merged.push(attachment);
  }
  return merged;
}
