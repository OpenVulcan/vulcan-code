import {
  normalizeGeneratedMediaKind,
  type GeneratedMediaAttachment,
} from "@/components/media/generated-media";
import type { ToolDetailPayload } from "@/components/conversation/conversation-detail";
import {
  metadataValue,
  objectValue,
  stringValue,
} from "@/components/conversation/conversation-metadata";

// generatedMediaAttachmentsFromToolDetailPayload extracts media outputs from the hidden full tool detail row.
// generatedMediaAttachmentsFromToolDetailPayload 从隐藏完整工具详情行中提取媒体输出。
export function generatedMediaAttachmentsFromToolDetailPayload(payload: ToolDetailPayload): GeneratedMediaAttachment[] {
  const metadataAttachments = generatedMediaAttachmentsFromMetadata(payload.metadata);
  if (metadataAttachments.length > 0) {
    return metadataAttachments;
  }
  const parsedBody = parseJsonObject(payload.body);
  return generatedMediaAttachmentsFromPayload(parsedBody);
}

// generatedMediaAttachmentsFromMetadata extracts host-generated media outputs from nested or flattened tool metadata.
// generatedMediaAttachmentsFromMetadata 从嵌套或扁平工具元数据中提取宿主生成媒体输出。
export function generatedMediaAttachmentsFromMetadata(metadata: unknown): GeneratedMediaAttachment[] {
  const hostResult = resolvedHostResult(metadata);
  if (hostResult?.kind === "generated_attachments") {
    return generatedMediaAttachmentsFromPayload(hostResult.payload);
  }
  const directAttachments = generatedMediaAttachmentsFromPayload(metadata);
  if (directAttachments.length > 0) {
    return directAttachments;
  }
  return generatedMediaAttachmentsFromPayload(metadataValue(metadata, ["display"]));
}

// generatedMediaAttachmentsFromPayload maps a generated-capability JSON payload to WebUI attachments.
// generatedMediaAttachmentsFromPayload 将生成能力 JSON 载荷映射为 WebUI 附件。
function generatedMediaAttachmentsFromPayload(payload: unknown): GeneratedMediaAttachment[] {
  const object = objectValue(payload);
  const outputs = Array.isArray(object?.outputs) ? object.outputs : [];
  return outputs
    .map((output): GeneratedMediaAttachment | null => {
      if (!output || typeof output !== "object") {
        return null;
      }
      const object = output as Record<string, unknown>;
      const rawKind = typeof object.kind === "string" ? object.kind.trim().toLowerCase() : "";
      const kind = normalizeGeneratedMediaKind(rawKind);
      const localPath = stringValue(object.local_path) || stringValue(object.source_path);
      if (!kind || !localPath) {
        return null;
      }
      return {
        kind,
        fileName: stringValue(object.file_name) || localPath.split(/[\\/]/).pop() || localPath,
        localPath,
        previewLocalPath: stringValue(object.preview_local_path) || undefined,
        mimeType: stringValue(object.mime_type) || undefined,
      };
    })
    .filter((output): output is GeneratedMediaAttachment => output !== null);
}

// parseJsonObject parses one JSON object string without treating plain tool text as structured data.
// parseJsonObject 解析单个 JSON 对象字符串，并避免把普通工具文本当成结构化数据。
function parseJsonObject(source: string): Record<string, unknown> | null {
  const trimmed = source.trim();
  if (!trimmed.startsWith("{") || !trimmed.endsWith("}")) {
    return null;
  }
  try {
    return objectValue(JSON.parse(trimmed));
  } catch {
    return null;
  }
}

// resolvedHostResult returns a normalized generated/tool host-result object from known runtime metadata shapes.
// resolvedHostResult 从已知运行时元数据形态中返回规范化的工具宿主结果对象。
function resolvedHostResult(metadata: unknown): { kind?: string; payload?: Record<string, unknown> } | null {
  const candidate = metadataValue(metadata, ["host_result", "hostResult"]) ?? metadata;
  if (!candidate || typeof candidate !== "object") {
    return null;
  }
  const object = candidate as Record<string, unknown>;
  const kind = stringValue(object.kind);
  const payload = object.payload && typeof object.payload === "object"
    ? object.payload as Record<string, unknown>
    : undefined;
  if (!kind || !payload) {
    return null;
  }
  return { kind, payload };
}
