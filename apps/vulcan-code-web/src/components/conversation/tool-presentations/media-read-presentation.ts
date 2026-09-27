import type { ToolExecutionConversationBlock } from "../conversation-tool-execution-types";
import { nativeMediaInputFileName } from "./native-media-input-preview";
import { argumentsFromMetadata, fixedToolName, fixedV1ToolEvent, objectValue, stringValue } from "./presentation-value-readers";
import type { MediaReadPresentation, ToolPresentationDisplayState } from "./types";
import { v1ModelResult } from "./v1-tool-domain-presentation";

/** Resolves `blocks` for the exact read_media tool at `state`; returns its compact receipt or null for other tools.
 * 按`state`解析精确read_media工具的`blocks`；返回紧凑回执，其他工具返回空。
 * Resource identity comes from canonical input and result metadata, never parsed from prose or media URLs.
 * 资源身份来自规范输入和结果元数据，绝不从说明文字或媒体URL反向解析。
 */
export function resolveMediaReadPresentation(
  blocks: ToolExecutionConversationBlock[],
  state: ToolPresentationDisplayState,
): MediaReadPresentation | null {
  if (fixedToolName(blocks) !== "read_media") return null;
  // V1 projection and legacy blocks both retain the exact tool arguments under metadata.arguments.
  // V1投影和旧版块都在metadata.arguments下保留精确工具参数。
  const argumentsValue = blocks.map((block) => objectValue(argumentsFromMetadata(block.rawMetadata)))
    .find((value) => value !== null);
  // An unfinished request may not have emitted its path yet.
  // 未完成构造的请求可能尚未产出路径。
  const path = stringValue(argumentsValue?.path) ?? "";
  // Canonical V1 output provides the readable filename for owned resource locators.
  // 规范V1输出为有归属的资源定位符提供可读文件名。
  const event = fixedV1ToolEvent(blocks);
  // Pending and legacy calls use the requested basename until a successful resource receipt exists.
  // 等待中及旧版调用使用请求的文件名，直到存在成功资源回执。
  const fileName = nativeMediaInputFileName(event?.output?.model_output)
    ?? path.replace(/\\/g, "/").split("/").at(-1) ?? "";
  // Failure text is read only from the formal result; successful binary envelopes are never rendered.
  // 失败文字仅从正式结果读取；成功的二进制信封绝不渲染。
  const failure = state === "failed"
    ? (event ? v1ModelResult(event.output) : blocks.find((block) => block.kind === "tool-result")?.body)
    : undefined;
  return {
    kind: "media_read",
    id: `media-read:${blocks[0].toolCallId ?? blocks[0].id}`,
    state,
    path,
    fileName,
    error: typeof failure === "string" ? failure : "",
  };
}
