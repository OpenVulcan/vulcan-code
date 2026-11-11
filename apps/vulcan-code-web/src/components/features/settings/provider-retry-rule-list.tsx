import * as React from "react"

import { useI18n } from "@/i18n"
import { PROVIDER_RESPONSE_HEADER_TIMEOUT_PATTERN, PROVIDER_RETRY_REGEX_PREFIX } from "@/lib/provider-retry-rules"
import { Button } from "@/ui-kit/primitives/button"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"

// One inline editor for an existing rule or a new entry.
// 用于现有规则或新增条目的单条行内编辑器。
export interface ProviderRetryRuleEditor {
  // Index in the settings draft, or null to append a new rule.
  // 设置草稿中的下标，追加新规则时为 null。
  index: number | null
  // Whether the content is stored with the regex prefix.
  // 内容是否带正则前缀存储。
  isRegex: boolean
  // Keyword text or regex pattern without the storage prefix.
  // 不含存储前缀的关键词文本或正则模式。
  content: string
}

// Supplies the draft rules, optional editor, and callbacks for local changes.
// 提供规则草稿、可选编辑器及本地变更回调。
interface ProviderRetryRuleListProps {
  // Ordered rules in the current settings draft.
  // 当前设置草稿中的有序规则。
  rules: readonly string[]
  // Built-in rules available for restoration.
  // 可恢复的内置规则。
  defaultRules: readonly string[]
  // Currently open editor, if any.
  // 当前打开的编辑器，可为空。
  editor: ProviderRetryRuleEditor | null
  // Replaces the settings draft's rule list.
  // 替换设置草稿的规则列表。
  onRulesChange: (rules: string[]) => void
  // Opens, updates, or closes the inline editor.
  // 打开、更新或关闭行内编辑器。
  onEditorChange: (editor: ProviderRetryRuleEditor | null) => void
}

// Renders each draft rule with independent actions and an editor with a type-specific example.
// 为每条规则草稿渲染独立操作，并为编辑器提供对应类型的样例。
export function ProviderRetryRuleList({ rules, defaultRules, editor, onRulesChange, onEditorChange }: ProviderRetryRuleListProps) {
  // Localize the list actions and the editing controls.
  // 本地化列表操作和编辑控件。
  const { t } = useI18n()
  // Connect the rule input to the displayed example.
  // 将规则输入框关联到显示的样例。
  const exampleId = React.useId()
  // Compare actual rule order so restoring defaults is only available when the draft differs.
  // 比较规则的实际顺序，仅在草稿有差异时提供恢复默认。
  const matchesDefaults = rules.length === defaultRules.length && rules.every((rule, index) => rule === defaultRules[index])
  // Store only meaningful input and reserve the prefix for explicit regex mode.
  // 仅存储有效输入，将正则前缀保留给显式正则模式。
  const content = editor?.content.trim() ?? ""
  const reservedKeyword = Boolean(editor && !editor.isRegex && content.startsWith(PROVIDER_RETRY_REGEX_PREFIX))
  // Use the same response-header pattern the conversation error action generates.
  // 采用与会话错误操作生成的响应头模式相同的样例。
  const example = editor?.isRegex ? PROVIDER_RESPONSE_HEADER_TIMEOUT_PATTERN.source : "connection reset"

  // Applies the open editor to its exact row, or appends a rule for a new entry.
  // 将打开的编辑器应用到精确行，新增时追加规则。
  function applyRule(): void {
    if (!editor || !content || reservedKeyword) return
    // Keep the persisted rule contract as a string with an explicit prefix for regex.
    // 持久化规则仍采用字符串契约，正则使用显式前缀。
    const rule = editor.isRegex ? PROVIDER_RETRY_REGEX_PREFIX + content : content
    onRulesChange(editor.index === null ? [...rules, rule] : rules.map((current, index) => index === editor.index ? rule : current))
    onEditorChange(null)
  }

  return (
    <fieldset className="context-basics-group">
      <legend>{t("settings.retry.errorKeywords")}</legend>
      {rules.length > 0 ? (
        <ul className="vk-retry-rule-list">
          {rules.map((rule, index) => {
            // Decode the explicit storage prefix for the row label and editor value.
            // 解读显式存储前缀，得到行标签及编辑器值。
            const isRegex = rule.startsWith(PROVIDER_RETRY_REGEX_PREFIX)
            const ruleContent = isRegex ? rule.slice(PROVIDER_RETRY_REGEX_PREFIX.length) : rule
            return (
              <li className="vk-form-row" key={`${index}:${rule}`}>
                <span className="vk-form-pref-text">
                  <small className="vk-form-note">{t(isRegex ? "settings.retry.regexRule" : "settings.retry.keywordRule")}</small>
                  <code>{ruleContent}</code>
                </span>
                <div className="vk-retry-rule-actions">
                  <Button disabled={editor !== null} onClick={() => onEditorChange({ index, isRegex, content: ruleContent })} size="sm" type="button" variant="ghost">{t("settings.retry.editRule")}</Button>
                  <Button disabled={editor !== null} onClick={() => onRulesChange(rules.filter((_, currentIndex) => currentIndex !== index))} size="sm" type="button" variant="ghost">{t("settings.retry.deleteRule")}</Button>
                </div>
              </li>
            )
          })}
        </ul>
      ) : <p className="vk-form-note">{t("settings.retry.emptyRules")}</p>}
      {editor ? (
        <div className="vk-form-pref-text vk-retry-rule-editor">
          <Select value={editor.isRegex ? "regex" : "keyword"} onValueChange={(value) => onEditorChange({ ...editor, isRegex: value === "regex" })}>
            <SelectTrigger aria-label={t("settings.retry.ruleType")} className="vk-form-w-md"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="keyword">{t("settings.retry.keywordRule")}</SelectItem>
              <SelectItem value="regex">{t("settings.retry.regexRule")}</SelectItem>
            </SelectContent>
          </Select>
          <Input
            aria-describedby={exampleId}
            aria-invalid={reservedKeyword}
            aria-label={t("settings.retry.ruleContent")}
            onChange={(event) => onEditorChange({ ...editor, content: event.currentTarget.value })}
            spellCheck={false}
            value={editor.content}
          />
          <p className="vk-form-note" id={exampleId}>{t("settings.retry.ruleExample")} <code>{example}</code></p>
          {reservedKeyword ? <p className="vk-field-error" role="alert">{t("settings.retry.ruleReservedPrefix")}</p> : null}
          <div className="vk-retry-rule-actions">
            <Button disabled={!content || reservedKeyword} onClick={applyRule} size="sm" type="button">{t(editor.index === null ? "settings.retry.addRule" : "settings.retry.applyRule")}</Button>
            <Button onClick={() => onEditorChange(null)} size="sm" type="button" variant="outline">{t("conversation.action.cancel")}</Button>
          </div>
        </div>
      ) : (
        <div className="vk-retry-rule-actions">
          <Button onClick={() => onEditorChange({ index: null, isRegex: false, content: "" })} size="sm" type="button" variant="outline">{t("settings.retry.addRule")}</Button>
          <Button disabled={matchesDefaults} onClick={() => onRulesChange([...defaultRules])} size="sm" type="button" variant="outline">{t("settings.retry.restoreDefaultKeywords")}</Button>
        </div>
      )}
    </fieldset>
  )
}
