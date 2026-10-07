// @vitest-environment jsdom
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { I18nProvider } from "@/i18n";
import { ConversationToolResultTabs, toolResultMode } from "./conversation-tool-result-tabs";
import ConversationBranchToolDomain from "./conversation-branch-tool-domain";

/** Builds a canonical tool fixture; name/input define its contract and output remains immutable.
 * 构造规范工具夹具；name/input 定义契约，output 保持不可变。
 * Returns an exact tool event for mode and markup assertions.
 * 返回用于模式与标记断言的精确工具事件。 */
function event(name: string, input: ToolCallEvent["input"] = {}): ToolCallEvent {
  return { kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: name, input, request_stream_id: "tabs-test", status: "completed", output: { model_output: "# exact result" } };
}

describe("tool result presentation policy", () => {
  it.each(["todolist", "get_goal", "create_goal", "update_goal", "submit_plan", "ask", "shell_command", "terminal_session", "runlua", "runlua_session", "spawn_agent", "send_message", "wait_agent", "list_agents", "agent_orchestration_info", "subagent_feedback", "close_agent", "interrupt_agent", "debug_session_open", "debug_session_request", "debug_session_events", "debug_session_status", "debug_session_close", "file_edit", "apply_patch", "vulcan_file_read"])("keeps %s as a dedicated body", name => {
    // markup retains the specialized view without raw details or tabs.
    // markup 保留专属视图，不包含原始详情与标签。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationToolResultTabs event={event(name)}><div>dedicated result</div></ConversationToolResultTabs></I18nProvider>);
    expect(markup).toBe("<div>dedicated result</div>");
  });
  it.each(["read", "write", "create", "delete", "rename", "list", "stat", "search"])("keeps file action %s direct", action => {
    expect(toolResultMode(event("file", { action }))).toBe("direct");
  });
  it("keeps the repository overview direct", () => {
    expect(toolResultMode(event("vulcan_codekit_repo_map"))).toBe("direct");
  });
  it.each(["file", "vulcan_file_list"])("shows %s listing content directly without any tab bar", name => {
    // markup preserves the listing renderer while omitting every payload tab.
    // markup 保留列表渲染内容，同时省略全部载荷标签。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationToolResultTabs event={event(name, { action: "list" })}><pre>[file] src/main.ts</pre></ConversationToolResultTabs></I18nProvider>);
    expect(markup).toBe("<pre>[file] src/main.ts</pre>");
  });
  it.each([["vulcan_codekit_ast_tree", "AST 树"], ["vulcan_codekit_rg", "搜索结果"], ["runtime_config", "配置详情"], ["unknown_tool", "返回信息"]])("names the result form for %s and always retains both raw tabs", (name, label) => {
    // host allows semantic tab queries without depending on serialized attribute order.
    // host 用于语义标签查询，不依赖序列化属性顺序。
    const host = document.createElement("div");
    host.innerHTML = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationToolResultTabs event={{ ...event(name), output: undefined }}><div>specialized</div></ConversationToolResultTabs></I18nProvider>);
    expect(Array.from(host.querySelectorAll('[role="tab"]'), tab => tab.textContent)).toEqual([label, "输入", "输出"]);
    expect(host.querySelector('[role="tabpanel"]:not([hidden])')?.textContent).toBe("specialized");
    expect(host.querySelector('[role="tab"][disabled]')).toBeNull();
  });
  it("shows a skill as one complete markdown source block without raw tabs", () => {
    // source exceeds the old preview threshold so its final line must remain present.
    // source 超过旧预览阈值，末行必须仍存在。
    const source = "# Skill\n" + "full content\n".repeat(200) + "END-SKILL";
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationToolResultTabs event={{ ...event("skill"), output: { model_output: source } }} /></I18nProvider>);
    expect(markup).toContain("markdown-code-block");
    expect(markup).toContain("END-SKILL");
    expect(markup).not.toContain('role="tab"');
    expect(markup).not.toContain("展开全文");
  });
  it("renders todo progress as compact accepted tasks without empty explanation", () => {
    // todo uses the accepted host payload instead of the intentionally different pending request.
    // todo 使用已接受的宿主载荷，而非刻意不同的待提交请求。
    const todo = { ...event("todolist", { items: [] }), output: { model_output: "updated", host_result: { kind: "todolist", payload: { items: [{ step: "done", status: "completed" }, { step: "active", status: "in_progress" }, { step: "later", status: "pending" }], explanation: null } } } };
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationBranchToolDomain event={todo} id="todo" /></I18nProvider>);
    expect(markup).toContain('role="progressbar"');
    expect(markup).toContain('aria-valuenow="1"');
    expect(markup.match(/class="tool-todo-item"/g)).toHaveLength(3);
    expect(markup).not.toContain("原始详情");
    expect(markup).not.toContain("<dt>");
    expect(markup).not.toContain("说明");
  });
});
