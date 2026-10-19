// Fallback card for tools without a purpose-built card (MCP servers, plugins, skills): the tool name, its most
// telling argument, and in the body the full arguments and the result as rendered Markdown.
// 没有专用卡的工具（MCP 服务、插件、技能）的兜底卡：工具名、最能说明问题的参数，正文为完整参数与以 Markdown
// 渲染的结果。
import { Wrench } from "lucide-react";
import { useI18n } from "@/i18n";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { formatToolDuration, type ToolCall } from "../tool-call";
import { JsonView } from "../views/json-view";
import { MarkdownResult } from "../views/result-views";

// TARGET_KEYS are argument names that usually say what a call is about, in preference order.
// TARGET_KEYS 是通常能说明调用对象的参数名，按优先顺序排列。
const TARGET_KEYS = ["path", "file", "query", "pattern", "url", "name", "id", "command", "prompt", "text"];

/** Renders one call through the generic card.
 * 以通用卡渲染一次调用。 */
export function GenericToolCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const { server, tool } = toolNameParts(call.event.tool_name);
  const target = primaryArgument(call);
  const hasInput = Object.keys(call.input).length > 0;
  return (
    <ToolFrame
      {...props}
      icon={Wrench}
      meta={(
        <>
          {server ? <span>{server}</span> : null}
          {call.durationMs !== null && call.state !== "running" ? <span>{formatToolDuration(call.durationMs)}</span> : null}
        </>
      )}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={tool}
    >
      {hasInput || call.modelOutput ? (
        <>
          {hasInput ? <JsonView label={t("toolCard.generic.input")} value={call.input} /> : null}
          {call.modelOutput ? <MarkdownResult error={call.isError} text={call.modelOutput} /> : null}
        </>
      ) : null}
    </ToolFrame>
  );
}

// toolNameParts splits "mcp__server__tool" style names into the server and the tool.
// toolNameParts 将 "mcp__server__tool" 形式的名称拆分为服务与工具。
export function toolNameParts(name: string): { server: string | null; tool: string } {
  const mcp = /^mcp__(.+?)__(.+)$/.exec(name);
  if (mcp) return { server: mcp[1], tool: mcp[2] };
  return { server: null, tool: name };
}

// primaryArgument picks the argument that best names what the call is about.
// primaryArgument 选取最能说明调用对象的参数。
export function primaryArgument(call: ToolCall): string {
  for (const key of TARGET_KEYS) {
    const value = call.input[key];
    if (typeof value === "string" && value.trim()) return value.trim().split("\n")[0];
  }
  for (const value of Object.values(call.input)) {
    if (typeof value === "string" && value.trim() && value.length <= 200) return value.trim().split("\n")[0];
  }
  return "";
}
