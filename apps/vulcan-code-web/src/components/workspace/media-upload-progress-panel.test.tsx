// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { MediaUploadProgressChangedDelta } from "@/types";
import { MediaUploadProgressPanel } from "./media-upload-progress-panel";

// progress is a complete synthetic upload event with deliberately unknown total size.
// progress 是总大小特意设为未知的完整合成上传事件。
const progress: MediaUploadProgressChangedDelta = {
  clientSubmissionId: "submission-1", conversationId: "conversation-1", providerId: "provider-1",
  serviceEntryId: "service-1", connectionId: "connection-1", modelId: "model-1", protocolProfileId: "profile-1",
  manifestRevision: 1, manifestGeneration: 1, routeRevision: "route-1", operationId: "conversation.media_input",
  uploadChannelId: "upload-1", fileName: "sample.mp4", mediaKind: "video", fileIndex: 0, fileCount: 1,
  phase: "uploading", bytesSent: 50, totalBytes: null, cancellable: true,
};
// host and root own one isolated transfer panel without sending actual file contents.
// host 和 root 持有一个隔离传输面板，不发送实际文件内容。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => { host = document.createElement("div"); document.body.append(host); root = createRoot(host); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("preserves unknown progress and allows retry after a rejected cancellation without duplicate requests", async () => {
  // rejectCancel holds the accepted request while rapid clicks exercise the synchronous guard.
  // rejectCancel 挂起已接受请求，让快速点击检验同步守卫。
  let rejectCancel!: (error: Error) => void;
  const onCancel = vi.fn().mockImplementationOnce(() => new Promise((_, reject) => { rejectCancel = reject; })).mockResolvedValue(true);
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><MediaUploadProgressPanel progress={progress} onCancel={onCancel} /></I18nProvider>));
  expect(host.querySelector("progress")?.hasAttribute("value")).toBe(false);
  expect(host.textContent).not.toContain("0%");
  // cancel is the actual shared button rendered by the component.
  // cancel 是组件渲染的真实共享按钮。
  const cancel = host.querySelector("button")!;
  await act(async () => { cancel.click(); cancel.click(); });
  expect(onCancel).toHaveBeenCalledExactlyOnceWith("submission-1");
  await act(async () => rejectCancel(new Error("Synthetic cancellation rejection")));
  expect(cancel.disabled).toBe(false);
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("未能取消上传");
  await act(async () => cancel.click());
  expect(onCancel).toHaveBeenCalledTimes(2);
  expect(cancel.disabled).toBe(true);
  expect(host.querySelector('[role="alert"]')).toBeNull();
});
