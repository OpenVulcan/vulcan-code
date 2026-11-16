import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { useI18n } from "@/i18n";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/ui-kit/primitives/table";

/** Describes one tool definition using the field names assigned by its encoder. */
/** 使用编码器指定的字段解析单条工具定义。 */
interface RequestToolDeclaration {
  /** Function name, or the explicit provider-native tool type. */
  /** 函数名或明确的供应商原生工具类型。 */
  name: string;
  /** Optional Markdown description supplied by the tool producer. */
  /** 工具生产者提供的可选 Markdown 说明。 */
  description?: string;
  /** Input Schema, when the declaration supplies one. */
  /** 声明提供时记录输入 Schema。 */
  schema: unknown;
  /** Alternate input format for a freeform tool. */
  /** 自由格式工具使用的备选输入格式。 */
  format: unknown;
}

// These keywords describe branch relationships. The branches retain their local required sets.
// 这些关键字描述分支关系，各分支保留各自的必填集合。
const COMBINATORS = [
  ["oneOf", "requestContext.schemaOneOf"],
  ["anyOf", "requestContext.schemaAnyOf"],
  ["allOf", "requestContext.schemaAllOf"],
] as const;

// Bound recursive preview depth to keep a large declaration navigable in the detail pane.
// 限制预览递归深度，让大型声明在详情面板内仍便于浏览。
const MAX_TOOL_SCHEMA_DEPTH = 8;

/** Returns a JSON object when the decoded value has object shape. */
/** 已解码值具有对象形态时返回 JSON 对象。 */
function schemaObject(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown> : undefined;
}

/** Extracts the display fields of one wrapped, flat, or native tool declaration. */
/** 提取一条包装式、扁平式或原生工具声明的展示字段。 */
function declaration(value: unknown): RequestToolDeclaration | undefined {
  // Source shape distinguishes a Chat/Cohere function wrapper from flat declarations.
  // 来源结构用来区分 Chat/Cohere 函数包装与扁平声明。
  const entry = schemaObject(value);
  if (!entry) return undefined;
  // Anthropic uses input_schema. OpenAI, Cohere, Gemini and the simulator use parameters.
  // Anthropic 使用 input_schema。OpenAI、Cohere、Gemini 和模拟使用 parameters。
  const tool = Object.hasOwn(entry, "function") ? schemaObject(entry.function) : entry;
  if (!tool) return undefined;
  // Provider-native tools may have a type instead of a function name.
  // 供应商原生工具可能提供类型而非函数名称。
  const name = typeof tool.name === "string" ? tool.name : tool.type;
  if (typeof name !== "string") return undefined;
  return {
    name,
    description: typeof tool.description === "string" ? tool.description : undefined,
    schema: Object.hasOwn(tool, "input_schema") ? tool.input_schema : tool.parameters,
    format: tool.format,
  };
}

/** Reads declarations from complete bounded JSON; incomplete nodes remain accessible in Raw. */
/** 从完整的有界 JSON 中读取声明，不完整的节点仍可在原始内容中查看。 */
export function readRequestToolDeclarations(content: string): RequestToolDeclaration[] {
  try {
    // Gemini emits a list of declarations inside a single provider tools node.
    // Gemini 在一个供应商工具节点中携带声明列表。
    const parsed = schemaObject(JSON.parse(content));
    if (!parsed) return [];
    // All other producers encode one declaration per node.
    // 其他生产者每个节点编码一条声明。
    const entries: unknown[] = Array.isArray(parsed.functionDeclarations) ? parsed.functionDeclarations : [parsed];
    return entries.flatMap((entry) => {
      // Only recognized declarations participate in the structured view.
      // 只有已识别的声明才进入结构化预览。
      const tool = declaration(entry);
      return tool ? [tool] : [];
    });
  } catch {
    return [];
  }
}

/** Renders declaration names, Markdown introductions and structured input Schemas. */
/** 渲染声明名称、Markdown 介绍和结构化输入 Schema。 */
export function RequestToolSchemaPreview({ declarations, truncated }: {
  /** Definitions decoded from the selected node. */
  /** 从选中节点解码得到的定义。 */
  declarations: RequestToolDeclaration[];
  /** Whether the raw node reached its budget. */
  /** 原始节点是否达到显示预算。 */
  truncated: boolean;
}) {
  const { t } = useI18n();
  if (!declarations.length) {
    return <p className="vk-request-context-text">{t(truncated ? "requestContext.truncated" : "requestContext.schemaUnavailable")}</p>;
  }
  return <div className="vk-request-context-text vk-request-context-schema">
    {declarations.map((tool, index) => <section key={index}>
      <div className="vk-request-context-schema-name">
        <span>{t("requestContext.schemaFunctionName")}</span>
        <code>{tool.name}</code>
      </div>
      {tool.description ? <MarkdownMessage content={tool.description} eager defaultCodeWrap /> : null}
      {tool.schema === undefined ? (
        tool.format === undefined ? <p>{t("requestContext.schemaNoParameters")}</p>
          : <pre>{JSON.stringify(tool.format, null, 2)}</pre>
      ) : <SchemaBody schema={tool.schema} depth={0} />}
    </section>)}
  </div>;
}

/** Displays a declared type or union exactly as supplied at the current Schema level. */
/** 按当前 Schema 层提供的内容精确展示类型或联合类型。 */
function SchemaType({ schema }: { schema: unknown }) {
  const { t } = useI18n();
  // A missing type does not imply any particular type.
  // 缺少类型时不推断具体类型。
  const type = schemaObject(schema)?.type;
  if (typeof type === "string") return <code>{type}</code>;
  if (Array.isArray(type) && type.every((value) => typeof value === "string")) {
    return <code>{type.join(" | ")}</code>;
  }
  return <span>{t("toolDomain.model.untyped")}</span>;
}

/** Renders one Schema object and keeps each nested constraint in its own scope. */
/** 渲染一个 Schema 对象，并把每条嵌套约束保留在各自作用域内。 */
function SchemaBody({ schema, depth }: { schema: unknown; depth: number }) {
  const { t } = useI18n();
  // Boolean schemas and format-specific literals retain their exact source representation.
  // 布尔 Schema 与格式专用字面值保留精确的来源表示。
  const object = schemaObject(schema);
  if (!object) return <code>{JSON.stringify(schema)}</code>;
  if (depth >= MAX_TOOL_SCHEMA_DEPTH) return <p>{t("settings.mcp.tooDeepSeeThe")}</p>;
  // Project required-only branches as well as branches that define their own property Schema.
  // 同时投影只声明必填项的分支与定义了自身参数 Schema 的分支。
  const properties = schemaObject(object.properties) ?? {};
  // Only the current Schema's required array marks fields in this table as required.
  // 只有当前 Schema 的 required 数组能将本表字段标记为必填。
  const required = Array.isArray(object.required)
    ? object.required.filter((value): value is string => typeof value === "string") : [];
  // Required-only names are appended without guessing their types from sibling branches.
  // 追加只有必填声明的名称，无需从同级分支猜测其类型。
  const names = [...new Set([...Object.keys(properties), ...required])];
  return <>
    {typeof object.description === "string" ? <MarkdownMessage content={object.description} eager defaultCodeWrap /> : null}
    {names.length ? (
      <Table aria-label={t("toolDomain.model.parameters")} className="vk-request-context-schema-table">
        <TableHeader><TableRow>
          <TableHead scope="col">{t("toolDomain.model.parameters")}</TableHead>
          <TableHead scope="col">{t("environment.field.kind")}</TableHead>
          <TableHead scope="col">{t("toolDomain.model.required")}</TableHead>
          <TableHead scope="col">{t("environment.field.description")}</TableHead>
        </TableRow></TableHeader>
        <TableBody>{names.map((name) => {
          // A required-only name has no local declaration to inspect.
          // 只有必填声明的名称没有本地参数定义可供检查。
          const field = Object.hasOwn(properties, name) ? properties[name] : undefined;
          return <TableRow key={name}>
            <TableCell><code>{name}</code></TableCell>
            <TableCell><SchemaType schema={field} /></TableCell>
            <TableCell>{t(required.includes(name) ? "toolDomain.model.required" : "toolDomain.model.optional")}</TableCell>
            <TableCell><ParameterDetails schema={field} depth={depth + 1} /></TableCell>
          </TableRow>;
        })}</TableBody>
      </Table>
    ) : depth === 0 && object.type === "object" && !COMBINATORS.some(([keyword]) => Object.hasOwn(object, keyword)) ? (
      <p>{t("requestContext.schemaNoParameters")}</p>
    ) : Object.hasOwn(object, "type") ? <SchemaType schema={object} /> : null}
    <SchemaConstraints schema={object} depth={depth} />
  </>;
}

/** Shows a parameter's own description and nested constraints inside its table cell. */
/** 在参数的表格单元内展示说明和嵌套约束。 */
function ParameterDetails({ schema, depth }: { schema: unknown; depth: number }) {
  if (schema === undefined) return null;
  // Boolean values are valid JSON Schemas and carry no property descriptions.
  // 布尔值是合法 JSON Schema，不包含参数说明。
  const object = schemaObject(schema);
  if (!object) return <code>{JSON.stringify(schema)}</code>;
  return <>
    {typeof object.description === "string" ? <MarkdownMessage content={object.description} eager defaultCodeWrap /> : null}
    {Object.hasOwn(object, "properties") || Object.hasOwn(object, "required")
      ? <SchemaBody schema={{ ...object, description: undefined }} depth={depth} />
      : <SchemaConstraints schema={object} depth={depth} />}
  </>;
}

/** Displays supported constraints without flattening conditional or alternative requirements. */
/** 展示支持的约束，同时保留条件及备选项各自的必填要求。 */
function SchemaConstraints({ schema, depth }: { schema: Record<string, unknown>; depth: number }) {
  const { t } = useI18n();
  if (depth >= MAX_TOOL_SCHEMA_DEPTH) return <p>{t("settings.mcp.tooDeepSeeThe")}</p>;
  return <>
    {["enum", "const", "default", "$ref", "additionalProperties", "format", "pattern", "minimum", "maximum", "minLength", "maxLength", "minItems", "maxItems"].map((keyword) => Object.hasOwn(schema, keyword) ? (
      <div className="vk-request-context-schema-rule" key={keyword}>
        {keyword === "enum" ? <span>{t("requestContext.schemaAllowedValues")}</span>
          : keyword === "const" ? <span>{t("requestContext.schemaFixedValue")}</span>
          : <code>{keyword}</code>}
        {keyword === "enum" && Array.isArray(schema.enum)
          ? <span>{schema.enum.map((value, index) => <code key={index}>{JSON.stringify(value)}</code>)}</span>
          : <code>{JSON.stringify(schema[keyword])}</code>}
      </div>
    ) : null)}
    {COMBINATORS.map(([keyword, title]) => {
      // Alternatives are ordered exactly as declared by the producer.
      // 备选项按生产者声明的精确顺序展示。
      const branches = schema[keyword];
      if (!Array.isArray(branches)) return null;
      return <details className="vk-request-context-schema-branch" key={keyword} open>
        <summary>{t(title)} <code>{keyword}</code></summary>
        <ol>{branches.map((branch, index) => <li key={index}><SchemaBody schema={branch} depth={depth + 1} /></li>)}</ol>
      </details>;
    })}
    {["items", "if", "then", "else", "not"].map((keyword) => Object.hasOwn(schema, keyword) ? (
      <details className="vk-request-context-schema-branch" key={keyword}>
        <summary><code>{keyword}</code></summary>
        <SchemaBody schema={schema[keyword]} depth={depth + 1} />
      </details>
    ) : null)}
  </>;
}
