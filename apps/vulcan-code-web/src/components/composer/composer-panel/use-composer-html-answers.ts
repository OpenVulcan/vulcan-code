import React from "react";
import { toast } from "sonner";
import { useI18n } from "@/i18n";
import { isHtmlAnswerModeNewer, type HtmlAnswerModeOwner } from "@/service/html-answer-mode";
import { setSessionHtmlAnswerMode } from "@/service/html-answers-service";
import { useComposerDraftSessionSettings } from "./use-composer-draft-session-settings";

/** Bind the draft settings capture and persisted HTML preference to one composer owner.
 * 将草稿设置捕获与持久 HTML 偏好绑定到同一输入区归属。
 * Returns both controllers so first submission and visible toggle share one draft authority.
 * 返回两个控制器，使首次提交与可见开关共用一个草稿权威。 */
export function useComposerSessionSettings(
  scopeKey: string, sessionId: string | null, authority: HtmlAnswerModeOwner | null,
) {
  const draftSessionSettings = useComposerDraftSessionSettings(scopeKey, sessionId);
  const htmlAnswers = useComposerHtmlAnswers({
    scopeKey, sessionId, authority,
    draftEnabled: draftSessionSettings.htmlAnswersEnabled,
    setDraftEnabled: draftSessionSettings.setHtmlAnswersEnabled,
  });
  return { draftSessionSettings, htmlAnswers };
}

/** Inputs bind settings and asynchronous receipts to the exact visible draft or session.
 * 输入把设置及异步回执绑定到精确可见草稿或会话。 */
interface HtmlAnswerComposerOptions {
  /** Stable draft slot, including unmaterialized conversations.
   * 稳定草稿槽，包含尚未物化的会话。 */
  scopeKey: string;
  /** Existing session, or null for a new draft.
   * 已有会话；新草稿为空。 */
  sessionId: string | null;
  /** Core preference for the current hydrated generation, or null while loading.
   * 当前已加载代次的 Core 偏好；加载期间为空。 */
  authority: HtmlAnswerModeOwner | null;
  /** Draft preference owned by the atomic first-submission settings capture.
   * 原子首次提交设置捕获持有的草稿偏好。 */
  draftEnabled: boolean;
  /** Synchronously update only the captured draft slot.
   * 同步更新捕获的草稿槽。 */
  setDraftEnabled: (enabled: boolean) => void;
}

/** Own a compare-and-set HTML toggle; returns rendered state and a send-time persistence barrier.
 * 管理比较交换的 HTML 开关；返回显示状态和发送时的持久化屏障。
 * `options` carries authoritative ownership; late or failed writes cannot alter another scope.
 * `options` 携带权威归属；迟到或失败的写入不能修改其他作用域。 */
export function useComposerHtmlAnswers(options: HtmlAnswerComposerOptions) {
  // t resolves all feedback from the shared locale catalog.
  // t 从共享语言目录解析全部反馈。
  const { t } = useI18n();
  // ownerKey invalidates old receipts on selection changes and history rewrites.
  // ownerKey 在选择变化及历史重写时使旧回执失效。
  const ownerKey = `${options.scopeKey}:${options.authority?.generation ?? "loading"}`;
  const activeOwner = React.useRef({ key: ownerKey });
  if (activeOwner.current.key !== ownerKey) activeOwner.current = { key: ownerKey };
  const owner = activeOwner.current;
  const mounted = React.useRef(true);
  React.useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  // accepted holds only successful Core receipts until the shared projection catches up.
  // accepted 仅保留成功 Core 回执，直到共享投影追上。
  const [accepted, setAccepted] = React.useState<{ key: string; owner: HtmlAnswerModeOwner } | null>(null);
  const [savingOwner, setSavingOwner] = React.useState<typeof owner | null>(null);
  // pending synchronously deduplicates clicks and allows submit to await this exact write.
  // pending 同步去重点击，并允许提交等待这次精确写入。
  const pending = React.useRef<{ owner: typeof owner; promise: Promise<boolean> } | null>(null);
  const authority = options.authority?.sessionId === options.sessionId ? options.authority : null;
  const mode = accepted?.key === ownerKey && authority !== null
    && isHtmlAnswerModeNewer(accepted.owner.mode, authority.mode) ? accepted.owner.mode : authority?.mode;
  const enabled = options.sessionId === null ? options.draftEnabled : mode?.enabled === true;
  const saving = savingOwner === owner;
  const awaitingRequest = options.sessionId !== null && mode !== undefined
    && (mode.prepared === null ? mode.enabled : mode.prepared.revision !== mode.revision);

  // toggle changes drafts synchronously or saves one revision-checked existing-session update.
  // toggle 同步修改草稿，或保存一次校验修订的已有会话更新。
  const toggle = React.useCallback(() => {
    if (options.sessionId === null) {
      options.setDraftEnabled(!options.draftEnabled);
      return;
    }
    if (authority === null || mode === undefined || pending.current?.owner === owner) return;
    const sessionId = options.sessionId;
    const generation = authority.generation;
    setSavingOwner(owner);
    const promise = setSessionHtmlAnswerMode(sessionId, !mode.enabled, mode.revision).then((response) => {
      if (!mounted.current || activeOwner.current !== owner) return false;
      if (response.session_id !== sessionId || response.window_generation !== generation) {
        throw new Error("HTML preference response owner changed.");
      }
      setAccepted({ key: ownerKey, owner: { sessionId, generation, mode: response.html_answers } });
      if (response.conflict) toast.info(t("htmlAnswers.conflict"));
      return !response.conflict;
    }).catch(() => {
      if (mounted.current && activeOwner.current === owner) toast.error(t("htmlAnswers.saveFailed"));
      return false;
    }).finally(() => {
      if (pending.current?.promise === promise) pending.current = null;
      if (mounted.current && activeOwner.current === owner) setSavingOwner(null);
    });
    pending.current = { owner, promise };
  }, [authority, mode, options, owner, ownerKey, t]);

  // flush waits only for this owner; it never transfers an old session's receipt to a new draft.
  // flush 只等待当前归属；绝不把旧会话回执转给新草稿。
  const flush = React.useCallback(async () => {
    const write = pending.current;
    return write?.owner === owner ? await write.promise : true;
  }, [owner]);
  return { enabled, saving, awaitingRequest, available: options.sessionId === null || mode !== undefined, toggle, flush };
}
