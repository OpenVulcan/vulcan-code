import { v1ModelResult } from "@/components/conversation/tool-presentations/v1-tool-domain-presentation";
import { numberOf, objectOf, textOf, type ToolCall } from "../tool-call";

// WorkFormat identifies the source format of a supported office operation.
// WorkFormat 标识支持的办公操作对应的源格式。
type WorkFormat = "pdf" | "xlsx" | "docx" | "pptx";

// WorkCount is a numeric fact returned by the corresponding skill entry.
// WorkCount 是由对应技能入口返回的数值事实。
type WorkCount = "page_count" | "field_count" | "fields_written" | "sheet_count" | "cells_written"
  | "block_count" | "comment_count" | "paragraphs_appended" | "slide_count" | "slides_appended"
  | "slides_removed" | "matches_replaced";

// WorkSpec binds a known operation to its format, presentation family, and result facts.
// WorkSpec 将已知操作绑定到格式、展示分类和结果事实。
interface WorkSpec {
  // Source or newly created file format.
  // 源文件或新建文件的格式。
  format: WorkFormat;
  // Writes publish a new output path after a successful call.
  // 写操作成功后发布新输出路径。
  family: "read" | "edit";
  // Ordered counts that are meaningful for this operation.
  // 此操作需要展示的计数及其顺序。
  counts: readonly WorkCount[];
}

// OPERATIONS is keyed by the documented entry names in the Vulcan Work manifest.
// OPERATIONS 以 Vulcan Work 清单声明的入口名称为键。
const OPERATIONS = {
  "pdf-inspect": { format: "pdf", family: "read", counts: ["page_count", "field_count"] },
  "pdf-extract-text": { format: "pdf", family: "read", counts: ["page_count"] },
  "pdf-ocr-pages": { format: "pdf", family: "read", counts: ["page_count"] },
  "pdf-compose": { format: "pdf", family: "edit", counts: ["page_count"] },
  "pdf-fill-form": { format: "pdf", family: "edit", counts: ["fields_written"] },
  "xlsx-list-sheets": { format: "xlsx", family: "read", counts: ["sheet_count"] },
  "xlsx-read-range": { format: "xlsx", family: "read", counts: [] },
  "xlsx-write-cells": { format: "xlsx", family: "edit", counts: ["cells_written"] },
  "docx-read-blocks": { format: "docx", family: "read", counts: ["block_count"] },
  "docx-list-comments": { format: "docx", family: "read", counts: ["comment_count"] },
  "docx-append-paragraphs": { format: "docx", family: "edit", counts: ["paragraphs_appended"] },
  "docx-replace-text": { format: "docx", family: "edit", counts: ["matches_replaced"] },
  "pptx-extract-text": { format: "pptx", family: "read", counts: ["slide_count"] },
  "pptx-create": { format: "pptx", family: "edit", counts: ["slide_count"] },
  "pptx-append-slides": { format: "pptx", family: "edit", counts: ["slides_appended", "slide_count"] },
  "pptx-remove-slides": { format: "pptx", family: "edit", counts: ["slides_removed", "slide_count"] },
  "pptx-replace-text": { format: "pptx", family: "edit", counts: ["matches_replaced"] },
  "pptx-export-preview": { format: "pptx", family: "edit", counts: [] },
} as const satisfies Record<string, WorkSpec>;

/** Resolves one office call's exact operation from its public identity and returns its presentation fields.
 * 从办公调用的公开身份解析精确操作并返回对应的展示字段。 */
export function vulcanWorkOperation(call: ToolCall) {
  // Implicit calls use a complete, case-sensitive name. Explicit calls may use the provider's underscore form.
  // 隐式调用使用区分大小写的完整名称，显式调用可使用供应商的下划线形式。
  const publicName = call.event.tool_name;
  const name = publicName === "call_luaskill"
    ? textOf(call.input.name)?.trim()
    : publicName.startsWith("vulcan_work_") ? publicName.replaceAll("_", "-") : publicName;
  if (!name?.startsWith("vulcan-work-")) return null;

  // Only the declared entries in the manifest are eligible for the specialized card.
  // 只有清单中声明的入口才能使用专用卡片。
  const operation = name.slice("vulcan-work-".length);
  if (!Object.hasOwn(OPERATIONS, operation)) return null;
  const id = operation as keyof typeof OPERATIONS;
  return { id, ...OPERATIONS[id] };
}

/** Reads the selected call's argument object in its explicit or documented dispatcher form.
 * 按显式形式或调度器已声明的形式读取该调用的参数对象。 */
function workArguments(call: ToolCall): Record<string, unknown> {
  if (call.event.tool_name !== "call_luaskill") return call.input;

  // The public dispatcher uses name/value pairs and retains object and JSON string forms for host callers.
  // 公开调度器使用名称和值组成的参数对，同时为宿主调用方保留对象及 JSON 字符串形式。
  const args = call.input.args;
  if (typeof args === "string") {
    try {
      return objectOf(JSON.parse(args)) ?? {};
    } catch {
      return {};
    }
  }
  if (!Array.isArray(args)) return objectOf(args) ?? {};
  return Object.fromEntries(args.flatMap((item) => {
    // A projected parameter is accepted only in the dispatcher's declared pair shape.
    // 只有符合调度器声明的参数对形状时才投影该参数。
    const pair = objectOf(item);
    return typeof pair?.name === "string" && typeof pair.value === "string" ? [[pair.name, pair.value]] : [];
  }));
}

/** Projects a resolved office call to its target, confirmed output, argument scope and result facts.
 * 将已解析的办公调用投影为目标、已确认输出、参数范围和结果事实。 */
export function vulcanWorkPresentation(call: ToolCall, operation: NonNullable<ReturnType<typeof vulcanWorkOperation>>) {
  // Only completed, successful results supply counts and openable output paths.
  // 仅完成且成功的结果可提供计数和可打开的输出路径。
  const input = workArguments(call);
  const result = call.state === "done" ? objectOf(v1ModelResult(call.event.output)) : null;
  const sourcePath = textOf(input.path);
  const outputPath = operation.family === "edit" ? textOf(result?.output_path) : null;
  const targetPath = operation.family === "edit" ? outputPath ?? textOf(input.output_path) : sourcePath;
  const filePath = result ? outputPath ?? (operation.family === "read" ? sourcePath : null) : null;
  const sheet = textOf(input.sheet);
  const range = textOf(input.range);
  const counts = operation.counts.flatMap((key) => {
    const count = numberOf(result?.[key]);
    return count !== null && Number.isInteger(count) && count >= 0 ? [{ key, count }] : [];
  });
  return { input, result, targetPath, sourcePath, outputPath, filePath, sheet, range, counts };
}
