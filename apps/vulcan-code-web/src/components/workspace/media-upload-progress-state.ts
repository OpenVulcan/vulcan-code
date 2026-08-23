import type { MediaUploadProgressChangedDelta } from "@/types";

// MediaUploadProgressMap stores active transfers by the exact submission-and-file identity.
// MediaUploadProgressMap 按精确的提交与文件身份保存活动传输。
export type MediaUploadProgressMap = Record<string, MediaUploadProgressChangedDelta>;

// mediaUploadProgressKey derives the stable UI identity for one file without changing its cancellation identity.
// mediaUploadProgressKey 为单个文件生成稳定的界面身份，同时不改变其取消身份。
//
// Parameters: progress is the exact ordered service event for one transfer.
// 参数：progress 是单个传输对应的精确有序服务事件。
//
// Returns: a collision-free key within all files of every client submission.
// 返回值：在所有客户端提交的全部文件范围内无冲突的键。
export function mediaUploadProgressKey(progress: MediaUploadProgressChangedDelta): string {
  return JSON.stringify([progress.clientSubmissionId, progress.fileIndex]);
}

// reduceMediaUploadProgress replaces one active file or removes only that file on a terminal event.
// reduceMediaUploadProgress 替换一个活动文件，或在终态事件到达时仅移除该文件。
//
// Parameters: current is the active transfer map and progress is the next ordered service event.
// 参数：current 是活动传输映射，progress 是下一条有序服务事件。
//
// Returns: a new active transfer map that preserves sibling files from the same submission.
// 返回值：新的活动传输映射，并保留同一提交中的其他文件。
export function reduceMediaUploadProgress(
  current: MediaUploadProgressMap,
  progress: MediaUploadProgressChangedDelta,
): MediaUploadProgressMap {
  // progressKey separates sibling files while clientSubmissionId remains the backend cancellation token.
  // progressKey 区分同一提交中的文件，同时 clientSubmissionId 继续作为后端取消令牌。
  const progressKey = mediaUploadProgressKey(progress);
  const next = { ...current };
  if (
    progress.phase === "preparing"
    || progress.phase === "uploading"
    || progress.phase === "awaiting_provider_confirmation"
  ) {
    next[progressKey] = progress;
  } else {
    delete next[progressKey];
  }
  return next;
}
