import React from "react";
import { formatMessage, translateActive } from "@/i18n";
import { SESSION_TITLE_GENERATED_TOPIC, type SessionTitleGeneratedEvent } from "@/service/session-title-events";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import { toast } from "@/ui-kit/primitives/toast";

// showSessionTitleGeneratedToast tells the user the new title, or which model failed to write one.
// showSessionTitleGeneratedToast 告知用户新标题，或哪个模型未能生成标题。
// Parameters: event is the outcome of one title request.
// 参数：event 是一次标题请求的结果。
export function showSessionTitleGeneratedToast(event: SessionTitleGeneratedEvent): void {
  if (event.outcome.kind === "renamed") {
    toast.success(formatMessage(translateActive("sessionTitle.toast.renamed"), { title: event.outcome.title }));
    return;
  }
  toast.warning(formatMessage(translateActive("sessionTitle.toast.failed"), { model: event.model }));
}

// useSessionTitleToasts shows a toast for every generated session-title outcome.
// useSessionTitleToasts 为每个会话标题生成结果显示提示。
// Parameters: serviceInstanceEpoch resets the subscription after a reconnect.
// 参数：serviceInstanceEpoch 在重连后重置订阅。
export function useSessionTitleToasts(serviceInstanceEpoch: number): void {
  React.useEffect(() => {
    if (!hasServiceRuntime()) return undefined;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    void serviceClient.subscribe<SessionTitleGeneratedEvent>(SESSION_TITLE_GENERATED_TOPIC, (event) => {
      if (!cancelled) showSessionTitleGeneratedToast(event);
    }).then((nextUnsubscribe) => {
      if (cancelled) nextUnsubscribe();
      else unsubscribe = nextUnsubscribe;
    }).catch(() => undefined);
    return () => {
      cancelled = true;
      unsubscribe?.();
      unsubscribe = null;
    };
  }, [serviceInstanceEpoch]);
}
