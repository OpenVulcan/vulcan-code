import { serviceClient } from "@/service/webui-service-runtime";
import type { ReviewPromptSystemDefault, SessionReviewPrompt } from "@/types";

// loadReviewPromptSystemDefault reads the system default additional review prompt.
// loadReviewPromptSystemDefault 读取系统默认附加复核提示词。
export async function loadReviewPromptSystemDefault(): Promise<ReviewPromptSystemDefault> {
  return serviceClient.request<ReviewPromptSystemDefault>("service_review_prompt_system_default", {});
}

// saveReviewPromptSystemDefault replaces the system default; sessions with their own prompt keep it.
// saveReviewPromptSystemDefault 替换系统默认值；拥有自有提示词的会话保留其提示词。
export async function saveReviewPromptSystemDefault(prompt: string): Promise<ReviewPromptSystemDefault> {
  return serviceClient.request<ReviewPromptSystemDefault>("service_save_review_prompt_system_default", { request: { prompt } });
}

// loadSessionReviewPrompt reads one session's own prompt next to the system default.
// loadSessionReviewPrompt 读取一个会话的自有提示词及系统默认值。
export async function loadSessionReviewPrompt(sessionId: string): Promise<SessionReviewPrompt> {
  return serviceClient.request<SessionReviewPrompt>("service_session_review_prompt", { sessionId });
}

// saveSessionReviewPrompt saves one session's own prompt, or returns it to the system default with null.
// saveSessionReviewPrompt 保存一个会话的自有提示词；null 使其恢复跟随系统默认值。
export async function saveSessionReviewPrompt(sessionId: string, prompt: string | null): Promise<SessionReviewPrompt> {
  return serviceClient.request<SessionReviewPrompt>("service_save_session_review_prompt", { request: { sessionId, prompt } });
}
