import { createHighlighterCore, type HighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import githubDarkTheme from "shiki/themes/github-dark.mjs";
import githubLightTheme from "shiki/themes/github-light.mjs";

import {
  DIFF_SHIKI_DARK_THEME,
  DIFF_SHIKI_LIGHT_THEME,
  type DiffShikiTheme,
  type DiffShikiTokenSpan,
  normalizeShikiLanguage,
  tokensToSpans,
} from "./diff-shiki-shared";
import { loadShikiLanguageRegistration } from "./diff-shiki-worker-loaders";

// DiffHighlightWorkerLine carries one line scheduled for background highlighting.
// DiffHighlightWorkerLine 承载一条被调度到后台高亮的行。
interface DiffHighlightWorkerLine {
  // Stable cache key owned by the main thread.
  // 主线程持有的稳定缓存键。
  key: string;
  // Raw code text without the diff prefix.
  // 不包含 diff 前缀的原始代码文本。
  content: string;
}

// DiffHighlightWorkerRequest asks the worker to highlight one language batch.
// DiffHighlightWorkerRequest 请求 Worker 高亮一个语言批次。
interface DiffHighlightWorkerRequest {
  // Message kind used by the worker protocol.
  // Worker 协议使用的消息类型。
  kind: "highlight";
  // Request id used to discard stale or failed responses.
  // 用于丢弃过期或失败响应的请求标识。
  id: number;
  // Requested Shiki language id or alias.
  // 请求的 Shiki 语言标识或别名。
  language: string;
  // Requested Shiki theme id.
  // 请求的 Shiki 主题标识。
  theme: DiffShikiTheme;
  // Lines to highlight in this batch.
  // 本批次需要高亮的行。
  lines: DiffHighlightWorkerLine[];
}

// DiffHighlightWorkerCancelRequest asks the worker to stop one obsolete in-flight batch.
// DiffHighlightWorkerCancelRequest 请求 Worker 停止一个已过期的进行中批次。
interface DiffHighlightWorkerCancelRequest {
  // Message kind used by the worker protocol.
  // Worker 协议使用的消息类型。
  kind: "cancel";
  // Request id that should stop at the next cooperative slice boundary.
  // 应在下一个协作式分片边界停止的请求标识。
  id: number;
}

// DiffHighlightWorkerMessage stores every message accepted by the highlight worker.
// DiffHighlightWorkerMessage 保存高亮 Worker 接收的所有消息。
type DiffHighlightWorkerMessage = DiffHighlightWorkerRequest | DiffHighlightWorkerCancelRequest;

// DiffHighlightWorkerResult carries one highlighted line result.
// DiffHighlightWorkerResult 承载单条高亮结果。
interface DiffHighlightWorkerResult {
  // Stable cache key echoed from the request.
  // 从请求中回传的稳定缓存键。
  key: string;
  // Serializable Shiki token spans generated from the source line.
  // 从源码行生成的可序列化 Shiki token span。
  spans: DiffShikiTokenSpan[];
}

// DiffHighlightWorkerResponse returns highlighted line token spans to the main thread.
// DiffHighlightWorkerResponse 将高亮后的行 token span 返回主线程。
interface DiffHighlightWorkerResponse {
  // Request id echoed from the request.
  // 从请求中回传的请求标识。
  id: number;
  // Highlighted line results.
  // 高亮后的行结果。
  results: DiffHighlightWorkerResult[];
  // Optional error message for diagnostics and no-token rendering.
  // 用于诊断和无 token 渲染的可选错误信息。
  error?: string;
  // Whether the worker stopped this batch after a main-thread cancellation.
  // Worker 是否因主线程取消而停止了该批次。
  cancelled?: boolean;
}

// DIFF_HIGHLIGHT_WORKER_TOKENIZE_SLICE_LIMIT yields after this many Shiki line tokenizations.
// DIFF_HIGHLIGHT_WORKER_TOKENIZE_SLICE_LIMIT 每处理这么多条 Shiki 行 token 后让出一次事件循环。
const DIFF_HIGHLIGHT_WORKER_TOKENIZE_SLICE_LIMIT = 24;

// DIFF_HIGHLIGHT_WORKER_TOKEN_CACHE_LIMIT caps worker-side token spans retained after Shiki work.
// DIFF_HIGHLIGHT_WORKER_TOKEN_CACHE_LIMIT 限制 Shiki Worker 内保留的 token span 数量。
const DIFF_HIGHLIGHT_WORKER_TOKEN_CACHE_LIMIT = 8000;

// DIFF_HIGHLIGHT_WORKER_TOKEN_CACHE_LINE_LENGTH_LIMIT avoids retaining unusually large source lines.
// DIFF_HIGHLIGHT_WORKER_TOKEN_CACHE_LINE_LENGTH_LIMIT 避免缓存异常大的源码行。
const DIFF_HIGHLIGHT_WORKER_TOKEN_CACHE_LINE_LENGTH_LIMIT = 8192;

// shikiHighlighterPromise stores the worker-owned singleton highlighter initialization.
// shikiHighlighterPromise 保存 Worker 持有的单例 highlighter 初始化过程。
let shikiHighlighterPromise: Promise<HighlighterCore> | null = null;

// loadedShikiLanguages records worker languages that have already been loaded into Shiki.
// loadedShikiLanguages 记录已经加载进 Shiki 的 Worker 语言。
const loadedShikiLanguages = new Set<string>();

// pendingShikiLanguageLoads de-duplicates concurrent worker language imports.
// pendingShikiLanguageLoads 对 Worker 内并发语言导入进行去重。
const pendingShikiLanguageLoads = new Map<string, Promise<void>>();

// highlightedTokenCache keeps recent Shiki token spans inside the worker after main-thread eviction.
// highlightedTokenCache 在主线程缓存淘汰后仍于 Worker 内保留近期 Shiki token span。
const highlightedTokenCache = new Map<string, DiffShikiTokenSpan[]>();

// activeHighlightRequestIds stores highlight batches that can still observe cancellation.
// activeHighlightRequestIds 保存仍可响应取消的高亮批次。
const activeHighlightRequestIds = new Set<number>();

// cancelledHighlightRequestIds stores request ids that should stop at the next yield boundary.
// cancelledHighlightRequestIds 保存应在下一个让出边界停止的请求标识。
const cancelledHighlightRequestIds = new Set<number>();

// ensureShikiHighlighter initializes the worker-owned highlighter and theme.
// ensureShikiHighlighter 初始化 Worker 持有的 highlighter 与主题。
async function ensureShikiHighlighter(): Promise<HighlighterCore> {
  if (!shikiHighlighterPromise) {
    shikiHighlighterPromise = createHighlighterCore({
      engine: createJavaScriptRegexEngine(),
      langs: [],
      themes: [githubLightTheme, githubDarkTheme],
    });
  }
  return shikiHighlighterPromise;
}

// ensureShikiLanguage loads one Shiki language inside the worker.
// ensureShikiLanguage 在 Worker 内加载单个 Shiki 语言。
async function ensureShikiLanguage(highlighter: HighlighterCore, language: string): Promise<void> {
  if (loadedShikiLanguages.has(language)) {
    return;
  }
  const pendingLoad = pendingShikiLanguageLoads.get(language);
  if (pendingLoad) {
    return pendingLoad;
  }
  const load = loadShikiLanguage(highlighter, language);
  pendingShikiLanguageLoads.set(language, load);
  return load;
}

// loadShikiLanguage imports and registers one language, then records the successful load.
// loadShikiLanguage 导入并注册单个语言，然后记录成功加载状态。
async function loadShikiLanguage(highlighter: HighlighterCore, language: string): Promise<void> {
  try {
    await highlighter.loadLanguage(await loadShikiLanguageRegistration(language));
    loadedShikiLanguages.add(language);
  } finally {
    pendingShikiLanguageLoads.delete(language);
  }
}

// highlightLine turns one code line into serializable token spans.
// highlightLine 将单条代码行转换为可序列化 token span。
function highlightLine(highlighter: HighlighterCore, line: string, language: string, theme: DiffShikiTheme): DiffShikiTokenSpan[] {
  const cacheKey = highlightWorkerTokenCacheKey(theme, language, line);
  if (cacheKey) {
    const cached = highlightedTokenCache.get(cacheKey);
    if (cached) {
      return cached;
    }
  }
  const [tokens = []] = highlighter.codeToTokensBase(line, {
    lang: language,
    theme,
  });
  const spans: DiffShikiTokenSpan[] = tokens.length ? tokensToSpans(tokens) : [[line, ""]];
  if (cacheKey) {
    rememberHighlightedTokenSpans(cacheKey, spans);
  }
  return spans;
}

// highlightBatch handles one worker request and always returns a diagnostic-safe response.
// highlightBatch 处理单个 Worker 请求，并始终返回诊断安全的响应。
async function highlightBatch(request: DiffHighlightWorkerRequest): Promise<DiffHighlightWorkerResponse> {
  activeHighlightRequestIds.add(request.id);
  try {
    const normalizedLanguage = normalizeShikiLanguage(request.language);
    const theme = normalizeDiffShikiTheme(request.theme);
    if (!normalizedLanguage) {
      return {
        id: request.id,
        results: plainHighlightResults(request.lines),
        error: "Unsupported language",
      };
    }
    const highlighter = await ensureShikiHighlighter();
    if (isHighlightRequestCancelled(request.id)) {
      return cancelledHighlightResponse(request.id);
    }
    await ensureShikiLanguage(highlighter, normalizedLanguage);
    if (isHighlightRequestCancelled(request.id)) {
      return cancelledHighlightResponse(request.id);
    }
    const results = await highlightLinesInSlices(highlighter, request.lines, normalizedLanguage, theme, request.id);
    if (!results) {
      return cancelledHighlightResponse(request.id);
    }
    return {
      id: request.id,
      results,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return {
      id: request.id,
      results: plainHighlightResults(request.lines),
      error: message,
    };
  } finally {
    cancelledHighlightRequestIds.delete(request.id);
    activeHighlightRequestIds.delete(request.id);
  }
}

// cancelledHighlightResponse returns a tiny response for a request that no longer has observers.
// cancelledHighlightResponse 为已经没有观察者的请求返回轻量响应。
function cancelledHighlightResponse(id: number): DiffHighlightWorkerResponse {
  return {
    id,
    results: [],
    cancelled: true,
  };
}

// plainHighlightResults returns no-token spans without invoking another highlighter.
// plainHighlightResults 返回无 token span，且不会调用其他高亮器。
function plainHighlightResults(lines: readonly DiffHighlightWorkerLine[]): DiffHighlightWorkerResult[] {
  const results: DiffHighlightWorkerResult[] = new Array(lines.length);
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    results[index] = { key: line.key, spans: [[line.content, ""]] };
  }
  return results;
}

// highlightLinesInSlices tokenizes one request in small slices to keep the worker event loop responsive.
// highlightLinesInSlices 将单次请求切成小片段 token，保持 Worker 事件循环响应性。
async function highlightLinesInSlices(
  highlighter: HighlighterCore,
  lines: readonly DiffHighlightWorkerLine[],
  normalizedLanguage: string,
  theme: DiffShikiTheme,
  requestId: number,
): Promise<DiffHighlightWorkerResult[] | null> {
  const results: DiffHighlightWorkerResult[] = new Array(lines.length);
  for (let index = 0; index < lines.length; index += 1) {
    if (isHighlightRequestCancelled(requestId)) {
      return null;
    }
    const line = lines[index];
    results[index] = {
      key: line.key,
      spans: highlightLine(highlighter, line.content, normalizedLanguage, theme),
    };
    if ((index + 1) % DIFF_HIGHLIGHT_WORKER_TOKENIZE_SLICE_LIMIT === 0 && index + 1 < lines.length) {
      await yieldToHighlightWorkerEventLoop();
      if (isHighlightRequestCancelled(requestId)) {
        return null;
      }
    }
  }
  return results;
}

// isHighlightRequestCancelled returns whether the main thread has dropped this batch.
// isHighlightRequestCancelled 返回主线程是否已经丢弃该批次。
function isHighlightRequestCancelled(requestId: number): boolean {
  return cancelledHighlightRequestIds.has(requestId);
}

// rememberCancelledHighlightRequest records one cooperative cancellation request.
// rememberCancelledHighlightRequest 记录一次协作式取消请求。
function rememberCancelledHighlightRequest(requestId: number): void {
  if (!activeHighlightRequestIds.has(requestId)) {
    return;
  }
  cancelledHighlightRequestIds.add(requestId);
}

// yieldToHighlightWorkerEventLoop lets queued worker messages and browser tasks proceed between slices.
// yieldToHighlightWorkerEventLoop 让排队的 Worker 消息与浏览器任务在片段之间继续推进。
function yieldToHighlightWorkerEventLoop(): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
}

// normalizeDiffShikiTheme keeps worker requests on the registered theme set.
// normalizeDiffShikiTheme 将 Worker 请求限制在已注册主题集合内。
function normalizeDiffShikiTheme(theme: DiffShikiTheme | undefined): DiffShikiTheme {
  return theme === DIFF_SHIKI_DARK_THEME ? DIFF_SHIKI_DARK_THEME : DIFF_SHIKI_LIGHT_THEME;
}

// highlightWorkerTokenCacheKey returns a bounded cache key for ordinary source lines.
// highlightWorkerTokenCacheKey 为普通源码行返回有界缓存键。
function highlightWorkerTokenCacheKey(theme: DiffShikiTheme, language: string, line: string): string | null {
  if (line.length > DIFF_HIGHLIGHT_WORKER_TOKEN_CACHE_LINE_LENGTH_LIMIT) {
    return null;
  }
  return `${theme}\u0000${language}\u0000${line}`;
}

// rememberHighlightedTokenSpans stores one worker-side token span entry with FIFO eviction.
// rememberHighlightedTokenSpans 以 FIFO 淘汰策略保存一条 Worker 侧 token span。
function rememberHighlightedTokenSpans(cacheKey: string, spans: DiffShikiTokenSpan[]): void {
  if (highlightedTokenCache.has(cacheKey)) {
    highlightedTokenCache.delete(cacheKey);
  } else if (highlightedTokenCache.size >= DIFF_HIGHLIGHT_WORKER_TOKEN_CACHE_LIMIT) {
    const oldestKey = highlightedTokenCache.keys().next().value;
    if (oldestKey) {
      highlightedTokenCache.delete(oldestKey);
    }
  }
  highlightedTokenCache.set(cacheKey, spans);
}

self.onmessage = (event: MessageEvent<DiffHighlightWorkerMessage>) => {
  if (event.data.kind === "cancel") {
    rememberCancelledHighlightRequest(event.data.id);
    return;
  }
  void highlightBatch(event.data).then((response) => {
    self.postMessage(response);
  });
};

export {};
