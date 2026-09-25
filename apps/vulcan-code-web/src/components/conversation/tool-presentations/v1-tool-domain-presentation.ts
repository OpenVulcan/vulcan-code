import { formatMessage, type TranslationKey } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import type { ToolJsonValue, ToolPayload } from "@/service/conversation-event-window-store";
import { modelExtensionToolTitleKey } from "./model-extension-tool-presentation";

/** One readable result section; value is retained verbatim, while markdown explicitly opts into rendering.
 * 一个可读结果区域；value 原样保留，markdown 显式启用 Markdown 渲染。 */
export interface V1ToolDomainSection {
  // User-facing section title.
  // 面向用户的区域标题。
  title: string;
  // Exact selected result value, never a reconstructed execution payload.
  // 精确选取的结果值，绝不重建执行载荷。
  value: ToolJsonValue;
  // Whether this result is documented Markdown or plain structured data.
  // 此结果是否为契约声明的 Markdown，而非普通结构化数据。
  markdown?: boolean;
  // A known configuration descriptor uses a compact package row instead of a generic field grid.
  // 已知配置声明使用紧凑技能包行，而不是通用字段网格。
  presentation?: "configuration-package" | "todo-list";
}

/** Pure first-level result model with a bounded summary and an explicit diagnostic.
 * 纯首层结果模型，包含有界摘要与显式诊断。 */
export interface V1ToolDomainPresentation {
  // Concise context copied from exact input or result fields.
  // 从精确输入或结果字段复制的简短上下文。
  summary: string;
  // Ordered readable sections whose original payload remains separately inspectable.
  // 有序可读区域，其原始载荷仍可单独检查。
  sections: V1ToolDomainSection[];
  // Visible failure or incomplete-contract explanation, when one exists.
  // 存在失败或契约不完整时可见的说明。
  diagnostic: string | null;
}

/** Reads only an object-shaped JSON value; undefined remains absent rather than successful empty data.
 * 只读取对象形态的 JSON 值；undefined 保持缺失，不能变成成功空数据。
 * Parameter value is one persisted member; returns its object or null.
 * 参数 value 是一个持久化成员；返回其对象或 null。 */
export function domainObject(value: ToolJsonValue | undefined): ToolPayload | null {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value : null;
}

/** Decodes the fixed model_output member produced by canonicalize_tool_invocation_result.
 * 解码 canonicalize_tool_invocation_result 产出的固定 model_output 成员。
 * Parameter output is the V1 envelope; returns decoded JSON or the original Markdown string.
 * 参数 output 是 V1 信封；返回解码后的 JSON 或原始 Markdown 字符串。 */
export function v1ModelResult(output: ToolPayload | undefined): ToolJsonValue | undefined {
  // Core-generated terminal/progress payloads are direct objects; completed bridge calls use model_output.
  // Core 生成的终态与进度载荷是直接对象；完成的桥接调用使用 model_output。
  if (output === undefined || !Object.hasOwn(output, "model_output")) return output;
  // Canonical output is a string: objects are serialized once by the runtime bridge.
  // 规范输出是字符串：对象由运行时桥接序列化一次。
  const value = output?.model_output;
  if (typeof value !== "string") return value;
  try {
    return JSON.parse(value) as ToolJsonValue;
  } catch {
    return value;
  }
}

/** Selects a host payload only after matching its declared semantic kind.
 * 只在匹配声明的语义类型后选择宿主载荷。
 * Parameters output and kind identify the fixed host result; returns that payload or null.
 * 参数 output 和 kind 标识固定宿主结果；返回该载荷或 null。 */
function hostPayload(output: ToolPayload | undefined, kind: string): ToolPayload | null {
  // host is the sole host_result object in the persisted envelope.
  // host 是持久化信封中唯一的 host_result 对象。
  const host = domainObject(output?.host_result);
  return host?.kind === kind ? domainObject(host.payload) : null;
}

/** Reads one exact optional string field for a compact card summary.
 * 读取一个精确的可选字符串字段用于紧凑卡片摘要。
 * Parameters object and key identify the declared field; returns text or an empty summary.
 * 参数 object 与 key 标识声明字段；返回文本或空摘要。 */
function textField(object: ToolPayload | null, key: string): string {
  return typeof object?.[key] === "string" ? object[key] as string : "";
}

/** Appends an existing value without inventing defaults for missing data.
 * 追加已有值，不为缺失数据编造默认值。
 * Parameters sections, title, value and markdown select one display section; returns nothing.
 * 参数 sections、title、value 与 markdown 选择一个展示区域；无返回值。 */
function addSection(sections: V1ToolDomainSection[], title: string, value: ToolJsonValue | undefined, markdown = false): void {
  if (value !== undefined) sections.push({ title, value, markdown });
}

/** UI copy key for each stable MCP failure code retained by the runtime.
 * 运行时保留的每个稳定 MCP 失败编码对应的界面文案键。 */
const MCP_FAILURE_LABELS: Readonly<Record<string, TranslationKey>> = {
  mcp_tool_not_found: "toolDomain.mcp.notFound",
  mcp_server_disconnected: "toolDomain.mcp.disconnected",
  mcp_tool_invocation_failed: "toolDomain.mcp.invocationFailed",
};

/** Translates one stable MCP failure code into UI copy while retaining machine code and detail.
 * 将稳定 MCP 失败编码翻译为界面文案，同时保留机器编码与英文详情。
 * Parameters error and t are the canonical tool error text and active-locale lookup; returns localized text or the original value.
 * 参数 error 与 t 是规范工具错误文本与当前语言查询；返回本地化文本，或原值。 */
function localizeMcpFailure(error: string, t: (key: TranslationKey) => string): string {
  // separator splits the stable machine code from its concise English diagnostic.
  // separator 把稳定机器编码与其简明英文诊断分开。
  const separator = error.indexOf(": ");
  if (separator <= 0) return error;
  const code = error.slice(0, separator);
  const labelKey = MCP_FAILURE_LABELS[code];
  if (labelKey === undefined) return error;
  const detail = error.slice(separator + 2).trim();
  return formatMessage(t(detail === "" ? "toolDomain.mcp.failure" : "toolDomain.mcp.failureDetail"), { label: t(labelKey), code, detail });
}

/** Selects declared fields verbatim while leaving all other facts in raw details.
 * 原样选择声明字段，同时将其他事实保留在原始详情。
 * Parameters object and keys identify known members; returns an object of present fields only.
 * 参数 object 与 keys 标识已知成员；返回仅包含现有字段的对象。 */
function fields(object: ToolPayload | null, keys: readonly string[]): ToolPayload {
  return Object.fromEntries(keys.filter((key) => object?.[key] !== undefined).map((key) => [key, object![key]]));
}

/** Projects the actual tool contract to readable sections without executing or resolving external resources.
 * 将实际工具契约投影为可读区域，不执行工具或解析外部资源。
 * Parameters tool and t are one authoritative V1 event and the active-locale lookup; returns summary, sections and visible diagnostics.
 * 参数 tool 与 t 是一个权威 V1 事件与当前语言查询；返回摘要、区域与可见诊断。 */
export function resolveV1ToolDomainPresentation(tool: ToolCallEvent, t: (key: TranslationKey) => string): V1ToolDomainPresentation {
  // result is decoded only from the formal model output, never by polling candidate paths.
  // result 仅从正式模型输出解码，绝不轮询候选路径。
  const result = v1ModelResult(tool.output);
  // object narrows structured model output when its contract admits an object.
  // object 在契约允许对象时收窄结构化模型输出。
  const object = domainObject(result);
  // sections retain exact result values in user-facing order.
  // sections 按面向用户的顺序保留精确结果值。
  const sections: V1ToolDomainSection[] = [];
  // summary starts empty so an unknown tool cannot impersonate another contract.
  // summary 初始为空，防止未知工具冒充其他契约。
  let summary = "";
  // diagnostic reports both transport and domain failures without changing persisted status.
  // diagnostic 同时报告传输与领域失败，不修改持久化状态。
  let diagnostic: string | null = null;

  if (tool.status === "failed" || tool.output?.is_error === true) {
    // failure is the canonical tool_error host payload created by the bridge.
    // failure 是桥接创建的规范 tool_error 宿主载荷。
    const failure = hostPayload(tool.output, "tool_error");
    diagnostic = failure === null ? (typeof result === "string" ? result : t("toolDomain.executionFailed")) : localizeMcpFailure(textField(failure, "error"), t);
  }

  switch (tool.tool_name) {
    case "todolist": {
      // Accepted list items belong to the host result; pending input remains explicitly a request.
      // 已接受任务项归宿主结果所有；待执行输入仍明确为请求。
      const accepted = hostPayload(tool.output, "todolist");
      // list deliberately distinguishes an accepted result from a pending request.
      // list 明确区分已接受结果与待执行请求。
      const list = accepted === null ? tool.input : accepted;
      // items uses the one declared task collection on the selected snapshot.
      // items 使用选定快照上唯一声明的任务集合。
      const items = list.items;
      if (Array.isArray(items)) {
        // completed counts only explicit completed item states.
        // completed 仅统计显式 completed 状态的任务项。
        const completed = items.filter((item) => domainObject(item)?.status === "completed").length;
        summary = formatMessage(t("toolDomain.todo.summary"), { completed, total: items.length }) + (accepted === null ? t("toolDomain.todo.requestSuffix") : "");
      }
      if (Array.isArray(items)) sections.push({ title: t(accepted === null ? "toolDomain.todo.requested" : "toolDomain.todo.progress"), value: items, presentation: "todo-list" });
      if (typeof list.explanation === "string" && list.explanation.trim()) addSection(sections, t("toolDomain.explanation"), list.explanation);
      break;
    }
    case "get_goal":
    case "create_goal":
    case "update_goal": {
      // goal is the documented GoalToolResponse.goal member, which may be null.
      // goal 是文档定义的 GoalToolResponse.goal 成员，允许为 null。
      const goal = domainObject(object?.goal);
      summary = goal ? textField(goal, "objective") : textField(tool.input, "objective");
      if (object?.goal === null) addSection(sections, t("toolDomain.goal.current"), t("toolDomain.goal.unset"));
      if (goal) addSection(sections, t("toolDomain.goal.budget"), fields(goal, ["objective", "status", "tokensUsed", "tokenBudget", "timeUsedSeconds"]));
      if (object?.remainingTokens !== undefined && object.remainingTokens !== null) addSection(sections, t("toolDomain.goal.remainingTokens"), object.remainingTokens);
      break;
    }
    case "runtime_config": {
      summary = `${t(tool.input.action === "validate" ? "toolDomain.config.validate" : "toolDomain.config.declarations")} · ${textField(tool.input, "skill_id") || t("toolDomain.config.allPackages")}`;
      if (object?.ok === false) diagnostic = textField(domainObject(object.error), "message") || t("toolDomain.config.queryFailed");
      if (object?.ok === true && object.action === "describe" && Array.isArray(object.result)) {
        // packageValue enumerates the exact effective descriptor array returned by LuaSkills.
        // packageValue 遍历 LuaSkills 返回的精确生效声明数组。
        for (const packageValue of object.result) {
          // descriptor represents one package and excludes configured values in the host request.
          // descriptor 表示一个技能包，宿主请求已排除配置值。
          const descriptor = domainObject(packageValue);
          if (!descriptor) continue;
          // packageSummary retains actual counts for details without laying out four empty issue rows.
          // packageSummary 保留真实计数供详情查看，不铺开四行空问题。
          const packageSummary = fields(descriptor, ["skill_version", "complete", "missing_count", "invalid_count", "business_issue_count", "orphaned_count"]);
          // Only declaration fields are projected. Effective values are excluded by the host contract.
          // 只投影声明字段。生效配置值由宿主契约排除。
          if (Array.isArray(descriptor.items)) packageSummary.items = descriptor.items.map((item) => fields(domainObject(item), ["key", "title", "type", "description", "required", "sensitive", "state", "constraints", "options", "validation_error"]));
          sections.push({ title: textField(descriptor, "skill_id"), value: packageSummary, presentation: "configuration-package" });
        }
        if (object.result.length === 0) addSection(sections, t("toolDomain.config.declarations"), t("toolDomain.config.noDeclarations"));
      } else if (object?.ok === true && object.action === "validate") {
        addSection(sections, t("toolDomain.config.validation"), object.result);
        if (domainObject(object.result)?.complete === false) diagnostic = t("toolDomain.config.validationFailed");
      }
      break;
    }
    case "codebase_retrieval":
      summary = textField(tool.input, "information_request");
      if (Array.isArray(object?.hits)) addSection(sections, t("toolDomain.code.hits"), object.hits.map((hit) => fields(domainObject(hit), ["path", "startLine", "endLine", "content", "source", "score"])));
      addSection(sections, t("toolDomain.code.degradations"), object?.degradations);
      break;
    case "submit_plan": {
      // artifact is the accepted plan body captured at the write boundary.
      // artifact 是在写入边界捕获的已接受计划正文。
      const artifact = hostPayload(tool.output, "plan_artifact");
      summary = textField(tool.input, "name");
      addSection(sections, t("toolDomain.plan.artifact"), object === null ? undefined : fields(object, ["name", "path", "message"]));
      addSection(sections, t(artifact === null ? "toolDomain.plan.pending" : "toolDomain.plan.body"), artifact === null ? tool.input.content : artifact.content, true);
      break;
    }
    case "ask": {
      // question_runtime stores host kind ask, with the structured display nested under payload.display.
      // question_runtime 保存宿主类型 ask，结构化展示位于 payload.display。
      const answer = hostPayload(tool.output, "ask");
      // display owns per-question answers in the official Ask host contract.
      // display 在正式 Ask 宿主契约中持有逐题答案。
      const display = domainObject(answer?.display);
      // questions uses accepted display facts only after a host reply exists.
      // questions 仅在宿主回答存在后使用已接受展示事实。
      const questions = answer === null ? tool.input.questions : display?.questions;
      summary = Array.isArray(questions) ? formatMessage(t("toolDomain.ask.count"), { count: questions.length }) : "";
      addSection(sections, t(answer === null ? "toolDomain.ask.questions" : "toolDomain.ask.answered"), questions);
      break;
    }
    case "knowledge_search":
      summary = Array.isArray(tool.input.queries) ? tool.input.queries.join(" · ") : "";
      if (Array.isArray(object?.results)) {
        // queryResult is one independently executed knowledge query.
        // queryResult 是一个独立执行的知识查询。
        for (const queryResult of object.results) {
          // group owns this query's hits, failures and warnings.
          // group 持有本查询的命中、失败与提示。
          const group = domainObject(queryResult);
          addSection(sections, formatMessage(t("toolDomain.knowledge.search"), { query: textField(group, "query") }), group?.hits);
          if (Array.isArray(group?.failures) && group.failures.length > 0) {
            diagnostic = t("toolDomain.knowledge.partialFailure");
            addSection(sections, t("toolDomain.knowledge.failed"), group.failures);
          }
          if (Array.isArray(group?.warnings) && group.warnings.length > 0) addSection(sections, t("toolDomain.knowledge.warnings"), group.warnings);
        }
      }
      break;
    case "knowledge_browse":
      summary = textField(tool.input, "directory");
      addSection(sections, t("toolDomain.knowledge.directory"), object?.documents);
      addSection(sections, t("toolDomain.knowledge.nextCursor"), object?.nextCursor);
      break;
    case "knowledge_read": {
      // result is discriminated by targetType in execute_knowledge_read.
      // execute_knowledge_read 使用 targetType 区分 result。
      const read = domainObject(object?.result);
      summary = textField(tool.input, "targetId");
      if (read?.targetType === "document") {
        addSection(sections, t("toolDomain.knowledge.documentSource"), read.document);
        addSection(sections, t("toolDomain.knowledge.readRange"), fields(read, ["startLine", "endLine", "truncated"]));
        addSection(sections, t("toolDomain.knowledge.content"), read.content, true);
      } else if (read?.targetType === "chunk") {
        // chunk is the exact KnowledgeChunk returned by the chunk branch.
        // chunk 是片段分支返回的精确 KnowledgeChunk。
        const chunk = domainObject(read.chunk);
        addSection(sections, t("toolDomain.knowledge.chunkSource"), fields(chunk, ["documentId", "headingPath", "startLine", "endLine"]));
        addSection(sections, t("toolDomain.knowledge.content"), chunk?.content, true);
      }
      break;
    }
    case "vmm_memory_search":
      summary = Array.isArray(tool.input.queries) ? tool.input.queries.join(" · ") : "";
      if (Array.isArray(object?.results)) {
        // queryResult is one result group from the VMM memory-search projection.
        // queryResult 是 VMM 记忆搜索投影中的一个结果组。
        for (const queryResult of object.results) {
          // group keeps memory ids and source turn ids attached to their original query.
          // group 保持记忆标识与来源回合标识属于原始查询。
          const group = domainObject(queryResult);
          addSection(sections, formatMessage(t("toolDomain.memory.recall"), { query: textField(group, "query") }), group?.hits);
        }
      }
      break;
    case "vmm_turn_details":
      addSection(sections, t("toolDomain.memory.turnDetails"), object?.turns);
      break;
    case "vmm_memory_write":
      addSection(sections, t("toolDomain.memory.writeResult"), object === null ? undefined : fields(object, ["item_count", "items"]));
      break;
    case "vmm_memory_delete":
      addSection(sections, t("toolDomain.memory.deleteResult"), object === null ? undefined : fields(object, ["deleted_count", "not_found_count", "deleted_memory_ids", "not_found_memory_ids"]));
      break;
    case "skill":
    case "luaskill":
    case "call_luaskill":
      summary = textField(tool.input, "name");
      addSection(sections, t("toolDomain.skill.content"), result, typeof result === "string");
      break;
    case "context7_resolve_library_id":
      summary = textField(tool.input, "libraryName");
      addSection(sections, t("toolDomain.context7.candidates"), result, typeof result === "string");
      break;
    case "context7_query_docs":
      summary = textField(tool.input, "libraryId");
      addSection(sections, t("toolDomain.context7.query"), result, typeof result === "string");
      break;
    case "debug_session_open":
    case "debug_session_request":
    case "debug_session_events":
    case "debug_session_status":
    case "debug_session_close":
      summary = textField(tool.input, "session_id");
      addSection(sections, t("toolDomain.debug.result"), result, typeof result === "string");
      break;
    case "get_model_extension_parameters":
      summary = textField(tool.input, "capability");
      addSection(sections, t("toolDomain.modelExtension.models"), object?.models);
      break;
    case "image_understand":
    case "video_understand":
    case "audio_understand":
      summary = textField(tool.input, "model");
      if (object?.schema_version === "vulcan.media-understanding.v1") {
        addSection(sections, t("toolDomain.media.understanding"), object.answer, true);
        addSection(sections, t("toolDomain.media.segments"), object.segments);
        addSection(sections, t("toolDomain.media.warnings"), object.warnings);
      }
      break;
    default:
      if (!modelExtensionToolTitleKey(tool.tool_name)) {
        addSection(sections, t("toolDomain.result"), result, typeof result === "string");
        break;
      }
      summary = textField(tool.input, "model");
      // Normalized provider outputs have an explicit kind, shared across exact extension tool ids.
      // 归一化供应商输出具有显式 kind，由精确扩展工具标识共享。
      if (object?.kind === "text") addSection(sections, t("toolDomain.result"), object.text, true);
      else if (object?.kind === "search") addSection(sections, t("toolDomain.search.results"), object.search);
      else if (object?.kind === "extract") {
        // extract follows ProviderToolExtractOutput with explicit successes and failures.
        // extract 遵循显式区分成功失败的 ProviderToolExtractOutput。
        const extract = domainObject(object.extract);
        addSection(sections, t("toolDomain.extract.content"), extract?.results);
        if (Array.isArray(extract?.failures) && extract.failures.length > 0) {
          diagnostic = t("toolDomain.extract.partialFailure");
          addSection(sections, t("toolDomain.extract.failures"), extract.failures);
        }
      }
      else if (object?.kind === "composite") addSection(sections, t("toolDomain.generated"), object.composite);
      else addSection(sections, t("toolDomain.result"), result, typeof result === "string");
  }

  // Missing structured results stay visibly unknown; raw output remains accessible for diagnosis.
  // 缺失结构化结果保持明确未知；原始输出仍可用于诊断。
  if (sections.length === 0 && tool.status === "completed" && result !== undefined) {
    addSection(sections, t("toolDomain.returned"), result, typeof result === "string");
  }
  return { summary: summary.replace(/\s+/g, " ").slice(0, 180), sections, diagnostic };
}
