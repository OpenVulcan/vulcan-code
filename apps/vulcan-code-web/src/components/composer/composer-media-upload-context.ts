import React from "react";
import type { MediaUploadProgressChangedDelta } from "@/types";

// ComposerMediaUploadProgressMap indexes active transfer progress by the service-owned transfer identity.
// ComposerMediaUploadProgressMap 按服务持有的传输身份索引活跃传输进度。
export type ComposerMediaUploadProgressMap = Readonly<Record<string, MediaUploadProgressChangedDelta>>;

// ComposerMediaUploadProgressContext shares service progress without threading workspace-only props through the transcript tree.
// ComposerMediaUploadProgressContext 在不向转录组件树逐层传递工作区专属属性的情况下共享服务进度。
export const ComposerMediaUploadProgressContext = React.createContext<ComposerMediaUploadProgressMap>({});

// useComposerMediaUploadProgresses reads the latest active upload facts emitted by the desktop service.
// useComposerMediaUploadProgresses 读取桌面服务发出的最新活跃上传事实。
// Returns: an immutable map whose values disappear after each transfer reaches a terminal phase.
// 返回值：一个不可变映射；每条传输到达终态后会从中移除。
export function useComposerMediaUploadProgresses(): ComposerMediaUploadProgressMap {
  return React.useContext(ComposerMediaUploadProgressContext);
}
