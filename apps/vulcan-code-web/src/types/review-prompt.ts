// ReviewPromptSystemDefault is the system default additional prompt the permission reviewer reads while a user is present.
// ReviewPromptSystemDefault 是用户在场时权限复核器读取的系统默认附加提示词。
export interface ReviewPromptSystemDefault {
  // Saved text; empty means none.
  // 已保存的文本；为空表示没有。
  prompt: string;
  // Longest accepted prompt in Unicode scalar values.
  // 允许的最长提示词（Unicode 标量值）。
  maxChars: number;
}

// SessionReviewPrompt is one session's own prompt (null follows the system default) next to the system default.
// SessionReviewPrompt 是一个会话的自有提示词（null 表示跟随系统默认值）及系统默认值。
export interface SessionReviewPrompt {
  sessionId: string;
  prompt: string | null;
  systemPrompt: string;
  maxChars: number;
}
