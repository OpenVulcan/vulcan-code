import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type {
  ProviderToolExtractPresentation,
  ProviderToolExtractPresentationFailure,
  ProviderToolExtractPresentationResult,
  ProviderToolSearchPresentation,
  ProviderToolSearchPresentationResult,
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
