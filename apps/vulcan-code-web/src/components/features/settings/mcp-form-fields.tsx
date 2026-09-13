import { useMcpText } from "./mcp-text"
import * as React from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { parseMcpBulkPairs, validateMcpPairs } from "./mcp-form-model"

// McpStringRows keeps exact ordered values and requires explicit bulk import confirmation.
// McpStringRows 保持精确有序值，批量导入需要显式确认。
export function McpStringRows(props: { label: string; values: string[]; onChange: (rows: string[]) => void; placeholder: string; exact?: boolean }) {
  // text follows the shared application locale.
  // text 遵循共享应用语言。
  const text = useMcpText()

  // bulk is an uncommitted multiline import draft.
  // bulk 是尚未应用的多行导入草稿。
  const [bulk, setBulk] = React.useState("")
  const preview = props.exact ? bulk.split(/\r?\n/) : bulk.split(/\r?\n/).map(line => line.trim()).filter(Boolean)
  return <section className="mcp-form-rows"><header className="mcp-form-rows-header"><strong>{props.label}</strong>
    <Button variant="outline" size="sm" onClick={() => props.onChange([...props.values, ""])}>{text("添加一行", "Add row")}</Button>
  </header>
    {props.values.map((value, index) => <div className="mcp-form-row" key={index}><span>{index + 1}</span>
      <Input aria-label={`${props.label} ${index + 1}`} placeholder={props.placeholder} value={value} onChange={e => props.onChange(props.values.map((current, position) => position === index ? e.target.value : current))} />
      <Button variant="ghost" size="sm" onClick={() => props.onChange(props.values.filter((_, position) => position !== index))}>{text("移除", "Remove")}</Button>
    </div>)}
    <details><summary>{text("批量粘贴", "Bulk paste")}</summary><Textarea aria-label={`${props.label} bulk`} value={bulk} onChange={e => setBulk(e.target.value)} />
      <p>{preview.length}{text("行", "rows")}{props.exact && text("· 空行也是参数", "Empty lines are arguments")}</p><pre>{JSON.stringify(preview, null, 2)}</pre>
      <Button disabled={!bulk} variant="outline" size="sm" onClick={() => { props.onChange([...props.values, ...preview]); setBulk("") }}>{text("确认追加", "Append rows")}</Button>
    </details>
  </section>
}

// McpKeyValueRows separates keys and values, masks secrets, and validates imports before applying.
// McpKeyValueRows 分离键和值、遮蔽密钥，并在应用导入前校验。
export function McpKeyValueRows(props: { label: string; values: Array<{ key: string; value: string }>; onChange: (rows: Array<{ key: string; value: string }>) => void; secret?: boolean; valuePlaceholder?: string; headers?: boolean }) {
  // text follows the shared application locale.
  // text 遵循共享应用语言。
  const text = useMcpText()

  // bulk and reveal are local editing controls and never reach the saved configuration.
  // bulk 与 reveal 是本地编辑控件，不会进入保存配置。
  const [bulk, setBulk] = React.useState("")
  const [reveal, setReveal] = React.useState(false)
  const preview = parseMcpBulkPairs(bulk)
  const issue = validateMcpPairs([...props.values, ...preview], props.label, props.headers, text)
  return <section className="mcp-form-rows"><header className="mcp-form-rows-header"><strong>{props.label}</strong>
    {props.secret && <Button variant="ghost" size="sm" onClick={() => setReveal(value => !value)}>{reveal ? text("隐藏值", "Hide values") : text("显示值", "Show values")}</Button>}
    <Button variant="outline" size="sm" onClick={() => props.onChange([...props.values, { key: "", value: "" }])}>{text("添加一行", "Add row")}</Button>
  </header>
    {props.values.map((row, index) => <div className="mcp-form-row pair" key={index}>
      <Input aria-label={`${props.label} key ${index + 1}`} placeholder="KEY" value={row.key} onChange={e => props.onChange(props.values.map((current, position) => position === index ? { ...current, key: e.target.value } : current))} />
      <Input autoComplete="off" type={props.secret && !reveal ? "password" : "text"} aria-label={`${props.label} value ${index + 1}`} placeholder={props.valuePlaceholder ?? "VALUE"} value={row.value} onChange={e => props.onChange(props.values.map((current, position) => position === index ? { ...current, value: e.target.value } : current))} />
      <Button variant="ghost" size="sm" onClick={() => props.onChange(props.values.filter((_, position) => position !== index))}>{text("移除", "Remove")}</Button>
    </div>)}
    <details><summary>{text("批量粘贴 KEY=VALUE", "Bulk paste")}</summary><Textarea aria-label={`${props.label} bulk`} value={bulk} onChange={e => setBulk(e.target.value)} />
      <p>{preview.length}{text("行", "rows")}</p><ul>{preview.map((row, index) => <li key={index}>{row.key} = {props.secret ? "••••••" : row.value}</li>)}</ul>
      {bulk && issue && <p role="alert">{issue}</p>}<Button disabled={!bulk || Boolean(issue)} variant="outline" size="sm" onClick={() => { props.onChange([...props.values, ...preview]); setBulk("") }}>{text("确认追加", "Append rows")}</Button>
    </details>
  </section>
}
