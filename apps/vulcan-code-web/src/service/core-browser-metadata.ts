import {
  listCoreBrowserSessions, listCoreBrowserPages, listCoreBrowserDownloads,
  listCoreBrowserDialogs, listCoreBrowserFileChoosers,
  type CoreBrowserSession, type CoreBrowserPage, type CoreBrowserDownload,
  type CoreBrowserDialog, type CoreBrowserFileChooser,
} from "./core-browser-service";
import { createSharedPollingResource, type PollingObserver, type SharedPollingResource } from "./shared-polling-resource";

// Intervals are defined once for all observers, including the workspace shell.
// 所有观察者（包括工作区外壳）共用唯一定义的轮询间隔。
const BROWSER_LIST_REFRESH_MS = 1_500;
const BROWSER_MODAL_REFRESH_MS = 500;
const resources = new Map<string, SharedPollingResource<unknown>>();

// Metadata is JSON-shaped and acyclic. Retain equal snapshots instead of rerendering each poll.
// 元数据是无环 JSON；保留等价快照，避免每轮查询都触发重绘。
function equalMetadata(previous: unknown, next: unknown): boolean {
  if (Object.is(previous, next)) return true;
  if (previous === null || next === null || typeof previous !== "object" || typeof next !== "object") return false;
  if (Array.isArray(previous) !== Array.isArray(next)) return false;
  const before = previous as Record<string, unknown>;
  const after = next as Record<string, unknown>;
  const keys = Object.keys(before);
  return keys.length === Object.keys(after).length
    && keys.every(key => Object.hasOwn(after, key) && equalMetadata(before[key], after[key]));
}

function resource<T>(kind: string, owner: string, session: string | null, load: () => Promise<T>, interval: number): SharedPollingResource<T> {
  // JSON tuples prevent ambiguous joins and keep conversation/browser identities distinct.
  // JSON 元组避免拼接歧义，并隔离会话及浏览器身份。
  const key = JSON.stringify([kind, owner, session]);
  const existing = resources.get(key);
  if (existing) return existing as SharedPollingResource<T>;
  const created = createSharedPollingResource(load, interval, equalMetadata, () => {
    if (resources.get(key) === created) resources.delete(key);
  });
  resources.set(key, created as SharedPollingResource<unknown>);
  return created;
}

function sessions(owner: string) {
  return resource("sessions", owner, null, async () => {
    const value = await listCoreBrowserSessions(owner);
    if (value.some(session => session.ownerSessionId !== owner)) throw new Error("Core browser list contains a different conversation owner.");
    return value;
  }, BROWSER_LIST_REFRESH_MS);
}
function pages(owner: string, session: string) {
  return resource("pages", owner, session, () => listCoreBrowserPages(owner, session), BROWSER_LIST_REFRESH_MS);
}
export const observeCoreBrowserSessions = (owner: string, observer: PollingObserver<CoreBrowserSession[]>) => sessions(owner).subscribe(observer);
export const observeCoreBrowserPages = (owner: string, session: string, observer: PollingObserver<CoreBrowserPage[]>) => pages(owner, session).subscribe(observer);
export const refreshCoreBrowserSessions = (owner: string) => sessions(owner).refresh();
export const refreshCoreBrowserPages = (owner: string, session: string) => pages(owner, session).refresh();
export const observeCoreBrowserDownloads = (owner: string, session: string, observer: PollingObserver<CoreBrowserDownload[]>) =>
  resource("downloads", owner, session, () => listCoreBrowserDownloads(owner, session), BROWSER_LIST_REFRESH_MS).subscribe(observer);
export const observeCoreBrowserDialogs = (owner: string, session: string, observer: PollingObserver<CoreBrowserDialog[]>) =>
  resource("dialogs", owner, session, () => listCoreBrowserDialogs(owner, session), BROWSER_MODAL_REFRESH_MS).subscribe(observer);
export const observeCoreBrowserFileChoosers = (owner: string, session: string, observer: PollingObserver<CoreBrowserFileChooser[]>) =>
  resource("fileChoosers", owner, session, () => listCoreBrowserFileChoosers(owner, session), BROWSER_MODAL_REFRESH_MS).subscribe(observer);
