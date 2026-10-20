import { formatMessage, type TranslationKey } from "@/i18n"

// NativeSessionTitled is the part of a memory source session that carries its optional host title.
// NativeSessionTitled 是记忆来源会话中携带可选宿主标题的部分。
interface NativeSessionTitled {
  displayTitle?: string | null
}

// nativeSessionDisplayName names a memory source session by its host title; a session no longer in the
// workspace (for example deleted while its memories were kept) falls back to a labelled short id.
// nativeSessionDisplayName 以宿主标题命名记忆来源会话；已不在工作区的会话（例如删除后保留了记忆）
// 退回为带说明的短标识。
// Parameters: the optional session summary, its stable id and the translator. Returns display text only.
// 参数：可选会话摘要、稳定标识与翻译函数。只返回展示文本。
export function nativeSessionDisplayName(
  session: NativeSessionTitled | undefined,
  sessionSourceId: string,
  t: (key: TranslationKey) => string,
): string {
  const title = session?.displayTitle?.trim()
  if (title) return title
  return formatMessage(t("memory.sessionList.removedSession"), { id: sessionSourceId.slice(0, 8) })
}
