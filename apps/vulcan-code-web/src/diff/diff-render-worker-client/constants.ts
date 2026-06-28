import type { DiffRenderableVirtualRow } from "../diff-render-html-core";
// DIFF_RENDER_WORKER_CACHE_LIMIT caps retained worker-rendered plain HTML blocks.
// DIFF_RENDER_WORKER_CACHE_LIMIT 限制保留的 Worker 纯 HTML 渲染块数量。
export const DIFF_RENDER_WORKER_CACHE_LIMIT = 512;

// DIFF_RENDER_WORKER_HIGHLIGHTED_SPAN_CACHE_LIMIT caps mirrored spans shared with the render worker.
// DIFF_RENDER_WORKER_HIGHLIGHTED_SPAN_CACHE_LIMIT 限制与渲染 Worker 同步的高亮 span 数量。
export const DIFF_RENDER_WORKER_HIGHLIGHTED_SPAN_CACHE_LIMIT = 12000;

// DISABLED_DIFF_RENDER_CACHE_KEY keeps inactive renderer hooks stable without computing real cache keys.
// DISABLED_DIFF_RENDER_CACHE_KEY 让未启用的渲染 hook 保持稳定，避免计算真实缓存键。
export const DISABLED_DIFF_RENDER_CACHE_KEY = "diff-render-disabled";

// EMPTY_DIFF_RENDERABLE_ROWS reuses one immutable empty row payload for inactive row renderers.
// EMPTY_DIFF_RENDERABLE_ROWS 为未启用的行渲染器复用一个不可变空行载荷。
export const EMPTY_DIFF_RENDERABLE_ROWS: readonly DiffRenderableVirtualRow[] = Object.freeze([]);
// DIFF_RENDER_FINGERPRINT_SEED is the FNV-1a seed used by render cache fingerprints.
// DIFF_RENDER_FINGERPRINT_SEED 是渲染缓存 fingerprint 使用的 FNV-1a 种子。
export const DIFF_RENDER_FINGERPRINT_SEED = 0x811c9dc5;

// DIFF_RENDER_CACHE_KEY_SEPARATOR separates stable render cache-key fields.
// DIFF_RENDER_CACHE_KEY_SEPARATOR 分隔稳定渲染缓存键字段。
export const DIFF_RENDER_CACHE_KEY_SEPARATOR = "\u0000";
// DIFF_HIGHLIGHTED_SPAN_COLLECTION_CACHE_LIMIT caps retained virtual-window span collections.
// DIFF_HIGHLIGHTED_SPAN_COLLECTION_CACHE_LIMIT 限制保留的虚拟窗口 span 收集结果数量。
export const DIFF_HIGHLIGHTED_SPAN_COLLECTION_CACHE_LIMIT = 512;

