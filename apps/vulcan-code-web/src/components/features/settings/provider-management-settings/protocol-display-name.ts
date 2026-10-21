// User-visible names for provider protocol profiles.
// 供应商协议 Profile 的用户可见名称。


// protocolDisplayName returns the user-visible label for one exact profile.
// protocolDisplayName 返回一个精确 Profile 的用户可见标签。
export function protocolDisplayName(profileId: string): string {
  return protocolDisplayNames[profileId] ?? profileId
}

// protocolDisplayNames is the complete user-visible label map for registered first-batch profiles.
// protocolDisplayNames 是首批已注册 Profile 的完整用户可见名称映射。
const protocolDisplayNames: Readonly<Record<string, string>> = {
  "anthropic-messages.v1": "Anthropic Messages",
  "gemini-interactions.v1": "Gemini Interactions",
  "google.code_assist.generate_content.v1": "Google Code Assist",
  "google.gemini.generate_content": "Gemini Generate Content",
  "openai-chat-completions.v1": "OpenAI Chat Completions",
  "openai-codex-responses.v1": "OpenAI Codex",
  "openai-responses.v1": "OpenAI Responses",
}
