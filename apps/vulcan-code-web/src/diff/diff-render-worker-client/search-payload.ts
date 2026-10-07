import type { DiffSerializedSearchPayload } from "./types";
// serializedSearchLineKeysCache stores ordered search keys for stable search result sets.
// serializedSearchLineKeysCache 按稳定搜索结果集合缓存有序搜索键。
export const serializedSearchLineKeysCache = new WeakMap<ReadonlySet<string>, string[]>();

// serializedSearchLineKeyStringCache stores joined search keys for repeated render cache keys.
// serializedSearchLineKeyStringCache 为重复渲染缓存键保存已拼接的搜索键。
export const serializedSearchLineKeyStringCache = new WeakMap<ReadonlySet<string>, string>();

// serializedSearchPayloadCache stores worker payloads for stable search result sets.
// serializedSearchPayloadCache 按稳定搜索结果集合缓存 Worker 载荷。
export const serializedSearchPayloadCache = new WeakMap<ReadonlySet<string>, DiffSerializedSearchPayload>();

// EMPTY_SERIALIZED_SEARCH_LINE_KEYS keeps empty search payloads allocation-free.
// EMPTY_SERIALIZED_SEARCH_LINE_KEYS 让空搜索载荷保持零分配。
export const EMPTY_SERIALIZED_SEARCH_LINE_KEYS: string[] = [];

// EMPTY_SERIALIZED_SEARCH_PAYLOAD keeps disabled or empty search requests allocation-free.
// EMPTY_SERIALIZED_SEARCH_PAYLOAD 让禁用或空搜索请求保持零分配。
export const EMPTY_SERIALIZED_SEARCH_PAYLOAD: DiffSerializedSearchPayload = {};
// searchStateCacheKey builds a stable cache key fragment from the serialized search signature.
// searchStateCacheKey 基于已序列化搜索签名构建稳定缓存 key 片段。
export function searchStateCacheKey(searchSignature?: string, activeSearchLineKey?: string | null): string {
  return `${activeSearchLineKey ?? ""}\u0001${searchSignature ?? ""}`;
}

// serializedSearchPayloadForRequest returns worker-ready search keys and signature together.
// serializedSearchPayloadForRequest 同时返回可发送给 Worker 的搜索 key 与签名。
export function serializedSearchPayloadForRequest(searchLineKeys?: ReadonlySet<string>): DiffSerializedSearchPayload {
  if (!searchLineKeys) {
    return EMPTY_SERIALIZED_SEARCH_PAYLOAD;
  }
  const cached = serializedSearchPayloadCache.get(searchLineKeys);
  if (cached) {
    return cached;
  }
  const { keys, signature } = serializedSearchPayloadParts(searchLineKeys);
  if (!keys.length) {
    serializedSearchPayloadCache.set(searchLineKeys, EMPTY_SERIALIZED_SEARCH_PAYLOAD);
    return EMPTY_SERIALIZED_SEARCH_PAYLOAD;
  }
  const payload = {
    keys,
    signature: signature || undefined,
  };
  serializedSearchPayloadCache.set(searchLineKeys, payload);
  return payload;
}

// serializedSearchPayloadParts builds ordered keys and a cache signature in one pass.
// serializedSearchPayloadParts 单次遍历生成有序 key 与缓存签名。
function serializedSearchPayloadParts(searchLineKeys: ReadonlySet<string>): Required<DiffSerializedSearchPayload> {
  if (!searchLineKeys.size) {
    serializedSearchLineKeysCache.set(searchLineKeys, EMPTY_SERIALIZED_SEARCH_LINE_KEYS);
    serializedSearchLineKeyStringCache.set(searchLineKeys, "");
    return { keys: EMPTY_SERIALIZED_SEARCH_LINE_KEYS, signature: "" };
  }
  const cachedKeys = serializedSearchLineKeysCache.get(searchLineKeys);
  const cachedSignature = serializedSearchLineKeyStringCache.get(searchLineKeys);
  if (cachedKeys && cachedSignature != null) {
    return { keys: cachedKeys, signature: cachedSignature };
  }
  if (cachedKeys) {
    const signature = serializedSearchLineKeyStringFromKeys(cachedKeys);
    serializedSearchLineKeyStringCache.set(searchLineKeys, signature);
    return { keys: cachedKeys, signature };
  }
  const keys: string[] = new Array(searchLineKeys.size);
  let index = 0;
  for (const key of searchLineKeys) {
    keys[index] = key;
    index += 1;
  }
  // signature joins once after the key array is complete to avoid per-key intermediate strings.
  // signature 在 key 数组完成后一次性拼接，避免每个 key 都产生中间字符串。
  const signature = keys.join("|");
  serializedSearchLineKeysCache.set(searchLineKeys, keys);
  serializedSearchLineKeyStringCache.set(searchLineKeys, signature);
  return { keys, signature };
}

// serializedSearchLineKeyStringFromKeys joins cached keys without rebuilding the key array.
// serializedSearchLineKeyStringFromKeys 复用已缓存 key 数组生成签名。
export function serializedSearchLineKeyStringFromKeys(keys: readonly string[]): string {
  return keys.join("|");
}

