import { describe, expect, it } from "vitest";
import type { MediaUploadProgressChangedDelta } from "@/types";
import {
  mediaUploadProgressKey,
  reduceMediaUploadProgress,
} from "@/components/workspace/media-upload-progress-state";

// mediaProgress creates one exact upload progress event for reducer regression tests.
// mediaProgress 为归约器回归测试创建一条精确的上传进度事件。
//
// Parameters: fileIndex selects the sibling file and phase selects its lifecycle state.
// 参数：fileIndex 选择同一提交中的文件，phase 选择其生命周期状态。
//
// Returns: one complete browser-side service contract fixture.
// 返回值：一份完整的浏览器侧服务契约夹具。
function mediaProgress(
  fileIndex: number,
  phase: MediaUploadProgressChangedDelta["phase"],
): MediaUploadProgressChangedDelta {
  return {
    clientSubmissionId: "submission-1",
    conversationId: "conversation-1",
    providerId: "provider-1",
    serviceEntryId: "service-1",
    connectionId: "connection-1",
    modelId: "model-1",
    protocolProfileId: "protocol-1",
    manifestRevision: 1,
    manifestGeneration: 2,
    routeRevision: "route-1",
    operationId: "conversation_media_input",
    uploadChannelId: "upload-1",
    fileName: `video-${fileIndex}.mp4`,
    mediaKind: "video",
    fileIndex,
    fileCount: 2,
    phase,
    bytesSent: 50,
    totalBytes: 100,
    cancellable: true,
  };
}

describe("media upload progress state", () => {
  it("keeps sibling files from one submission as independent rows", () => {
    const first = mediaProgress(0, "uploading");
    const second = mediaProgress(1, "preparing");
    const state = reduceMediaUploadProgress(reduceMediaUploadProgress({}, first), second);

    expect(Object.keys(state)).toEqual([
      mediaUploadProgressKey(first),
      mediaUploadProgressKey(second),
    ]);
    expect(Object.values(state).map((progress) => progress.fileName)).toEqual([
      "video-0.mp4",
      "video-1.mp4",
    ]);
  });

  it("removes only the terminal sibling and preserves the active transfer", () => {
    const first = mediaProgress(0, "uploading");
    const second = mediaProgress(1, "uploading");
    const active = reduceMediaUploadProgress(reduceMediaUploadProgress({}, first), second);
    const completedFirst = mediaProgress(0, "completed");
    const next = reduceMediaUploadProgress(active, completedFirst);

    expect(next[mediaUploadProgressKey(first)]).toBeUndefined();
    expect(next[mediaUploadProgressKey(second)]).toEqual(second);
  });
});
