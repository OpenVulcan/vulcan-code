// metadataString reads a dotted path string from unknown JSON metadata.
// metadataString 从未知 JSON 元数据中读取点分路径字符串。
export function metadataString(metadata: unknown, paths: string[]): string | null {
  for (const path of paths) {
    const value = metadataValue(metadata, [path]);
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

// metadataValue reads a dotted path value from unknown JSON metadata.
// metadataValue 从未知 JSON 元数据中读取点分路径值。
export function metadataValue(metadata: unknown, paths: string[]): unknown {
  for (const path of paths) {
    const value = path.split(".").reduce<unknown>((current, segment) => {
      if (current && typeof current === "object" && segment in current) {
        return (current as Record<string, unknown>)[segment];
      }
      return undefined;
    }, metadata);
    if (value !== undefined && value !== null) {
      return value;
    }
  }
  return undefined;
}

// numberMetadata reads a safe integer from unknown JSON values.
// numberMetadata 从未知 JSON 值中读取安全整数。
export function numberMetadata(value: unknown): number {
  const numberValue = numericMetadata(value);
  return numberValue && Number.isFinite(numberValue) ? Math.max(0, Math.trunc(numberValue)) : 0;
}

// numericMetadata reads a number from unknown JSON values.
// numericMetadata 从未知 JSON 值中读取数字。
export function numericMetadata(value: unknown): number | null {
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

// stringValue returns one trimmed string from an unknown JSON value.
// stringValue 从未知 JSON 值中返回一条清理后的字符串。
export function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

// objectValue narrows an unknown JSON value into a plain object record.
// objectValue 将未知 JSON 值收窄为普通对象记录。
export function objectValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}
