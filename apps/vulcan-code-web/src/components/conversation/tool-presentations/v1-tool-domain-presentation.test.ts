import { describe, expect, it } from "vitest";
import { readLocaleMessage, type TranslationKey } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import type { ToolJsonValue, ToolPayload } from "@/service/conversation-event-window-store";
import { resolveV1ToolDomainPresentation as resolveWithLocale, v1ModelResult } from "./v1-tool-domain-presentation";

/** Resolves one event with Chinese copy so assertions match the default UI locale.
 * 使用中文文案解析一个事件，使断言与默认界面语言一致。 */
const zh = (key: TranslationKey) => readLocaleMessage("zh-CN", key);
const resolveV1ToolDomainPresentation = (tool: ToolCallEvent) => resolveWithLocale(tool, zh);

/** Builds the exact bridge envelope, including JSON serialized inside model_output.
 * 构建精确桥接信封，包括 model_output 内序列化的 JSON。
 * Parameters name, input, result and host select fixture facts; returns a V1 tool event.
 * 参数 name、input、result 与 host 选择夹具事实；返回 V1 工具事件。 */
function event(name: string, input: ToolPayload, result?: ToolJsonValue, host?: ToolPayload): ToolCallEvent {
  return {
    kind: "tool_call", api_call_id: 1, tool_call_id: 2, tool_name: name, input,
    status: result === undefined ? "running" : "completed", request_stream_id: "domain-test",
    output: result === undefined ? undefined : {
      tool_call_id: "provider-2", tool_name: name, is_error: false,
      model_output: typeof result === "string" ? result : JSON.stringify(result), host_result: host ?? null,
    },
  };
}

describe("V1 domain results from actual runtime contracts", () => {
  it("reads current Ask replies from the sole body and preserves original questions", () => {
    const answer = "yes🦀, exactly";
    const body = `## User Answers\n\n- Scope: ${answer}\n`;
    const domain = resolveV1ToolDomainPresentation(event("ask", { questions: [{ header: "Scope", question: "Proceed?" }] }, body,
      { kind: "ask", payload: { format: "markdown" } }));
    expect(domain.sections[0].value).toEqual([{ header: "Scope", question: "Proceed?" }]);
    expect(domain.sections[1]).toMatchObject({ value: body, markdown: true });
  });
  it.each(["knowledge_search", "knowledge_browse", "knowledge_read", "codebase_retrieval",
    "vmm_memory_search", "vmm_turn_details", "vmm_memory_write", "vmm_memory_delete",
    "get_goal", "create_goal", "update_goal", "image_understand", "video_understand",
    "audio_understand", "browser_list", "browser_open", "browser_select", "browser_close"])(
    "renders the sole Markdown body of %s without needing the historical JSON fields",
    (name) => {
      // The fenced object is source content, not another tool-result envelope.
      // 围栏中的对象是源正文，不是另一个工具结果封装。
      const body = '# Source\n\n```json\n{"id":"original-source"}\n```';
      const domain = resolveV1ToolDomainPresentation(event(name, {}, body));
      expect(domain.sections).toHaveLength(1);
      expect(domain.sections[0]).toMatchObject({ value: body, markdown: true });
    },
  );

  // An accepted plan body is read from its committed snapshot, never reconstructed from the request.
  // 已接受计划正文读取提交快照，不从请求重新构造。
  it("keeps a Markdown plan receipt beside its authoritative artifact", () => {
    const body = "# Plan saved\n\nPath: `docs/plan/accepted.md`";
    const domain = resolveV1ToolDomainPresentation(event("submit_plan",
      { name: "Plan", content: "Unaccepted input" }, body,
      { kind: "plan_artifact", payload: { name: "Plan", path: "docs/plan/accepted.md", content: "Accepted plan" } }));
    expect(domain.sections).toEqual(expect.arrayContaining([
      expect.objectContaining({ value: body, markdown: true }),
      expect.objectContaining({ value: "Accepted plan", markdown: true }),
    ]));
    expect(JSON.stringify(domain.sections)).not.toContain("Unaccepted input");
  });

  it("decodes the canonical model result without mining unrelated result-shaped fields", () => {
    expect(v1ModelResult({ model_output: '{"hits":[]}', host_result: { kind: "other", payload: { hits: ["wrong"] } } })).toEqual({ hits: [] });
    expect(v1ModelResult({ model_output: "# Readable result" })).toBe("# Readable result");
    expect(v1ModelResult({ error: { kind: "interrupted" } })).toEqual({ error: { kind: "interrupted" } });
  });

  it("projects declarations without configured values and reports empty package discovery", () => {
    // descriptor follows LuaSkills 0.5.5 RuntimeSkillPackageConfigDescriptor.
    // descriptor 遵循 LuaSkills 0.5.5 RuntimeSkillPackageConfigDescriptor。
    const descriptor = { skill_id: "example", skill_version: "1.0", complete: false, missing_count: 1, invalid_count: 0, business_issue_count: 0, orphaned_count: 0, items: [{ key: "api_key", type: "string", required: true, sensitive: true, state: "missing", description: "Service credential", value: "MUST_NOT_RENDER" }] };
    const domain = resolveV1ToolDomainPresentation(event("runtime_config", { action: "describe" }, { ok: true, action: "describe", result: [descriptor] }));
    expect(domain.summary).toBe("配置声明 · 全部技能包");
    expect(JSON.stringify(domain.sections)).toContain("api_key");
    expect(JSON.stringify(domain.sections)).not.toContain("MUST_NOT_RENDER");
    const empty = resolveV1ToolDomainPresentation(event("runtime_config", { action: "describe" }, { ok: true, action: "describe", result: [] }));
    expect(empty.sections[0].value).toBe("没有生效的技能包配置声明");
  });

  it("keeps failed validation and missing configuration distinct from a successful tool invocation", () => {
    const invalid = resolveV1ToolDomainPresentation(event("runtime_config", { action: "validate", skill_id: "example" }, { ok: true, action: "validate", result: { skill_id: "example", complete: false, missing: [{ key: "api_key", code: "missing", message: "Required" }], invalid: [], business_issues: [], orphaned: [] } }));
    expect(invalid.sections[0].value).toMatchObject({ complete: false, missing: [{ key: "api_key" }] });
    const failed = resolveV1ToolDomainPresentation(event("runtime_config", { action: "validate", skill_id: "absent" }, { ok: false, action: "validate", error: { code: "NOT_FOUND", message: "Package is unavailable" } }));
    expect(failed.diagnostic).toBe("Package is unavailable");
  });

  it("uses accepted todo items rather than untrimmed request values", () => {
    const domain = resolveV1ToolDomainPresentation(event("todolist", { items: [{ step: " wrong ", status: "pending" }] }, "Todolist updated", { kind: "todolist", payload: { items: [{ step: "Accepted", status: "completed" }], explanation: "done" } }));
    expect(domain.summary).toBe("1/1 项完成");
    expect(domain.sections[0].value).toEqual([{ step: "Accepted", status: "completed" }]);
    expect(resolveV1ToolDomainPresentation(event("todolist", { items: [] })).summary).toContain("请求内容");
  });

  it("keeps absent goal budget unknown and preserves real zero usage", () => {
    const domain = resolveV1ToolDomainPresentation(event("get_goal", {}, { goal: { objective: "Finish", status: "active", tokensUsed: 0, timeUsedSeconds: 0 }, remainingTokens: null }));
    expect(domain.sections[0].value).toEqual({ objective: "Finish", status: "active", tokensUsed: 0, timeUsedSeconds: 0 });
    expect(domain.sections).toHaveLength(1);
    expect(resolveV1ToolDomainPresentation(event("get_goal", {}, { goal: null })).sections[0].value).toBe("尚未设置目标");
  });

  it("retains source lines and code exactly while keeping internal blob identity in raw details", () => {
    const domain = resolveV1ToolDomainPresentation(event("codebase_retrieval", { information_request: "Find settings" }, { hits: [{ id: "occurrence", blobName: "internal-blob", path: "src/settings.ts", startLine: 4, endLine: 5, content: "const x = 1;\nreturn x;", source: "semantic", score: 0.75 }], degradations: ["Rerank unavailable"] }));
    expect(domain.sections[0].value).toEqual([{ path: "src/settings.ts", startLine: 4, endLine: 5, content: "const x = 1;\nreturn x;", source: "semantic", score: 0.75 }]);
    expect(domain.sections[1].value).toEqual(["Rerank unavailable"]);
  });

  it("uses the exact accepted plan artifact without rereading a file", () => {
    const domain = resolveV1ToolDomainPresentation(event("submit_plan", { name: "Plan", content: "Pending" }, { name: "Plan", path: "docs/plan/plan.md", message: "Plan artifact saved." }, { kind: "plan_artifact", payload: { name: "Plan", path: "docs/plan/plan.md", content: "# Accepted plan", updatedAt: "1" } }));
    expect(domain.sections[1]).toEqual({ title: "计划正文", value: "# Accepted plan", markdown: true });
  });

  it("reads Ask answers from host kind ask and payload.display, preserving pending questions", () => {
    const questions = [{ header: "范围", question: "继续？", options: [], multiple: false }];
    const answeredQuestions = [{ ...questions[0], answers: ["继续"], answered: true }];
    const domain = resolveV1ToolDomainPresentation(event("ask", { questions }, "## User Answers", { kind: "ask", payload: { tool_name: "ask", headers: ["范围"], answers: [["继续"]], display: { kind: "ask_question", questions: answeredQuestions } } }));
    expect(domain.sections[0].value).toEqual(answeredQuestions);
    expect(resolveV1ToolDomainPresentation(event("ask", { questions })).sections[0]).toMatchObject({ title: "问题", value: questions });
  });

  it("surfaces partial knowledge and extract failures alongside available results", () => {
    const knowledge = resolveV1ToolDomainPresentation(event("knowledge_search", { queries: ["test"] }, { results: [{ query: "test", hits: [{ title: "Found", content: "Result" }], failures: [{ libraryId: "failed", error: "Unavailable" }], warnings: [] }] }));
    expect(knowledge.diagnostic).toContain("部分知识库检索失败");
    expect(knowledge.sections[0].value).toEqual([{ title: "Found", content: "Result" }]);
    const extract = resolveV1ToolDomainPresentation(event("web_extract", { model: "provider/model", args: {} }, { kind: "extract", extract: { results: [{ url: "https://example.com", content: "Found" }], failures: [{ url: "https://example.org", error: "Unavailable" }] } }));
    expect(extract.diagnostic).toContain("部分网页提取失败");
    expect(extract.sections).toHaveLength(2);
  });

  it("preserves VMM actual memory and source turn identifiers including missing-source zero", () => {
    const hit = { memory_id: 3, source_turn_id: 0, has_source_turn: false, abstract: "Remember", details_preview: "Details" };
    expect(resolveV1ToolDomainPresentation(event("vmm_memory_search", { queries: ["Remember"] }, { results: [{ query: "Remember", hits: [hit] }] })).sections[0].value).toEqual([hit]);
    const deleted = resolveV1ToolDomainPresentation(event("vmm_memory_delete", {}, { deleted_count: 0, not_found_count: 1, deleted_memory_ids: [], not_found_memory_ids: [3] }));
    expect(deleted.sections[0].value).toMatchObject({ deleted_count: 0, not_found_count: 1 });
  });

  it("reads all three understanding tools through the single canonical media schema", () => {
    for (const name of ["image_understand", "video_understand", "audio_understand"]) {
      const domain = resolveV1ToolDomainPresentation(event(name, { model: "provider/model", args: {} }, { schema_version: "vulcan.media-understanding.v1", media_kind: "image", answer: "A blue flame", segments: [], warnings: [] }));
      expect(domain.sections[0]).toEqual({ title: "理解结果", value: "A blue flame", markdown: true });
    }
  });

  it("shows dynamic tool JSON without applying another tool's kind contract", () => {
    const domain = resolveV1ToolDomainPresentation(event("unregistered_extension", {}, { kind: "text", text: "retained", extra: "also retained" }));
    expect(domain.sections[0].value).toEqual({ kind: "text", text: "retained", extra: "also retained" });
  });

  it("uses the bridge error field and never hides it behind raw JSON", () => {
    const failed = event("runtime_config", {}, "Long provider error", { kind: "tool_error", payload: { error: "Precise failure", details: { retryable: false } } });
    failed.status = "failed";
    expect(resolveV1ToolDomainPresentation(failed).diagnostic).toBe("Precise failure");
  });

  it("translates each stable MCP failure code into Chinese UI copy while retaining code and detail", () => {
    const mcpFailure = (error: string): string | null => {
      const failed = event("mcp_file_edit", {}, `Tool \`mcp_file_edit\` failed.\nReason: ${error}`, { kind: "tool_error", payload: { error, details: { code: "mcp" } } });
      failed.status = "failed";
      return resolveV1ToolDomainPresentation(failed).diagnostic;
    };
    expect(mcpFailure("mcp_tool_not_found: no MCP tool matches `server/read` in the current runtime catalog")).toBe("MCP 工具不存在（mcp_tool_not_found）：no MCP tool matches `server/read` in the current runtime catalog");
    expect(mcpFailure("mcp_server_disconnected: MCP server `server` is not connected; tool `read` was not invoked")).toBe("MCP 服务器未连接（mcp_server_disconnected）：MCP server `server` is not connected; tool `read` was not invoked");
    expect(mcpFailure("mcp_tool_invocation_failed: MCP tool `read` failed: tools/call(stdio): timeout")).toBe("MCP 工具调用失败（mcp_tool_invocation_failed）：MCP tool `read` failed: tools/call(stdio): timeout");
    expect(mcpFailure("Precise failure")).toBe("Precise failure");
  });
});
