import * as React from "react";
import { ChevronDown, Copy } from "lucide-react";
import { Button } from "@/components/ui/button";
import { MarkdownMessage } from "./conversation-markdown";
import { ConversationMeasuredCollapse, useConversationMeasuredExpansion } from "./conversation-measured-collapse";
import { domainObject, type V1ToolDomainPresentation, type V1ToolDomainSection } from "./tool-presentations/v1-tool-domain-presentation";
import type { ToolJsonValue, ToolPayload } from "@/service/conversation-event-window-store";
import "@/styles/tool-domain.css";

// FIELD_LABELS translates exact protocol members without merging aliases or inferring ownership.
// FIELD_LABELS 翻译精确协议成员，不合并别名或推测归属。
const FIELD_LABELS: Readonly<Record<string, string>> = {
  objective: "目标", status: "状态", step: "任务", tokensUsed: "已用 Token", tokenBudget: "Token 预算", timeUsedSeconds: "耗时（秒）",
  skill_id: "技能包", skill_version: "版本", complete: "配置完整", missing_count: "缺少必填项", invalid_count: "无效项",
  business_issue_count: "业务校验问题", orphaned_count: "未声明配置项", items: "条目", key: "配置键", type: "类型",
  title: "标题", description: "说明", required: "必填", sensitive: "敏感项", state: "配置状态", constraints: "约束",
  options: "选项", validation_error: "校验错误", missing: "缺少项", invalid: "无效项", business_issues: "业务问题",
  orphaned: "未声明键", revision: "修订", store_scope: "存储作用域", code: "代码", message: "说明", value: "声明值",
  path: "路径", startLine: "起始行", endLine: "结束行", content: "内容", source: "来源", kind: "类型", score: "相关度",
  query: "查询", hits: "命中", failures: "失败", warnings: "提示", libraryName: "知识库", relativePath: "来源路径",
  headingPath: "标题路径", document: "文档", chunk: "片段", targetType: "读取类型", truncated: "已截断",
  libraryId: "知识库标识", library_id: "知识库标识", library_name: "知识库", documentId: "文档标识", chunkId: "片段标识",
  lineCount: "行数", byteSize: "字节数", chunkCount: "片段数", sourceId: "来源标识", folderPath: "目录",
  query_index: "查询序号", hit_count: "命中数", memory_id: "记忆标识", source_turn_id: "来源回合", has_source_turn: "有来源回合",
  abstract: "摘要", details_preview: "内容预览", category: "分类", created_datetime: "创建时间", retrieval: "召回依据",
  turn_id: "回合标识", user_question: "用户问题", assistant_answer: "助手回答", timeline: "过程", detail: "详情",
  previous_turn_ids: "前序回合", next_turn_ids: "后续回合", item_count: "返回数量", deduped: "已去重",
  deleted_count: "已删除", not_found_count: "未找到", deleted_memory_ids: "已删除记忆标识", not_found_memory_ids: "未找到记忆标识",
  header: "题目", question: "问题", multiple: "允许多选", answers: "用户回答", answered: "已回答", label: "选项",
  name: "名称", model: "模型", display_name: "模型名称", characteristics: "特点", input_modes: "输入模式",
  resolution_policy: "分辨率", duration_policy: "时长", format_policy: "格式", limitations: "限制", args_schema: "参数定义",
  properties: "参数", default: "默认值", minimum: "最小值", maximum: "最大值", enum: "可选值", additionalProperties: "额外参数",
  results: "结果", url: "来源链接", source_domain: "来源网站", snippet: "摘要", rank: "排名", error: "错误",
  text: "文本", resources: "生成资源", searches: "搜索", extracts: "提取", upstream_response_id: "上游请求标识",
  session_id: "会话标识", sessionId: "会话标识", command_session_id: "终端标识", protocol: "协议", method: "方法", command: "命令",
  connected: "已连接", closed: "已关闭", events: "事件", result: "返回结果", ok: "请求成功",
};

// STATUS_LABELS keeps domain states distinct from the outer tool execution lifecycle.
// STATUS_LABELS 将领域状态与外层工具执行生命周期区分。
const STATUS_LABELS: Readonly<Record<string, string>> = {
  pending: "待开始", in_progress: "进行中", completed: "已完成", active: "进行中", complete: "已完成", blocked: "受阻",
  unset: "未配置", missing: "缺少必填值", default: "使用默认值", configured: "已配置", invalid: "无效", failed: "失败",
};

/** Renders exact JSON values as bounded, selectable fields with explicit null and empty states.
 * 将精确 JSON 值渲染为有界且可选择的字段，明确区分 null 与空集合。
 * Parameters value and field identify one result member; returns its read-only view.
 * 参数 value 与 field 标识一个结果成员；返回其只读视图。 */
function ToolResultValue(props: { value: ToolJsonValue; field?: string; depth?: number }) {
  // expanded controls only this long value; no tool action is attached.
  // expanded 只控制当前长值，不绑定任何工具动作。
  const [expanded, setExpanded] = React.useState(false);
  // depth bounds eagerly rendered structure while keeping every branch available on demand.
  // depth 限制立即渲染的结构深度，同时保留每个分支按需查看的能力。
  const depth = props.depth ?? 0;
  // value is the exact persisted JSON member, not formatted data written back to the runtime.
  // value 是精确持久化 JSON 成员，不会将格式化数据回写运行时。
  const value = props.value;
  if (value === null) return <span className="v1-tool-domain-muted">-</span>;
  if (typeof value === "boolean") return <span>{value ? "是" : "否"}</span>;
  if (typeof value === "number") return <span title={String(value)}>{Number.isInteger(value) ? value.toLocaleString() : Number(value.toPrecision(6)).toString()}</span>;
  if (typeof value === "string") {
    // text labels only explicit status fields; arbitrary content remains untouched.
    // text 仅为显式状态字段提供标签；任意正文保持原样。
    const text = (props.field === "status" || props.field === "state") ? STATUS_LABELS[value] ?? value : value;
    if (text === "") return <span className="v1-tool-domain-muted">（空）</span>;
    // A source URL is clickable only for the exact url field and a web scheme.
    // 仅精确 url 字段且使用网页协议时，来源链接才可点击。
    if (props.field === "url" && /^https?:\/\//i.test(text)) return <a href={text} target="_blank" rel="noreferrer">{text}</a>;
    return <div className="v1-tool-domain-text"><span>{expanded ? text : text.slice(0, 900)}</span>{text.length > 900 ? <Button size="sm" variant="ghost" onClick={() => setExpanded(!expanded)}>{expanded ? "收起全文" : `展开全文（${text.length} 字符）`}</Button> : null}</div>;
  }
  if (Array.isArray(value)) {
    if (value.length === 0) return <span className="v1-tool-domain-muted">无条目</span>;
    return <div className="v1-tool-domain-list">
      {(expanded ? value : value.slice(0, 6)).map((entry, index) => <div className="v1-tool-domain-list-item" key={index}><ToolResultValue value={entry} depth={depth + 1} /></div>)}
      {value.length > 6 ? <Button size="sm" variant="ghost" onClick={() => setExpanded(!expanded)}>{expanded ? "收起列表" : `查看全部 ${value.length} 项`}</Button> : null}
    </div>;
  }
  // entries preserve all unknown members of dynamic tool contracts without guessing their semantics.
  // entries 保留动态工具契约的所有未知成员，不猜测其语义。
  const entries = Object.entries(value);
  if (entries.length === 0) return <span className="v1-tool-domain-muted">无字段</span>;
  if (depth > 2 && !expanded) return <Button size="sm" variant="ghost" onClick={() => setExpanded(true)}>查看 {entries.length} 个字段</Button>;
  return <dl className="v1-tool-domain-fields">{entries.map(([key, entry]) => <div className="v1-tool-domain-field" key={key}><dt title={key}>{FIELD_LABELS[key] ?? key}</dt><dd><ToolResultValue value={entry} field={key} depth={depth + 1} /></dd></div>)}</dl>;
}

/** Presents first-level domain results while retaining the existing Markdown renderer for declared text.
 * 展示首层领域结果，并对声明文本保留既有 Markdown 渲染器。
 * Parameter domain is a pure tool projection; returns a readable result body with explicit diagnostics.
 * 参数 domain 是纯工具投影；返回可读结果正文与显式诊断。 */
export function ConversationV1ToolDomainBody(props: { domain: V1ToolDomainPresentation }) {
  return <div className="v1-tool-domain" data-tool-domain-result>
    {props.domain.diagnostic ? <div className="v1-tool-domain-error" role="status">{props.domain.diagnostic}</div> : null}
    {props.domain.sections.map((section, index) => section.presentation === "configuration-package"
      ? <ConfigurationPackageResult key={`${section.title}:${index}`} section={section} />
      : <section key={`${section.title}:${index}`} className="v1-tool-domain-section"><h4>{section.title}</h4>{section.markdown && typeof section.value === "string" ? <BoundedToolMarkdown text={section.value} /> : <ToolResultValue value={section.value} />}</section>)}
    {props.domain.sections.length === 0 && !props.domain.diagnostic ? <div className="v1-tool-domain-muted">尚无可展示结果</div> : null}
  </div>;
}

/** Shows package identity and completeness on one row, with declarations available on demand.
 * 将技能包身份与完整性显示在一行，按需展开声明。
 * Parameter section contains the exact redacted descriptor projection; returns its readable summary.
 * 参数 section 包含精确且已过滤配置值的声明投影；返回其可读摘要。 */
function ConfigurationPackageResult(props: { section: V1ToolDomainSection }) {
  // descriptor is owned by the configuration-package projection, never inferred from arbitrary tools.
  // descriptor 归 configuration-package 投影所有，不从任意工具推断。
  const descriptor = domainObject(props.section.value);
  // expanded mounts declaration fields only after a user request.
  // expanded 仅在用户请求后挂载声明字段。
  const [expanded, setExpanded] = React.useState(false);
  if (!descriptor) return <div className="v1-tool-domain-error">配置声明格式无效，请查看原始详情。</div>;
  // issueKeys are the four count fields defined by RuntimeSkillPackageConfigDescriptor.
  // issueKeys 是 RuntimeSkillPackageConfigDescriptor 定义的四个计数字段。
  const issueKeys = ["missing_count", "invalid_count", "business_issue_count", "orphaned_count"] as const;
  // issues shows only nonzero facts; full zero and missing distinctions remain in the tooltip and raw details.
  // issues 只显示非零事实；零值与缺失的完整区别保留在提示与原始详情。
  const issues = issueKeys.filter((key) => typeof descriptor[key] === "number" && descriptor[key] > 0);
  // countDetail does not invent zero for an absent count.
  // countDetail 不为缺失计数编造零值。
  const countDetail = issueKeys.map((key) => `${FIELD_LABELS[key]}：${descriptor[key] === undefined ? "-" : String(descriptor[key])}`).join("\n");
  return <section className="v1-tool-config-package">
    <div className="v1-tool-config-heading"><strong>{props.section.title}</strong>
      {typeof descriptor.skill_version === "string" ? <span className="v1-tool-domain-muted">v{descriptor.skill_version}</span> : null}
      <span className="v1-tool-config-state" data-complete={typeof descriptor.complete === "boolean" ? descriptor.complete : undefined} title={countDetail}>{descriptor.complete === true ? "配置完整" : descriptor.complete === false ? "需要配置" : "-"}</span>
    </div>
    {issues.length > 0 ? <div className="v1-tool-config-issues">{issues.map((key) => <span key={key}>{FIELD_LABELS[key]} {String(descriptor[key])}</span>)}</div> : null}
    {Array.isArray(descriptor.items) && descriptor.items.length > 0 ? <>
      <Button size="sm" variant="ghost" className="v1-tool-domain-raw-toggle" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>{expanded ? "收起" : "查看"} {descriptor.items.length} 项配置声明<ChevronDown aria-hidden="true" size={13} /></Button>
      {expanded ? <ToolResultValue value={descriptor.items} /> : null}
    </> : null}
  </section>;
}

/** Defers expensive Markdown work for long results until the user expands their body.
 * 对长结果延后昂贵的 Markdown 处理，直到用户展开正文。
 * Parameter text is exact declared Markdown; returns its bounded preview and full-content toggle.
 * 参数 text 是精确声明 Markdown；返回有界预览与全文开关。 */
function BoundedToolMarkdown(props: { text: string }) {
  // expanded is presentation-only and never mutates the persisted Markdown.
  // expanded 仅用于展示，绝不修改持久化 Markdown。
  const [expanded, setExpanded] = React.useState(false);
  return <div className="v1-tool-domain-markdown">{props.text.length <= 1600 || expanded ? <MarkdownMessage content={props.text} eager /> : <div className="v1-tool-domain-text">{props.text.slice(0, 900)}</div>}{props.text.length > 1600 ? <Button size="sm" variant="ghost" onClick={() => setExpanded(!expanded)}>{expanded ? "收起全文" : `展开全文（${props.text.length} 字符）`}</Button> : null}</div>;
}

/** Keeps the complete input and output behind a remembered, independently expandable details control.
 * 将完整输入输出放在独立记忆展开状态的详情控件后。
 * Parameters id, input and output are authoritative event facts; returns copyable raw details.
 * 参数 id、input 与 output 是权威事件事实；返回可复制的原始详情。 */
export function ConversationV1ToolRawDetails(props: { id: string; input: ToolPayload; output?: ToolPayload }) {
  // collapseId binds details to the stable event identity.
  // collapseId 将详情绑定到稳定事件身份。
  const collapseId = `${props.id}:raw`;
  const [expanded, setExpanded] = useConversationMeasuredExpansion(collapseId, false);
  return <section className="v1-tool-domain-raw">
    <Button className="v1-tool-domain-raw-toggle" variant="ghost" size="sm" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>原始详情<ChevronDown aria-hidden="true" size={14} /></Button>
    <ConversationMeasuredCollapse collapseId={collapseId} expanded={expanded}>{expanded ? <><ToolRawPayload title="输入" value={props.input} />{props.output !== undefined ? <ToolRawPayload title="输出" value={props.output} /> : null}</> : null}</ConversationMeasuredCollapse>
  </section>;
}

/** Copies the complete retained payload and makes clipboard failure visible.
 * 复制完整保留载荷，并使剪贴板失败可见。
 * Parameters title and value identify one raw payload; returns selectable JSON plus copy feedback.
 * 参数 title 与 value 标识一个原始载荷；返回可选择 JSON 与复制反馈。 */
function ToolRawPayload(props: { title: string; value: ToolPayload }) {
  // text is computed only while raw details are mounted.
  // text 仅在原始详情挂载时计算。
  const text = React.useMemo(() => JSON.stringify(props.value, null, 2), [props.value]);
  // copyState provides explicit success and failure feedback instead of silent clipboard errors.
  // copyState 提供显式成功失败反馈，而不是静默剪贴板错误。
  const [copyState, setCopyState] = React.useState("");
  return <div className="v1-tool-domain-raw-payload"><div className="v1-tool-domain-raw-heading"><strong>{props.title}</strong><Button size="sm" variant="ghost" onClick={async () => { try { await navigator.clipboard.writeText(text); setCopyState("已复制"); } catch { setCopyState("复制失败，请选择文本复制"); } }}><Copy aria-hidden="true" size={13} />复制</Button><span role="status">{copyState}</span></div><pre>{text}</pre></div>;
}
