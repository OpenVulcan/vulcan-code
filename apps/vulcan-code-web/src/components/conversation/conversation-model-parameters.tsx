import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import type { ToolJsonValue, ToolPayload } from "@/service/conversation-event-window-store";
import { domainObject, resolveV1ToolDomainPresentation, v1ModelResult } from "./tool-presentations/v1-tool-domain-presentation";
import "@/styles/tool-domain.css";

// Profile sections follow render_model_extension_parameters in provider_tool_bridge.rs.
// 简介分区遵循 provider_tool_bridge.rs 中 render_model_extension_parameters 的返回契约。
const PROFILE_SECTIONS = [["characteristics", "特点"], ["constraints", "约束"], ["limitations", "限制"], ["resolution_policy", "分辨率"], ["duration_policy", "时长"], ["format_policy", "格式"], ["evidence_refs", "参考依据"]] as const;

/** Displays a declared profile value without recursively expanding generic key/value tables.
 * 展示声明的简介值，不递归展开通用键值表。
 * Parameter value is the original JSON member; returns a readable list, text or exact structured source.
 * 参数 value 为原始 JSON 字段；返回可读列表、文本或精确结构源码。
 */
function ProfileValue(props: { value: ToolJsonValue }) {
  if (Array.isArray(props.value)) return <ul>{props.value.map((value, index) => <li key={index}><ProfileValue value={value} /></li>)}</ul>;
  if (props.value !== null && typeof props.value === "object") return <pre className="tool-model-schema">{JSON.stringify(props.value, null, 2)}</pre>;
  return <>{String(props.value)}</>;
}

/** Presents one exact top-level JSON Schema property as name, type, requiredness and description.
 * 将一个精确的顶层 JSON Schema 属性展示为名称、类型、必填状态与说明。
 * Parameters name, value and required identify the original property; returns one compact parameter row.
 * 参数 name、value、required 标识原始属性；返回一个紧凑参数行。
 */
function ParameterRow(props: { name: string; value: ToolJsonValue; required: boolean }) {
  // schema is object-shaped only when the producer declares an object schema rather than a boolean schema.
  // schema 仅在产出方声明对象模式而非布尔模式时具有对象形态。
  const schema = domainObject(props.value);
  return <li className="tool-model-parameter">
    <div className="tool-model-parameter-name"><code>{props.name}</code><span className="tool-model-type">{schema?.type === undefined ? "未指定类型" : Array.isArray(schema.type) ? schema.type.join(" | ") : String(schema.type)}</span><span className={props.required ? "tool-model-required" : "tool-model-optional"}>{props.required ? "必填" : "可选"}</span></div>
    {typeof schema?.description === "string" ? <p>{schema.description}</p> : null}
    {Array.isArray(schema?.enum) ? <div className="tool-model-options">{schema.enum.map((value, index) => <code key={index}>{JSON.stringify(value)}</code>)}</div> : null}
    {schema?.default !== undefined ? <div className="tool-model-default">默认值：<code>{JSON.stringify(schema.default)}</code></div> : null}
  </li>;
}

/** Renders one native model profile and its schema, preserving full constraints in a disclosure.
 * 渲染一个原生模型简介与参数模式，在展开区域保留完整约束。
 * Parameter model owns the exact backend profile; returns a model identity header and compact parameter reference.
 * 参数 model 持有精确后端简介；返回模型身份标题与紧凑参数参考。
 */
function ModelParameterCard(props: { model: ToolPayload }) {
  // schema and properties are the declared args_schema and its top-level properties; allOf is not flattened.
  // schema 与 properties 为声明的 args_schema 及其顶层属性；不摊平 allOf。
  const schema = domainObject(props.model.args_schema);
  const properties = domainObject(schema?.properties);
  // required only describes the root required array; conditional requirements remain in the exact schema.
  // required 仅描述根级 required 数组；条件必填要求保留在精确模式中。
  const required = Array.isArray(schema?.required) ? schema.required : [];
  return <article className="tool-model-card">
    <header className="tool-model-heading"><strong>{String(props.model.display_name)}</strong><code>{String(props.model.model)}</code></header>
    {typeof props.model.description === "string" && props.model.description ? <p className="tool-model-description">{props.model.description}</p> : null}
    {Array.isArray(props.model.input_modes) && props.model.input_modes.length > 0 ? <div className="tool-model-options" aria-label="输入模式">{props.model.input_modes.map((mode, index) => <span key={index}>{String(mode)}</span>)}</div> : null}
    {PROFILE_SECTIONS.map(([key, title]) => props.model[key] === null || props.model[key] === undefined || (Array.isArray(props.model[key]) && props.model[key].length === 0) ? null : <section className="tool-model-profile" key={key}><h4>{title}</h4><ProfileValue value={props.model[key]} /></section>)}
    <section className="tool-model-parameters"><h4>参数定义{properties ? ` · ${Object.keys(properties).length} 项` : ""}</h4>
      {properties ? <ul>{Object.entries(properties).map(([name, value]) => <ParameterRow key={name} name={name} value={value} required={required.includes(name)} />)}</ul> : null}
      {schema?.allOf !== undefined || schema?.anyOf !== undefined || schema?.oneOf !== undefined ? <p className="tool-model-schema-note">含组合或条件约束，请展开完整参数定义查看。</p> : null}
      <details className="tool-model-schema-details"><summary>完整参数定义 · JSON Schema</summary><pre className="tool-model-schema">{JSON.stringify(props.model.args_schema, null, 2)}</pre></details>
    </section>
  </article>;
}

/** Displays available model profiles synchronously without invocation payload tabs or lazy loading.
 * 同步展示可用模型简介，不提供调用载荷标签或延迟加载。
 * Parameter event is the canonical parameters lookup; returns exact profiles or an explicit empty/error state.
 * 参数 event 为规范参数查询；返回精确简介或明确的空值、错误状态。
 */
export function ConversationModelParameters(props: { event: ToolCallEvent }) {
  // diagnostic retains the canonical failure projection even when no model list was returned.
  // diagnostic 在未返回模型列表时仍保留规范错误投影。
  const diagnostic = resolveV1ToolDomainPresentation(props.event).diagnostic;
  if (diagnostic) return <div className="v1-tool-domain-error" role="alert">{diagnostic}</div>;
  // result is decoded by the canonical model-output boundary, not by probing alternate fields.
  // result 由规范模型输出边界解码，不探测替代字段。
  const result = v1ModelResult(props.event.output);
  const payload = domainObject(result);
  if (!payload || !Array.isArray(payload.models)) return <div className="v1-tool-domain-muted" role="status">{typeof result === "string" ? result : "尚无可展示的模型参数"}</div>;
  // models are validated before rendering so malformed contracts never become partial cards.
  // models 在渲染前校验，避免无效契约变为不完整卡片。
  const models = payload.models.map(domainObject);
  if (models.some(model => model === null || typeof model.model !== "string" || typeof model.display_name !== "string" || model.args_schema === undefined)) return <div role="alert">模型参数返回格式无效</div>;
  return <div className="tool-model-parameters-result">
    {models.length === 0 ? <div className="v1-tool-domain-muted">当前没有可用模型</div> : models.map((model, index) => <ModelParameterCard key={index} model={model!} />)}
    {typeof payload.instruction === "string" && payload.instruction ? <details className="tool-model-instruction"><summary>调用说明</summary><p>{payload.instruction}</p></details> : null}
  </div>;
}
