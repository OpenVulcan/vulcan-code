// Code-mode card for exec and wait. The head names the JavaScript cell and how it ended; the body shows the code
// that ran, the notify() messages it sent, what it printed, and any images it returned to the model.
// exec 与 wait 的代码模式卡。标题标明 JavaScript 单元及其结束方式；正文展示运行的代码、它通过 notify() 发送的消息、
// 它打印的内容，以及它交给模型的图片。
import * as React from "react";
import { SquareCode } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { formatToolDuration, objectOf, textOf, type ToolCall } from "../tool-call";
import { CodeView } from "../views/code-view";
import { ErrorNote } from "../views/result-views";
import { TerminalView } from "../views/terminal-view";
import { codeLines } from "./lua-card";

// CodeModeStatus is how one observation of a cell ended.
// CodeModeStatus 是一次单元观察的结束方式。
export type CodeModeStatus = "yielded" | "completed" | "failed" | "terminated";

// CodeModeImage is one image the cell returned to the model.
// CodeModeImage 是单元交给模型的一张图片。
export interface CodeModeImage {
  // Base64 payload.
  // Base64 载荷。
  data: string;
  // Image media type.
  // 图片媒体类型。
  mimeType: string;
}

// CodeModeObservation is the parsed result of one exec or wait call.
// CodeModeObservation 是一次 exec 或 wait 调用的解析结果。
export interface CodeModeObservation {
  // How the observation ended, or null when the output has no status header.
  // 观察的结束方式；输出没有状态标题时为 null。
  status: CodeModeStatus | null;
  // Cell that is still running, when the cell yielded.
  // 单元让出时仍在运行的单元标识。
  cellId: string | null;
  // notify() messages delivered with this observation.
  // 本次观察交付的 notify() 消息。
  notifications: string[];
  // Text the cell printed.
  // 单元打印的文本。
  output: string;
  // Images the cell returned.
  // 单元返回的图片。
  images: CodeModeImage[];
}

// STATUS_LABELS names each observation status.
// STATUS_LABELS 命名每种观察状态。
const STATUS_LABELS: Record<CodeModeStatus, TranslationKey> = {
  completed: "toolCard.codeMode.completed",
  failed: "toolCard.codeMode.failed",
  terminated: "toolCard.codeMode.terminated",
  yielded: "toolCard.codeMode.yielded",
};

// MEDIA_MARKER marks a tool result that carries images for the model.
// MEDIA_MARKER 标记携带交给模型之图片的工具结果。
const MEDIA_MARKER = "$vulcan_tool_result_media";

/** Returns whether one call is a code-mode exec or wait call; both carry their own argument shape.
 * 返回一次调用是否为代码模式 exec 或 wait 调用；两者各有专属参数形状。 */
export function toolCallIsCodeMode(call: ToolCall): boolean {
  if (call.name === "exec") return typeof call.input.code === "string" || call.request !== null;
  if (call.name === "wait") return typeof call.input.cell_id === "string" || call.request !== null;
  return false;
}

/** Splits the model output of one exec or wait call into its status, notifications, printed text and images.
 * 将一次 exec 或 wait 调用的模型输出拆分为状态、通知、打印文本与图片。 */
export function readCodeModeObservation(modelOutput: string | null): CodeModeObservation {
  const images: CodeModeImage[] = [];
  let text = modelOutput ?? "";
  if (text.startsWith("{") && text.includes(MEDIA_MARKER)) {
    try {
      const envelope = objectOf(JSON.parse(text));
      if (envelope?.[MEDIA_MARKER] === 1 && Array.isArray(envelope.content)) {
        const parts: string[] = [];
        for (const value of envelope.content) {
          const block = objectOf(value);
          if (block?.type === "text") parts.push(textOf(block.text) ?? "");
          else if (block?.type === "image" && typeof block.data === "string" && typeof block.mimeType === "string") {
            images.push({ data: block.data, mimeType: block.mimeType });
          }
        }
        // The first block is the header, which already ends with "Output:\n".
        // 第一个块是标题，它本身以 "Output:\n" 结尾。
        text = parts.length > 0 ? parts[0] + parts.slice(1).join("\n") : "";
      }
    } catch {
      // A result that only looks like an envelope stays plain text.
      // 只是形似封套的结果保持为纯文本。
    }
  }
  const header = /^Script (running with cell ID (\S+)|completed|failed|terminated)\nWall time [^\n]*\n(?:Notifications:\n([\s\S]*?))?Output:\n/.exec(text);
  if (!header) return { cellId: null, images, notifications: [], output: text, status: null };
  const status: CodeModeStatus = header[2] !== undefined
    ? "yielded"
    : (header[1] as Exclude<CodeModeStatus, "yielded">);
  return {
    cellId: header[2] ?? null,
    images,
    notifications: (header[3] ?? "").split("\n").filter((line) => line.trim()),
    output: text.slice(header[0].length),
    status,
  };
}

// firstCodeLine names a script by its first line that is neither blank nor the exec pragma.
// firstCodeLine 以既非空白也非 exec 编译指示的首行命名脚本。
export function firstCodeLine(code: string): string {
  return code.split(/\r?\n/).map((line) => line.trim()).find((line) => line && !line.startsWith("// @exec:")) ?? "";
}

/** Renders one exec or wait call.
 * 渲染一次 exec 或 wait 调用。 */
export function CodeModeCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const exec = call.name === "exec";
  const code = exec ? textOf(call.input.code) ?? "" : "";
  const lines = React.useMemo(() => codeLines(code), [code]);
  const observation = React.useMemo(() => readCodeModeObservation(call.modelOutput), [call.modelOutput]);
  const requestedCell = textOf(call.input.cell_id);
  const cellId = observation.cellId ?? requestedCell;
  const verb = exec
    ? "toolCard.codeMode.exec"
    : call.input.terminate === true ? "toolCard.codeMode.terminate" : "toolCard.codeMode.wait";
  const target = exec ? firstCodeLine(code) : requestedCell ? t("toolCard.codeMode.cell").replace("{id}", requestedCell) : "";
  const showOutput = observation.status !== null || (!call.isError && observation.output.trim() !== "");
  return (
    <ToolFrame
      {...props}
      icon={SquareCode}
      meta={(
        <>
          {observation.status === "yielded" && cellId ? <span>{t("toolCard.codeMode.cell").replace("{id}", cellId)}</span> : null}
          {observation.status ? <span>{t(STATUS_LABELS[observation.status])}</span> : null}
          {call.durationMs !== null && call.state !== "running" ? <span>{formatToolDuration(call.durationMs)}</span> : null}
        </>
      )}
      state={call.state}
      target={target}
      targetTitle={exec ? code : target}
      verb={t(verb)}
    >
      {code.trim() || call.modelOutput ? (
        <>
          {code.trim() ? <CodeView lines={lines} /> : null}
          {observation.notifications.length > 0 ? (
            <p className="vk-tool-note">
              {t("toolCard.codeMode.notifications")}
              {"\n"}
              {observation.notifications.join("\n")}
            </p>
          ) : null}
          {call.isError && observation.status === null ? <ErrorNote text={call.modelOutput} /> : null}
          {showOutput ? <TerminalView emptyText={t("toolCard.codeMode.noOutput")} text={observation.output} /> : null}
          {observation.images.length > 0 ? (
            <ul className="vk-media-list">
              {observation.images.map((image, index) => (
                <li className="vk-media-item" key={index}>
                  <img
                    alt={t("toolCard.codeMode.image").replace("{index}", String(index + 1))}
                    decoding="async"
                    loading="lazy"
                    src={`data:${image.mimeType};base64,${image.data}`}
                  />
                </li>
              ))}
            </ul>
          ) : null}
        </>
      ) : null}
    </ToolFrame>
  );
}
