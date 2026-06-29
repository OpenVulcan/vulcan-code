import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";

// argumentsFromMetadata returns the raw tool arguments from normalized metadata.
// argumentsFromMetadata 从规范化 metadata 中返回原始工具参数。
export function argumentsFromMetadata(metadata: unknown): unknown {
  return metadataValue(metadata, ["arguments"]);
}

// normalizeToolName converts tool names into the canonical comparison form.
// normalizeToolName 将工具名称转换为规范比较格式。
export function normalizeToolName(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "_");
}

// fixedToolName reads the tool name from the normalized visible WebUI record only.
// fixedToolName 仅从 WebUI 已规范化的可见记录读取工具名。
export function fixedToolName(blocks: ToolExecutionConversationBlock[]): string | null {
  return blocks
    .map((block) => block.toolName ?? metadataString(block.rawMetadata, ["tool_name"]))
    .find(Boolean)
    ?? null;
}

// fixedHostKind reads the top-level host result kind preserved by the runtime metadata bridge.
// fixedHostKind 读取运行时 metadata 桥保留的顶层宿主结果类型。
export function fixedHostKind(blocks: ToolExecutionConversationBlock[]): string | null {
  return blocks
    .map((block) => metadataString(block.rawMetadata, ["kind", "host_result.kind", "hostResult.kind"]))
    .find(Boolean)
    ?? null;
}

// fixedHostPayload reads the stable host-result payload from the WebUI metadata bridge.
// fixedHostPayload 从 WebUI metadata 桥读取稳定的宿主结果载荷。
export function fixedHostPayload(blocks: ToolExecutionConversationBlock[]): Record<string, unknown> | null {
  return blocks
    .map((block) => objectValue(metadataValue(block.rawMetadata, ["host_result.payload", "hostResult.payload", "payload"])))
    .find(Boolean)
    ?? null;
}

// metadataString returns the first non-empty string at one of the provided metadata paths.
// metadataString 返回指定 metadata 路径中的首个非空字符串。
export function metadataString(metadata: unknown, paths: string[]): string | null {
  for (const path of paths) {
    const value = stringValue(metadataValue(metadata, [path]));
    if (value) {
      return value;
    }
  }
  return null;
}

// metadataValue returns the value at the first existing dotted path.
// metadataValue 返回首个存在的点分路径值。
export function metadataValue(metadata: unknown, paths: string[]): unknown {
  for (const candidate of paths) {
    const segments = candidate.split(".");
    let current: unknown = metadata;
    let found = true;
    for (const segment of segments) {
      if (!current || typeof current !== "object" || !(segment in current)) {
        found = false;
        break;
      }
      current = (current as Record<string, unknown>)[segment];
    }
    if (found) {
      return current;
    }
  }
  return null;
}

// objectValue narrows unknown values to plain records.
// objectValue 将未知值收窄为普通对象记录。
export function objectValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

// arrayValue narrows unknown values to arrays.
// arrayValue 将未知值收窄为数组。
export function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

// stringValue returns trimmed strings and null for blank or non-string values.
// stringValue 返回裁剪后的字符串，空白或非字符串值返回 null。
export function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

// textValue returns raw string values without trimming file content.
// textValue 返回不裁剪文件内容的原始字符串值。
export function textValue(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

// numberValue normalizes numeric JSON values or numeric strings.
// numberValue 规范化 JSON 数值或数字字符串。
export function numberValue(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return Math.trunc(value);
  }
  if (typeof value === "string" && /^-?\d+$/.test(value.trim())) {
    return Number.parseInt(value.trim(), 10);
  }
  return null;
}

// compactHash creates a small deterministic hash for rows without backend ids.
// compactHash 为没有后端标识的行创建短小稳定哈希。
export function compactHash(source: string): string {
  let hash = 0;
  for (let index = 0; index < source.length; index += 1) {
    hash = Math.imul(31, hash) + source.charCodeAt(index) | 0;
  }
  return Math.abs(hash).toString(36);
}
