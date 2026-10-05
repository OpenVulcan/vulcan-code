// Declarative plugin settings layout as authored in a `kind: settings` mount; Core validates it before serving.
// `kind: settings` 挂载中编写的声明式插件设置版面；Core 在提供前已完成校验。
// The authoritative shape is docs/contracts/plugin-settings-layout.schema.json, generated from the Rust types.
// 权威结构为由 Rust 类型生成的 docs/contracts/plugin-settings-layout.schema.json。

import type { PluginDataBlock, PluginDataSource } from "./plugin-data-view";

// PLUGIN_SETTINGS_LAYOUT_VERSION is the only layout format this renderer understands.
// PLUGIN_SETTINGS_LAYOUT_VERSION 是此渲染器理解的唯一版面格式。
export const PLUGIN_SETTINGS_LAYOUT_VERSION = 1;

// PluginSettingsScalar is a configuration value a select option or condition may hold.
// PluginSettingsScalar 是下拉选项或条件可持有的配置值。
export type PluginSettingsScalar = string | number | boolean;

// PluginSettingsOption is one select choice.
// PluginSettingsOption 是一个下拉选项。
export interface PluginSettingsOption {
  value: string | number;
  label_message_key?: string;
}

// PluginSettingsOptionGroup is one titled checklist group.
// PluginSettingsOptionGroup 是一个带标题的清单分组。
export interface PluginSettingsOptionGroup {
  id: string;
  title_message_key: string;
  values: string[];
}

// PluginSettingsField edits one top-level configuration property.
// PluginSettingsField 编辑一个顶层配置属性。
export interface PluginSettingsField {
  type: "field";
  key: string;
  control: "switch" | "select" | "text" | "number" | "checklist";
  label_message_key: string;
  description_message_key?: string;
  placeholder_message_key?: string;
  options?: PluginSettingsOption[];
  value_format?: "plain" | "duration_ms" | "days";
  option_groups?: PluginSettingsOptionGroup[];
  selection?: "include" | "exclude";
  visible_when?: { key: string; values: PluginSettingsScalar[] };
}

// PluginSettingsCredential stores one write-only secret through declared actions.
// PluginSettingsCredential 通过已声明 Action 保存一个只写密钥。
export interface PluginSettingsCredential {
  type: "credential";
  id: string;
  label_message_key: string;
  description_message_key?: string;
  placeholder_message_key?: string;
  status_action: string;
  set_action: string;
  set_argument: string;
  delete_action?: string;
}

// PluginSettingsAction runs one declared action without arguments.
// PluginSettingsAction 无参数运行一个已声明 Action。
export interface PluginSettingsAction {
  type: "action";
  id: string;
  action: string;
  label_message_key: string;
  description_message_key?: string;
  success_message_key?: string;
}

// PluginSettingsItem is one row of a settings section.
// PluginSettingsItem 是设置区块中的一行。
export type PluginSettingsItem = PluginSettingsField | PluginSettingsCredential | PluginSettingsAction;

// PluginSettingsSection is one titled section card: settings rows, or a read-only data view.
// PluginSettingsSection 是一张带标题的区块卡片：设置行或只读数据视图。
export interface PluginSettingsSection {
  id: string;
  title_message_key: string;
  description_message_key?: string;
  layout?: "rows" | "inline";
  items?: PluginSettingsItem[];
  data?: PluginDataSource;
  blocks?: PluginDataBlock[];
  links?: Array<{ url: string; label_message_key: string }>;
}

// PluginSettingsHeader is the availability pill, metadata links and package documents beside the tabs.
// PluginSettingsHeader 是标签旁的可用状态、元数据链接与包内文档。
export interface PluginSettingsHeader {
  status?: { ready_message_key: string; unavailable_message_key: string };
  links?: Array<{ url: string; label_message_key: string; aria_label_message_key?: string; icon?: "link" | "github" }>;
  documents?: Array<{
    id: string; asset: string; label_message_key: string; title_message_key: string;
    description_message_key?: string; aria_label_message_key?: string; icon?: "document" | "license";
  }>;
}

// PluginSettingsTab groups sections under one page tab.
// PluginSettingsTab 将区块归入一个页面标签。
export interface PluginSettingsTab {
  id: string;
  title_message_key: string;
  sections: string[];
}

// PluginSettingsLayout is one complete settings page.
// PluginSettingsLayout 是一个完整设置页面。
export interface PluginSettingsLayout {
  schema_version: number;
  config?: { read_action: string; write_action: string };
  header?: PluginSettingsHeader;
  tabs?: PluginSettingsTab[];
  sections: PluginSettingsSection[];
}

// PluginCredentialStatus is the host-read part of a credential status action result.
// PluginCredentialStatus 是凭据状态 Action 结果中宿主读取的部分。
export interface PluginCredentialStatus {
  configured: boolean;
  validationStatus?: "unknown" | "untested" | "valid" | "invalid";
}

// JsonSchemaNode is the subset of JSON Schema keywords the renderer checks before Core re-validates.
// JsonSchemaNode 是渲染器在 Core 复核前检查的 JSON Schema 关键字子集。
export interface JsonSchemaNode {
  type?: string;
  enum?: unknown[];
  pattern?: string;
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
  items?: JsonSchemaNode;
  properties?: Record<string, JsonSchemaNode>;
}

// FieldIssue explains why a draft value is not yet savable.
// FieldIssue 说明草稿值为何尚不可保存。
export type FieldIssue = { kind: "range"; min: number; max: number } | { kind: "invalid" };

// parsePluginSettingsLayout decodes the served layout and rejects formats this renderer does not know.
// parsePluginSettingsLayout 解码服务提供的版面，并拒绝此渲染器不认识的格式。
export function parsePluginSettingsLayout(source: string): PluginSettingsLayout {
  const layout = JSON.parse(source) as PluginSettingsLayout;
  if (layout?.schema_version !== PLUGIN_SETTINGS_LAYOUT_VERSION || !Array.isArray(layout.sections)) {
    throw new Error(`Plugin settings layout must use schema_version ${PLUGIN_SETTINGS_LAYOUT_VERSION}.`);
  }
  return layout;
}

// layoutFields lists every field of a layout in document order.
// layoutFields 按文档顺序列出版面中的全部字段。
export function layoutFields(layout: PluginSettingsLayout): PluginSettingsField[] {
  return layout.sections.flatMap((section) => sectionFields(section));
}

// sectionFields lists the fields of one section; data sections have none.
// sectionFields 列出单个区块的字段；数据区块没有字段。
export function sectionFields(section: PluginSettingsSection): PluginSettingsField[] {
  return (section.items ?? []).filter((item): item is PluginSettingsField => item.type === "field");
}

// propertySchema returns the configuration schema node of one top-level key.
// propertySchema 返回单个顶层键的配置 Schema 节点。
export function propertySchema(configSchema: unknown, key: string): JsonSchemaNode | undefined {
  return (configSchema as JsonSchemaNode | undefined)?.properties?.[key];
}

// valueIssue checks value against the supported keyword subset; Core remains the final authority.
// valueIssue 依据支持的关键字子集检查 value；Core 仍是最终权威。
export function valueIssue(schema: JsonSchemaNode | undefined, value: unknown): FieldIssue | null {
  if (!schema) return null;
  const invalid: FieldIssue = { kind: "invalid" };
  switch (schema.type) {
    case "boolean": if (typeof value !== "boolean") return invalid; break;
    case "integer": if (typeof value !== "number" || !Number.isInteger(value)) return invalid; break;
    case "number": if (typeof value !== "number" || !Number.isFinite(value)) return invalid; break;
    case "string": if (typeof value !== "string") return invalid; break;
    case "array":
      if (!Array.isArray(value)) return invalid;
      return value.some((entry) => valueIssue(schema.items, entry) !== null) ? invalid : null;
    default: break;
  }
  if (schema.enum && !schema.enum.some((candidate) => candidate === value)) return invalid;
  if (typeof value === "number") {
    const below = schema.minimum !== undefined && value < schema.minimum;
    const above = schema.maximum !== undefined && value > schema.maximum;
    if (below || above) {
      return schema.minimum !== undefined && schema.maximum !== undefined ? { kind: "range", min: schema.minimum, max: schema.maximum } : invalid;
    }
  }
  if (typeof value === "string") {
    const length = [...value].length;
    if ((schema.minLength !== undefined && length < schema.minLength) || (schema.maxLength !== undefined && length > schema.maxLength)) return invalid;
    if (schema.pattern !== undefined && !new RegExp(schema.pattern, "u").test(value)) return invalid;
  }
  return null;
}

// numberInputValue converts typed text for numeric properties, leaving unparsable text to fail validation.
// numberInputValue 为数值属性转换输入文本，无法解析的文本保留以便校验失败。
export function numberInputValue(schema: JsonSchemaNode | undefined, text: string): unknown {
  if (schema?.type !== "integer" && schema?.type !== "number") return text;
  return /^-?\d+(\.\d+)?$/.test(text.trim()) ? Number(text.trim()) : text;
}

// isFieldVisible evaluates a visibility condition against the current draft.
// isFieldVisible 依据当前草稿计算可见条件。
export function isFieldVisible(field: PluginSettingsField, draft: Record<string, unknown>): boolean {
  return field.visible_when === undefined || field.visible_when.values.some((value) => value === draft[field.visible_when!.key]);
}

// checklistUniverse lists every offered checklist value.
// checklistUniverse 列出清单提供的全部值。
export function checklistUniverse(field: PluginSettingsField): string[] {
  return (field.option_groups ?? []).flatMap((group) => group.values);
}

// checkedValues maps a stored array to the visibly checked entries according to the selection mode.
// checkedValues 根据选择模式把已存数组映射为可见勾选项。
export function checkedValues(field: PluginSettingsField, stored: unknown): Set<string> {
  const values = new Set(Array.isArray(stored) ? stored.filter((entry): entry is string => typeof entry === "string") : []);
  if (field.selection !== "exclude") return values;
  return new Set(checklistUniverse(field).filter((value) => !values.has(value)));
}

// storedValues maps checked entries back to the stored array, keeping stored values the layout does not offer.
// storedValues 把勾选项映射回已存数组，并保留版面未提供的已存值。
export function storedValues(field: PluginSettingsField, stored: unknown, checked: ReadonlySet<string>): string[] {
  const universe = new Set(checklistUniverse(field));
  const kept = Array.isArray(stored) ? stored.filter((entry): entry is string => typeof entry === "string" && !universe.has(entry)) : [];
  const offered = [...universe].filter((value) => (field.selection === "exclude" ? !checked.has(value) : checked.has(value)));
  return [...new Set([...kept, ...offered])].sort();
}

// formatOptionValue renders an option without its own label through the host locale.
// formatOptionValue 通过宿主语言渲染未提供标签的选项。
export function formatOptionValue(value: string | number, format: PluginSettingsField["value_format"], locale: string): string {
  if (typeof value !== "number" || format === undefined || format === "plain") return String(value);
  if (format === "days") return new Intl.NumberFormat(locale, { style: "unit", unit: "day", unitDisplay: "long" }).format(value);
  return value % 1000 === 0
    ? new Intl.NumberFormat(locale, { style: "unit", unit: "second", unitDisplay: "short" }).format(value / 1000)
    : new Intl.NumberFormat(locale, { style: "unit", unit: "millisecond", unitDisplay: "short" }).format(value);
}

// sameConfig compares two configuration objects structurally for dirty tracking.
// sameConfig 结构化比较两个配置对象，用于跟踪未保存状态。
export function sameConfig(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}
