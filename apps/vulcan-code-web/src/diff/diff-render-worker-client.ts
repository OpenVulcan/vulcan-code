export type {
  DiffHighlightedFileWorkerOptions,
  DiffHighlightedRowsWorkerOptions,
  DiffPlainRowsWorkerOptions,
} from "./diff-render-worker-client/types";
export {
  useHighlightedDiffFileHtml,
  useHighlightedDiffRowsHtml,
  usePlainDiffFileHtml,
  usePlainDiffRowsHtml,
} from "./diff-render-worker-client/hooks";
