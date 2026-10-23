// Lua card for runlua and runlua_session. The head names the task or session; the body shows the code that ran
// above what it printed or returned, so a script reads like a notebook cell.
// runlua 与 runlua_session 的 Lua 卡。标题标明任务或会话；正文先展示运行的代码，再展示其打印或返回的内容，
// 让脚本读起来像一个笔记本单元。
import * as React from "react";
import { Braces } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { numberOf, textOf } from "../tool-call";
import { CodeView, type CodeLine } from "../views/code-view";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// LUA_TOOLS are the tools that use the Lua card.
// LUA_TOOLS 是使用 Lua 卡的工具。
export const LUA_TOOLS = new Set(["runlua", "runlua_session"]);

// SESSION_VERBS names each runlua_session action.
// SESSION_VERBS 命名每个 runlua_session 动作。
const SESSION_VERBS: Record<string, TranslationKey> = {
  close: "toolCard.lua.close",
  eval: "toolCard.lua.eval",
  list: "toolCard.lua.list",
  start: "toolCard.lua.start",
  status: "toolCard.lua.status",
};

// codeLines numbers the submitted source.
// codeLines 为提交的源码编号。
export function codeLines(code: string): CodeLine[] {
  return code.replace(/\r\n?/g, "\n").replace(/\n+$/, "").split("\n").map((text, index) => ({ number: index + 1, text }));
}

// luaSessionBody drops the "# Lua session: action" heading the card already names.
// luaSessionBody 去掉卡片标题已表达的 "# Lua session: action" 标题。
export function luaSessionBody(text: string): string {
  return text.replace(/^# Lua session: \w+\n+/, "");
}

/** Renders one runlua or runlua_session call.
 * 渲染一次 runlua 或 runlua_session 调用。 */
export function LuaCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const session = call.name === "runlua_session";
  const host = call.host?.payload ?? {};
  const action = textOf(call.input.action) ?? textOf(host.action) ?? "eval";
  const code = textOf(call.input.code) ?? "";
  const lines = React.useMemo(() => codeLines(code), [code]);
  const target = session
    ? textOf(call.input.name) ?? textOf(host.name) ?? textOf(call.input.session_id) ?? textOf(host.sessionId) ?? ""
    : textOf(call.input.task) ?? "";
  const sessionCount = numberOf(host.sessionCount);
  const output = call.modelOutput ?? "";
  const result = session ? luaSessionBody(output) : output;
  return (
    <ToolFrame
      {...props}
      icon={Braces}
      meta={(
        <>
          {sessionCount !== null ? <span>{t("toolCard.lua.sessions").replace("{count}", String(sessionCount))}</span> : null}
        </>
      )}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={t(session ? SESSION_VERBS[action] ?? "toolCard.lua.eval" : "toolCard.lua.run")}
    >
      {code.trim() || result.trim() ? (
        <>
          {code.trim() ? <CodeView lines={lines} /> : null}
          {call.isError ? <ErrorNote text={call.modelOutput} /> : result.trim() ? <MarkdownResult text={result} /> : null}
        </>
      ) : null}
    </ToolFrame>
  );
}
