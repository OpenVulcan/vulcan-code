import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type {
  ProviderToolExtractPresentation,
  ProviderToolExtractPresentationFailure,
  ProviderToolExtractPresentationResult,
  ProviderToolSearchPresentation,
  ProviderToolSearchPresentationResult,
  ProviderToolStructuredPresentation,
  ProviderToolTextPresentation,
  ToolPresentationDisplayState,
} from "./types";
import {
  arrayValue,
  compactHash,
  fixedHostKind,
  fixedHostPayload,
  numberValue,
  objectValue,
  stringValue,
  textValue,
} from "./presentation-value-readers";

// resolveProviderToolTextPresentation projects the unified provider text host result.
// resolveProviderToolTextPresentation 投影统一供应商文本宿主结果。
//
// Parameters:
// `blocks` are the merged tool records and `displayState` is their normalized state.
// 参数：
// `blocks` 是合并后的工具记录，`displayState` 是其规范化状态。
//
// Returns:
// A strict text presentation only for the provider_tool_output/text contract.
// 返回值：
// 仅为 provider_tool_output/text 契约返回严格文本展示。
export function resolveProviderToolTextPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
): ProviderToolTextPresentation | null {
  if (fixedHostKind(blocks) !== "provider_tool_output") {
    return null;
  }
  const payload = fixedHostPayload(blocks);
  if (stringValue(payload?.kind) !== "text") {
    return null;
  }
  const text = textValue(payload?.text);
  if (text === null) {
    return null;
  }
  const sourceId = blocks.find((block) => block.toolCallId)?.toolCallId
    ?? blocks[0]?.id
    ?? compactHash(text);
  return {
    id: `provider-text:${sourceId}`,
    kind: "provider_tool_text",
    state: displayState,
    text,
  };
}

// resolveProviderToolStructuredPresentation projects closed embedding, rerank, and composite host results.
// resolveProviderToolStructuredPresentation 投影封闭的向量、重排与复合宿主结果。
//
// Parameters:
// `blocks` are the merged tool records and `displayState` is their normalized state.
// 参数：
// `blocks` 是合并后的工具记录，`displayState` 是其规范化状态。
//
// Returns:
// A strict structured presentation only for one of the three registered unified output contracts.
// 返回值：
// 仅为三种已注册统一输出契约之一返回严格结构化展示。
export function resolveProviderToolStructuredPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
): ProviderToolStructuredPresentation | null {
  if (fixedHostKind(blocks) !== "provider_tool_output") {
    return null;
  }
  const hostPayload = fixedHostPayload(blocks);
  const outputKind = stringValue(hostPayload?.kind);
  if (outputKind !== "embedding" && outputKind !== "rerank" && outputKind !== "composite") {
    return null;
  }
  const payload = objectValue(hostPayload?.[outputKind]);
  if (!payload) {
    return null;
  }
  const projection = providerToolStructuredProjection(outputKind, payload);
  if (!projection) {
    return null;
  }
  const sourceId = blocks.find((block) => block.toolCallId)?.toolCallId
    ?? blocks[0]?.id
    ?? compactHash(JSON.stringify(payload));
  return {
    id: `provider-${outputKind}:${sourceId}`,
    kind: "provider_tool_structured",
    state: displayState,
    outputKind,
    itemCount: projection.itemCount,
    payload: projection.preview,
  };
}

// providerToolStructuredProjection validates the closed shape and builds a bounded card preview.
// providerToolStructuredProjection 验证封闭形状并构建有界卡片预览。
function providerToolStructuredProjection(
  outputKind: ProviderToolStructuredPresentation["outputKind"],
  payload: Record<string, unknown>,
): { itemCount: number; preview: Record<string, unknown> } | null {
  if (outputKind === "embedding") {
    if (!Array.isArray(payload.vectors)) {
      return null;
    }
    return {
      itemCount: payload.vectors.length,
      preview: {
        model: stringValue(payload.model),
        vector_count: payload.vectors.length,
        vectors: payload.vectors.slice(0, 20).map((value) => {
          const vector = objectValue(value);
          return {
            index: numberValue(vector?.index),
            dimensions: arrayValue(vector?.values).length,
          };
        }),
        truncated: payload.vectors.length > 20,
      },
    };
  }
  if (outputKind === "rerank") {
    if (!Array.isArray(payload.results)) {
      return null;
    }
    return {
      itemCount: payload.results.length,
      preview: {
        model: stringValue(payload.model),
        result_count: payload.results.length,
        results: payload.results.slice(0, 50).map((value) => {
          const result = objectValue(value);
          return {
            index: numberValue(result?.index),
            score: finiteNumberValue(result?.score),
            document: boundedTextValue(result?.document, 500),
          };
        }),
        truncated: payload.results.length > 50,
      },
    };
  }
  const collections = ["text", "resources", "embeddings", "reranks", "searches", "extracts"];
  if (!collections.every((key) => Array.isArray(payload[key]))) {
    return null;
  }
  const counts = Object.fromEntries(
    collections.map((key) => [key, (payload[key] as unknown[]).length]),
  );
  return {
    itemCount: Object.values(counts).reduce((total, count) => total + count, 0),
    preview: {
      counts,
      text_preview: (payload.text as unknown[])
        .slice(0, 10)
        .map((value) => boundedTextValue(value, 500)),
      resources: (payload.resources as unknown[]).slice(0, 20),
      truncated: collections.some((key) => (payload[key] as unknown[]).length > 20),
    },
  };
}

// finiteNumberValue returns only finite floating-point values from a normalized JSON payload.
// finiteNumberValue 仅从规范化 JSON 载荷返回有限浮点数值。
function finiteNumberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

// boundedTextValue returns one exact string preview within a fixed character limit.
// boundedTextValue 返回一个位于固定字符上限内的精确字符串预览。
function boundedTextValue(value: unknown, maximumCharacters: number): string | null {
  const text = textValue(value);
  if (text === null || text.length <= maximumCharacters) {
    return text;
  }
  return `${text.slice(0, maximumCharacters)}…`;
}

// resolveProviderToolExtractPresentation projects the unified provider extraction host result.
// resolveProviderToolExtractPresentation 投影统一供应商网页提取宿主结果。
//
// Parameters:
// `blocks` are the merged tool records and `displayState` is their normalized state.
// 参数：
// `blocks` 是合并后的工具记录，`displayState` 是其规范化状态。
//
// Returns:
// A strict extraction presentation only for the provider_tool_output/extract contract.
// 返回值：
// 仅为 provider_tool_output/extract 契约返回严格网页提取展示。
export function resolveProviderToolExtractPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
): ProviderToolExtractPresentation | null {
  if (fixedHostKind(blocks) !== "provider_tool_output") {
    return null;
  }
  const payload = fixedHostPayload(blocks);
  if (stringValue(payload?.kind) !== "extract") {
    return null;
  }
  const extract = objectValue(payload?.extract);
  const results = arrayValue(extract?.results)
    .map(providerToolExtractResult)
    .filter((result): result is ProviderToolExtractPresentationResult => result !== null);
  const failures = arrayValue(extract?.failures)
    .map(providerToolExtractFailure)
    .filter((failure): failure is ProviderToolExtractPresentationFailure => failure !== null);
  const sourceId = blocks.find((block) => block.toolCallId)?.toolCallId
    ?? blocks[0]?.id
    ?? compactHash(JSON.stringify({ failures, results }));
  return {
    id: `provider-extract:${sourceId}`,
    kind: "provider_tool_extract",
    failures,
    results,
    state: displayState,
  };
}

// providerToolExtractResult validates one normalized extracted page.
// providerToolExtractResult 验证一个归一化提取页面。
function providerToolExtractResult(value: unknown): ProviderToolExtractPresentationResult | null {
  const result = objectValue(value);
  const url = stringValue(result?.url);
  const content = textValue(result?.content);
  if (!url || content === null) {
    return null;
  }
  return {
    content,
    favicon: stringValue(result?.favicon),
    images: arrayValue(result?.images)
      .map(stringValue)
      .filter((image): image is string => image !== null),
    url,
  };
}

// providerToolExtractFailure validates one normalized extraction failure.
// providerToolExtractFailure 验证一个归一化提取失败。
function providerToolExtractFailure(value: unknown): ProviderToolExtractPresentationFailure | null {
  const failure = objectValue(value);
  const url = stringValue(failure?.url);
  const error = stringValue(failure?.error);
  return url && error ? { error, url } : null;
}

// resolveProviderToolSearchPresentation projects the unified provider search host result.
// resolveProviderToolSearchPresentation 投影统一供应商搜索宿主结果。
//
// Parameters:
// `blocks` are the merged tool records and `displayState` is their normalized state.
// 参数：
// `blocks` 是合并后的工具记录，`displayState` 是其规范化状态。
//
// Returns:
// A strict search presentation only for the provider_tool_output/search contract.
// 返回值：
// 仅为 provider_tool_output/search 契约返回严格搜索展示。
export function resolveProviderToolSearchPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
): ProviderToolSearchPresentation | null {
  if (fixedHostKind(blocks) !== "provider_tool_output") {
    return null;
  }
  const payload = fixedHostPayload(blocks);
  if (stringValue(payload?.kind) !== "search") {
    return null;
  }
  const search = objectValue(payload?.search);
  const query = stringValue(search?.query);
  if (!query) {
    return null;
  }
  const results = arrayValue(search?.results)
    .map(providerToolSearchResult)
    .filter((result): result is ProviderToolSearchPresentationResult => result !== null);
  const sourceId = blocks.find((block) => block.toolCallId)?.toolCallId
    ?? blocks[0]?.id
    ?? compactHash(query);
  return {
    id: `provider-search:${sourceId}`,
    kind: "provider_tool_search",
    query,
    results,
    state: displayState,
  };
}

// providerToolSearchResult validates one normalized provider search row.
// providerToolSearchResult 校验一条归一化供应商搜索行。
//
// Parameters:
// `value` is one item from the fixed normalized results array.
// 参数：
// `value` 是固定归一化结果数组中的一项。
//
// Returns:
// A display-safe row or null when the unified contract is malformed.
// 返回值：
// 返回展示安全的行；统一契约畸形时返回 null。
function providerToolSearchResult(value: unknown): ProviderToolSearchPresentationResult | null {
  const result = objectValue(value);
  const rank = numberValue(result?.rank);
  const title = stringValue(result?.title);
  const url = stringValue(result?.url);
  const sourceDomain = stringValue(result?.source_domain);
  const snippet = stringValue(result?.snippet);
  if (rank === null || rank < 1 || !title || !url || !sourceDomain || !snippet) {
    return null;
  }
  return { rank, snippet, sourceDomain, title, url };
}
