import type { DiffFile, DiffHunkExpansionRequest } from "../types";

// DiffFileDetailLoader lazily requests one file-level diff detail from the host.
// DiffFileDetailLoader 从宿主懒请求单个文件级 diff 详情。
export type DiffFileDetailLoader = (file: DiffFile, contextLines?: number, expansion?: DiffHunkExpansionRequest) => void;
