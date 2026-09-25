import * as React from "react"
import { Button } from "@/components/ui/button"
import { InlineDisclosure } from "@/components/ui/disclosure"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { parseMcpBulkPairs, validateMcpPairs } from "./mcp-form-model"
import { useI18n, formatMessage } from "@/i18n"

// McpStringRows keeps exact ordered values and requires explicit bulk import confirmation.
// McpStringRows 保持精确有序值，批量导入需要显式确认。
export function McpStringRows(props: { label: string; values: string[]; onChange: (rows: string[]) => void; placeholder: string; exact?: boolean }) {
  const { t } = useI18n();
  // text follows the shared application locale.
  // text 遵循共享应用语言。

  // bulk is an uncommitted multiline import draft.
  // bulk 是尚未应用的多行导入草稿。
  const [bulk, setBulk] = React.useState("")
  const preview = props.exact ? bulk.split(/\r?\n/) : bulk.split(/\r?\n/).map(line => line.trim()).filter(Boolean)
  return <section className="mcp-form-rows"><header className="mcp-form-rows-header"><strong>{props.label}</strong>
    <Button variant="outline" size="sm" onClick={() => props.onChange([...props.values, ""])}>{t("settings.mcp.addRow")}</Button>
  </header>
    {props.values.map((value, index) => <div className="mcp-form-row" key={index}><span>{index + 1}</span>
      <Input aria-label={`${props.label} ${index + 1}`} placeholder={props.placeholder} value={value} onChange={e => props.onChange(props.values.map((current, position) => position === index ? e.target.value : current))} />
      <Button variant="ghost" size="sm" onClick={() => props.onChange(props.values.filter((_, position) => position !== index))}>{t("settings.mcp.remove")}</Button>
    </div>)}
    <InlineDisclosure className="mcp-bulk" label={t("settings.mcp.pasteMultipleRows")} openLabel={t("settings.mcp.hideBulkPaste")}>
      <Textarea aria-label={`${props.label} bulk`} placeholder={t("settings.mcp.onePerLine")} value={bulk} onChange={e => setBulk(e.target.value)} />
      <p>{formatMessage(t("settings.mcp.rowsToAdd"), { length: preview.length })}{props.exact && t("settings.mcp.emptyLinesCountAs")}</p><pre>{JSON.stringify(preview, null, 2)}</pre>
      <Button disabled={!bulk} variant="outline" size="sm" onClick={() => { props.onChange([...props.values, ...preview]); setBulk("") }}>{t("settings.mcp.addTheseRows")}</Button>
    </InlineDisclosure>
  </section>
}

// McpKeyValueRows separates keys and values, masks secrets, and validates imports before applying.
// McpKeyValueRows 分离键和值、遮蔽密钥，并在应用导入前校验。
export function McpKeyValueRows(props: { label: string; values: Array<{ key: string; value: string }>; onChange: (rows: Array<{ key: string; value: string }>) => void; secret?: boolean; valuePlaceholder?: string; headers?: boolean }) {
  const { t } = useI18n();
  // text follows the shared application locale.
  // text 遵循共享应用语言。

  // bulk and reveal are local editing controls and never reach the saved configuration.
  // bulk 与 reveal 是本地编辑控件，不会进入保存配置。
  const [bulk, setBulk] = React.useState("")
  const [reveal, setReveal] = React.useState(false)
  const preview = parseMcpBulkPairs(bulk)
  const issue = validateMcpPairs([...props.values, ...preview], props.label, props.headers, t)
  return <section className="mcp-form-rows"><header className="mcp-form-rows-header"><strong>{props.label}</strong>
    {props.secret && <Button variant="ghost" size="sm" onClick={() => setReveal(value => !value)}>{reveal ? t("settings.mcp.hideValues") : t("settings.mcp.showValues")}</Button>}
    <Button variant="outline" size="sm" onClick={() => props.onChange([...props.values, { key: "", value: "" }])}>{t("settings.mcp.addRow")}</Button>
  </header>
    {props.values.map((row, index) => <div className="mcp-form-row pair" key={index}>
      <Input aria-label={`${props.label} key ${index + 1}`} placeholder="KEY" value={row.key} onChange={e => props.onChange(props.values.map((current, position) => position === index ? { ...current, key: e.target.value } : current))} />
      <Input autoComplete="off" type={props.secret && !reveal ? "password" : "text"} aria-label={`${props.label} value ${index + 1}`} placeholder={props.valuePlaceholder ?? "VALUE"} value={row.value} onChange={e => props.onChange(props.values.map((current, position) => position === index ? { ...current, value: e.target.value } : current))} />
      <Button variant="ghost" size="sm" onClick={() => props.onChange(props.values.filter((_, position) => position !== index))}>{t("settings.mcp.remove")}</Button>
    </div>)}
    <InlineDisclosure className="mcp-bulk" label={t("settings.mcp.pasteMultipleRows")} openLabel={t("settings.mcp.hideBulkPaste")}>
      <Textarea aria-label={`${props.label} bulk`} placeholder="KEY=VALUE" value={bulk} onChange={e => setBulk(e.target.value)} />
      <p>{formatMessage(t("settings.mcp.rowsToAddOne"), { length: preview.length })}</p><ul>{preview.map((row, index) => <li key={index}>{row.key} = {props.secret ? "••••••" : row.value}</li>)}</ul>
      {bulk && issue && <p role="alert">{issue}</p>}<Button disabled={!bulk || Boolean(issue)} variant="outline" size="sm" onClick={() => { props.onChange([...props.values, ...preview]); setBulk("") }}>{t("settings.mcp.addTheseRows")}</Button>
    </InlineDisclosure>
  </section>
}
