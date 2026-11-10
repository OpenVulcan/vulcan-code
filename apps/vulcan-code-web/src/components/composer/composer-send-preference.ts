// ComposerSendMode selects the delivery action while a conversation is running.
// ComposerSendMode 选择会话运行时的消息发送操作。
export type ComposerSendMode = "queue" | "insert";

// STORAGE_KEY stores the explicit Enter action for all conversations in this client.
// STORAGE_KEY 保存此客户端所有会话共用的显式 Enter 操作。
const STORAGE_KEY = "vulcan.composer.enter-send-mode.v1";

// CHANGE_EVENT notifies the document that wrote the preference, which does not receive its own storage event.
// CHANGE_EVENT 通知写入偏好的当前文档，因为它不会收到自身写入的 storage 事件。
const CHANGE_EVENT = "vulcan:composer-enter-send-mode-changed";

// readComposerEnterSendMode takes no arguments and returns the saved action or the queue default.
// readComposerEnterSendMode 无参数，返回已保存的操作或默认排队。
export function readComposerEnterSendMode(): ComposerSendMode {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "insert" ? "insert" : "queue";
  } catch {
    return "queue";
  }
}

// saveComposerEnterSendMode writes the explicitly selected mode and returns after storage succeeds.
// saveComposerEnterSendMode 写入用户显式选择的 mode，在存储成功后返回。
export function saveComposerEnterSendMode(mode: ComposerSendMode): void {
  window.localStorage.setItem(STORAGE_KEY, mode);
  window.dispatchEvent(new Event(CHANGE_EVENT));
}

// Subscribe to successful writes in this document and storage changes from other documents. Return the cleanup.
// 订阅当前文档的成功写入与其他文档的存储变更，返回取消订阅函数。
export function subscribeComposerEnterSendMode(onChange: () => void): () => void {
  // Notify only for this preference or when another document clears storage.
  // 仅在此偏好变化或其他文档清空存储时通知。
  const onStorage = (event: StorageEvent): void => {
    if (event.key === STORAGE_KEY || event.key === null) onChange();
  };
  window.addEventListener(CHANGE_EVENT, onChange);
  window.addEventListener("storage", onStorage);
  return () => {
    window.removeEventListener(CHANGE_EVENT, onChange);
    window.removeEventListener("storage", onStorage);
  };
}

// Return the Ctrl+Enter action for the given Enter mode.
// 返回给定 Enter 模式对应的 Ctrl+Enter 操作。
export function oppositeComposerSendMode(mode: ComposerSendMode): ComposerSendMode {
  return mode === "queue" ? "insert" : "queue";
}
