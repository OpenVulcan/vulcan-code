import type { EnvironmentValueScope } from "./planning-questions";

// EnvironmentVariableKindKey says whether a stored value is shown or kept hidden.
// EnvironmentVariableKindKey 表示存储值是否显示或保持隐藏。
export type EnvironmentVariableKindKey = "plain" | "secret";

// EnvironmentVariableEntry is one stored global or project variable; secret values arrive empty.
// EnvironmentVariableEntry 是一条已存储的全局或项目变量；密钥值以空字符串到达。
export interface EnvironmentVariableEntry {
  id: string;
  name: string;
  kind: EnvironmentVariableKindKey;
  value: string;
  description: string;
  global: boolean;
  projectIds: string[];
  createdAtMs: number;
  updatedAtMs: number;
}

// SessionEnvironmentVariableEntry is one variable a session keeps itself; secret values arrive empty.
// SessionEnvironmentVariableEntry 是会话自身保存的一个变量；密钥值以空字符串到达。
export interface SessionEnvironmentVariableEntry {
  name: string;
  kind: EnvironmentVariableKindKey;
  value: string;
  description: string;
  updatedAtMs: number;
}

// EnvironmentVariableRules are the naming rules and limits every editor shares.
// EnvironmentVariableRules 是所有编辑器共用的命名规则与限制。
export interface EnvironmentVariableRules {
  recommendedPrefix: string;
  reservedNames: string[];
  nameMaxChars: number;
  valueMaxBytes: number;
  descriptionMaxChars: number;
}

// EnvironmentVariablesSnapshot is everything the environment-variable manager shows.
// EnvironmentVariablesSnapshot 是环境变量管理器展示的全部内容。
export interface EnvironmentVariablesSnapshot {
  variables: EnvironmentVariableEntry[];
  rules: EnvironmentVariableRules;
}

// SessionEnvironmentSnapshot lists the variables one session keeps itself.
// SessionEnvironmentSnapshot 列出一个会话自身保存的变量。
export interface SessionEnvironmentSnapshot {
  sessionId: string;
  variables: SessionEnvironmentVariableEntry[];
}

// EnvironmentVariableSaveRequest creates or updates one variable; an absent value keeps the stored one.
// EnvironmentVariableSaveRequest 创建或更新一条变量；缺省值时保留已存储的值。
export interface EnvironmentVariableSaveRequest {
  id?: string;
  name: string;
  value?: string;
  kind: EnvironmentVariableKindKey;
  description: string;
  global: boolean;
  projectIds: string[];
}

// EnvironmentVariableNameCheck reports facts about a name being entered.
// EnvironmentVariableNameCheck 报告正在输入的名称的相关事实。
export interface EnvironmentVariableNameCheck {
  name: string;
  invalidReason?: string | null;
  definedBySystem: boolean;
  hasRecommendedPrefix: boolean;
}

// EnvironmentValueSubmission is one value entered in the Ask panel.
// EnvironmentValueSubmission 是在 Ask 面板中输入的一个值。
export interface EnvironmentValueSubmission {
  name: string;
  value: string;
  scope: EnvironmentValueScope;
}

// EnvironmentValuesSubmitRequest answers one pending value request.
// EnvironmentValuesSubmitRequest 回答一条待处理取值请求。
export interface EnvironmentValuesSubmitRequest {
  requestId: string;
  declined: boolean;
  values: EnvironmentValueSubmission[];
}
