// SubagentModelEntry is one model subagents may run besides the session model, with the user's guidance.
// SubagentModelEntry 是子代理在会话模型之外可使用的一个模型，附带用户说明。
export interface SubagentModelEntry {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  providerId: string;
  // Provider-scoped model identifier.
  // Provider 范围内的模型标识。
  modelId: string;
  // Which tasks suit the model and what to watch for; the agent reads it verbatim.
  // 适合哪些任务以及注意事项；代理原样读取。
  note: string;
}

// SubagentModelsSnapshot is the system default allowlist with its bounds.
// SubagentModelsSnapshot 是系统默认白名单及其边界。
export interface SubagentModelsSnapshot {
  // Includes the calling session model in the allowed set.
  // 将调用会话模型纳入允许范围。
  includeSessionModel: boolean;
  models: SubagentModelEntry[];
  // Longest accepted note in Unicode scalar values, and the largest number of models.
  // 允许的最长备注（Unicode 标量值）与最大模型数。
  maxNoteChars: number;
  maxModels: number;
}

// SessionSubagentModelsSnapshot is one session's own list (null follows the system list) next to the system list.
// SessionSubagentModelsSnapshot 是一个会话的自有清单（null 表示跟随系统清单）及系统清单。
export interface SessionSubagentModelsSnapshot {
  // Complete session override, null while following the system.
  // 完整会话覆盖，跟随系统时为空。
  includeSessionModel: boolean | null;
  // Inclusion choice inherited with systemModels.
  // 与 systemModels 一起继承的包含选择。
  systemIncludeSessionModel: boolean;
  sessionId: string;
  models: SubagentModelEntry[] | null;
  systemModels: SubagentModelEntry[];
  maxNoteChars: number;
  maxModels: number;
}
