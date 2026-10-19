// Model parameter reference for get_model_extension_parameters: each returned model profile with its declared
// arguments, then the exact schema and the producer's instruction on request.
// get_model_extension_parameters 的模型参数参考：每个返回的模型简介及其声明参数，按需展开精确模式与产出方说明。
import { formatMessage, useI18n } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import type { ToolJsonValue, ToolPayload } from "@/service/conversation-event-window-store";
import { domainObject, resolveV1ToolDomainPresentation, v1ModelResult } from "@/components/conversation/tool-presentations/v1-tool-domain-presentation";
import { JsonView } from "./json-view";

// PROFILE_SECTIONS follow render_model_extension_parameters in provider_tool_bridge.rs.
// PROFILE_SECTIONS 遵循 provider_tool_bridge.rs 中 render_model_extension_parameters 的返回契约。
const PROFILE_SECTIONS = [
  ["characteristics", "toolDomain.model.characteristics"],
  ["constraints", "toolDomain.model.constraints"],
  ["limitations", "toolDomain.model.limitations"],
  ["resolution_policy", "toolDomain.model.resolution"],
  ["duration_policy", "toolDomain.model.duration"],
  ["format_policy", "toolDomain.model.format"],
  ["evidence_refs", "toolDomain.model.evidence"],
] as const;

/** Shows one declared profile value: a list, plain text, or the exact structured source.
 * 展示一个声明的简介值：列表、纯文本或精确的结构化源码。 */
function ProfileValue(props: { value: ToolJsonValue }) {
  if (Array.isArray(props.value)) return <ul>{props.value.map((value, index) => <li key={index}><ProfileValue value={value} /></li>)}</ul>;
  if (props.value !== null && typeof props.value === "object") return <JsonView value={props.value} />;
  return <>{String(props.value)}</>;
}

/** One top-level schema property: name, type, requiredness, description, allowed values and default.
 * 一个顶层模式属性：名称、类型、必填状态、说明、可选值与默认值。 */
function ParameterRow(props: { name: string; value: ToolJsonValue; required: boolean }) {
  const { t } = useI18n();
  // schema is object-shaped only when the producer declares an object schema rather than a boolean schema.
  // schema 仅在产出方声明对象模式而非布尔模式时具有对象形态。
  const schema = domainObject(props.value);
  const type = schema?.type === undefined
    ? t("toolDomain.model.untyped")
    : Array.isArray(schema.type) ? schema.type.join(" | ") : String(schema.type);
  return (
    <li className="vk-param">
      <div className="vk-param-head">
        <code>{props.name}</code>
        <span className="vk-param-type">{type}</span>
        <span className="vk-param-req" data-required={props.required ? "" : undefined}>
          {t(props.required ? "toolDomain.model.required" : "toolDomain.model.optional")}
        </span>
      </div>
      {typeof schema?.description === "string" ? <p>{schema.description}</p> : null}
      {Array.isArray(schema?.enum) ? (
        <div className="vk-param-values">{schema.enum.map((value, index) => <code key={index}>{JSON.stringify(value)}</code>)}</div>
      ) : null}
      {schema?.default !== undefined ? (
        <div className="vk-param-default">{t("toolDomain.model.default")}<code>{JSON.stringify(schema.default)}</code></div>
      ) : null}
    </li>
  );
}

/** One model profile: identity, description, input modes, profile sections and parameters.
 * 一个模型简介：身份、说明、输入方式、简介分区与参数。 */
function ModelProfile(props: { model: ToolPayload }) {
  const { t } = useI18n();
  // schema and properties are the declared args_schema and its top-level properties; allOf is not flattened.
  // schema 与 properties 为声明的 args_schema 及其顶层属性；不摊平 allOf。
  const schema = domainObject(props.model.args_schema);
  const properties = domainObject(schema?.properties);
  // required describes only the root required array; conditional requirements stay in the exact schema.
  // required 仅描述根级 required 数组；条件必填要求保留在精确模式中。
  const required = Array.isArray(schema?.required) ? schema.required : [];
  const modes = Array.isArray(props.model.input_modes) ? props.model.input_modes : [];
  return (
    <article className="vk-model">
      <header className="vk-model-head">
        <strong>{String(props.model.display_name)}</strong>
        <code>{String(props.model.model)}</code>
      </header>
      {typeof props.model.description === "string" && props.model.description ? <p className="vk-model-desc">{props.model.description}</p> : null}
      {modes.length > 0 ? (
        <div className="vk-chips" aria-label={t("toolDomain.model.inputModes")}>
          {modes.map((mode, index) => <span className="vk-badge" key={index}>{String(mode)}</span>)}
        </div>
      ) : null}
      {PROFILE_SECTIONS.map(([key, title]) => {
        const value = props.model[key];
        if (value === null || value === undefined || (Array.isArray(value) && value.length === 0)) return null;
        return <section className="vk-model-section" key={key}><h4>{t(title)}</h4><ProfileValue value={value} /></section>;
      })}
      <section className="vk-model-section">
        <h4>{properties ? formatMessage(t("toolDomain.model.parameterCount"), { count: Object.keys(properties).length }) : t("toolDomain.model.parameters")}</h4>
        {properties ? (
          <ul className="vk-params">
            {Object.entries(properties).map(([name, value]) => <ParameterRow key={name} name={name} required={required.includes(name)} value={value} />)}
          </ul>
        ) : null}
        {schema?.allOf !== undefined || schema?.anyOf !== undefined || schema?.oneOf !== undefined
          ? <p className="vk-model-note">{t("toolDomain.model.compositeNote")}</p> : null}
        <details className="vk-model-more">
          <summary>{t("toolDomain.model.fullSchema")}</summary>
          <JsonView value={props.model.args_schema} />
        </details>
      </section>
    </article>
  );
}

/** Renders the model profiles a parameters lookup returned, or its explicit empty or failure state.
 * 渲染参数查询返回的模型简介，或其明确的空值与失败状态。 */
export function ModelParametersView(props: { event: ToolCallEvent }) {
  const { t } = useI18n();
  // diagnostic keeps the canonical failure projection even when no model list was returned.
  // diagnostic 在未返回模型列表时仍保留规范错误投影。
  const diagnostic = resolveV1ToolDomainPresentation(props.event, t).diagnostic;
  if (diagnostic) return <pre className="vk-tool-error" role="alert">{diagnostic}</pre>;
  // result is decoded by the canonical model-output boundary, not by probing alternate fields.
  // result 由规范模型输出边界解码，不探测替代字段。
  const result = v1ModelResult(props.event.output);
  const payload = domainObject(result);
  if (!payload || !Array.isArray(payload.models)) {
    return <p className="vk-tool-note" role="status">{typeof result === "string" ? result : t("toolDomain.model.noParameters")}</p>;
  }
  // models are validated before rendering so a malformed contract never becomes a partial profile.
  // models 在渲染前校验，避免无效契约变为不完整的简介。
  const models = payload.models.map(domainObject);
  if (models.some((model) => model === null || typeof model.model !== "string" || typeof model.display_name !== "string" || model.args_schema === undefined)) {
    return <pre className="vk-tool-error" role="alert">{t("toolDomain.model.invalid")}</pre>;
  }
  return (
    <div className="vk-models" data-model-parameters>
      {models.length === 0
        ? <p className="vk-tool-note">{t("toolDomain.model.noModels")}</p>
        : models.map((model, index) => <ModelProfile key={index} model={model!} />)}
      {typeof payload.instruction === "string" && payload.instruction ? (
        <details className="vk-model-more">
          <summary>{t("toolDomain.model.instruction")}</summary>
          <p className="vk-model-desc">{payload.instruction}</p>
        </details>
      ) : null}
    </div>
  );
}
