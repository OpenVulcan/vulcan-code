import { useMcpText } from "./mcp-text"
import * as React from "react"
import { Button } from "@/components/ui/button"
import { InlineDisclosure } from "@/components/ui/disclosure"

// schemaObject recognizes JSON object schemas without assuming optional keyword shapes.
// schemaObject 识别 JSON 对象 Schema，不猜测可选关键字结构。
export function schemaObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

// McpSchemaDetail renders structured properties and keeps the complete raw schema accessible.
// McpSchemaDetail 渲染结构化属性，并始终保留完整原始 Schema 的查看入口。
export function McpSchemaDetail(props: { schema: unknown; title: string }) {
  // text follows the shared application locale.
  // text 遵循共享应用语言。
  const text = useMcpText()

  // copyState reports clipboard failures instead of claiming success unconditionally.
  // copyState 报告剪贴板失败，不无条件宣称成功。
  const [copyState, setCopyState] = React.useState("")
  // json preserves every keyword, including unknown extensions and boolean schemas.
  // json 保留全部关键字，包括未知扩展与布尔 Schema。
  const json = JSON.stringify(props.schema, null, 2)
  return <section className="mcp-schema"><h4>{props.title}</h4>
    {props.schema === null ? <p>{text("服务器没有提供", "Not provided by the server")}</p> : <>
      <SchemaProperties schema={props.schema} depth={0} />
      <InlineDisclosure className="mcp-schema-raw" keepMounted label={text("查看原始 JSON", "View raw JSON")} openLabel={text("收起原始 JSON", "Hide raw JSON")}>
        <Button variant="outline" size="sm" onClick={async () => {
          try { await navigator.clipboard.writeText(json); setCopyState(text("已复制", "Copied")) }
          catch { setCopyState(text("复制失败，请手动选择", "Copy failed; select manually")) }
        }}>{text("复制", "Copy")}</Button><span role="status">{copyState}</span><pre>{json}</pre>
      </InlineDisclosure>
    </>}
  </section>
}

// SchemaProperties expands nested object and array fields while raw JSON remains the full oracle.
// SchemaProperties 展开嵌套对象与数组字段，完整依据仍为原始 JSON。
function SchemaProperties(props: { schema: unknown; depth: number }) {
  // text follows the shared application locale.
  // text 遵循共享应用语言。
  const text = useMcpText()

  if (!schemaObject(props.schema)) return <code>{JSON.stringify(props.schema)}</code>
  // schema, properties, and required are validated against their JSON types before rendering.
  // schema、properties 和 required 在渲染前按 JSON 类型校验。
  const schema = props.schema
  const properties = schemaObject(schema.properties) ? Object.entries(schema.properties) : []
  const required = Array.isArray(schema.required) ? schema.required : []
  if (props.depth > 8) return <p>{text("层级太深，请在原始 JSON 里查看", "Too deep; see the raw JSON")}</p>
  return <div className="mcp-schema-fields">
    {properties.map(([name, field]) => <div key={name} className="mcp-schema-field">
      <strong>{name}</strong> <span>{required.includes(name) ? text("必填", "Required") : text("可选", "Optional")}</span>
      {schemaObject(field) && <>
        <code>{typeof field.type === "string" ? field.type : JSON.stringify(field.type)}</code>
        {typeof field.description === "string" && <p>{field.description}</p>}
        {Object.entries(field).filter(([key]) => !["type", "description", "properties", "items"].includes(key)).map(([key, value]) => <div key={key}><code>{key}</code>: <code>{JSON.stringify(value)}</code></div>)}
        {("properties" in field || "items" in field) && <InlineDisclosure label={text("展开子字段", "Show sub-fields")} openLabel={text("收起子字段", "Hide sub-fields")}><SchemaProperties schema={"items" in field ? field.items : field} depth={props.depth + 1} /></InlineDisclosure>}
      </>}
      {!schemaObject(field) && <code>{JSON.stringify(field)}</code>}
    </div>)}
    {properties.length === 0 && <code>{JSON.stringify(schema)}</code>}
  </div>
}
