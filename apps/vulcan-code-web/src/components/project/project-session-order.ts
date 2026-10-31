import type { ConversationSummary, ProjectSessionSortMode, ProjectSummary, ProjectTreePin } from "@/types";

// DEFAULT_SESSION_SORT_MODE is the service default for a project without a stored choice.
// DEFAULT_SESSION_SORT_MODE 是没有已存储选择的项目使用的服务默认值。
export const DEFAULT_SESSION_SORT_MODE: ProjectSessionSortMode = "last_request";

// SESSION_SORT_MODES lists the selectable modes with their catalog labels, in menu order.
// SESSION_SORT_MODES 按菜单顺序列出可选的排序方式及其目录标签。
export const SESSION_SORT_MODES = [
  { mode: "last_request", label: "sidebar.sessionSort.lastRequest" },
  { mode: "manual", label: "sidebar.sessionSort.manual" },
] as const satisfies ReadonlyArray<{ mode: ProjectSessionSortMode; label: string }>;

// sessionSortModeOf reads a project's sort mode; the service omits nothing, but fixtures and the browser
// preview may, and an absent mode is the documented default.
// sessionSortModeOf 读取项目的排序方式；服务端总会给出，测试数据与浏览器预览可能缺省，缺省即为约定的默认值。
export function sessionSortModeOf(project: ProjectSummary): ProjectSessionSortMode {
  return project.sessionSortMode ?? DEFAULT_SESSION_SORT_MODE;
}

// millisOf parses a Unix-millisecond text field; an unparsable value sorts as the oldest.
// millisOf 解析 Unix 毫秒文本字段；无法解析的值按最旧排序。
function millisOf(text: string | undefined): number {
  const value = Number.parseInt(text ?? "", 10);
  return Number.isFinite(value) ? value : 0;
}

// lastRequestMillis is the time last-request ordering and its recency groups use.
// lastRequestMillis 是按最近请求排序及其时间分组所用的时间。
export function lastRequestMillis(conversation: ConversationSummary): number {
  return millisOf(conversation.lastRequestAt);
}

// orderProjectSessions orders one project's sessions for display. Last-request mode puts the newest request first.
// Manual mode puts sessions the user has not arranged first, newest creation first, then the arranged ones by
// their stored position, so a new session appears at the top. Ties keep the service order.
// orderProjectSessions 为显示排列一个项目的会话。按最近请求时最近的请求在前；手动排序时用户尚未排列的会话在前（按创建时间
// 倒序），其后是已排列的会话（按存储位置），因此新会话出现在顶部。相同时保持服务顺序。
export function orderProjectSessions(
  sessions: readonly ConversationSummary[],
  mode: ProjectSessionSortMode,
): ConversationSummary[] {
  if (mode === "last_request") {
    return [...sessions].sort((left, right) => lastRequestMillis(right) - lastRequestMillis(left));
  }
  return [...sessions].sort((left, right) => {
    const leftPosition = left.manualPosition ?? null;
    const rightPosition = right.manualPosition ?? null;
    if (leftPosition === null && rightPosition === null) return millisOf(right.createdAt) - millisOf(left.createdAt);
    if (leftPosition === null) return -1;
    if (rightPosition === null) return 1;
    return leftPosition - rightPosition;
  });
}

// withSubsetOrder reorders the ids of a shown subset in place inside the complete list, leaving every other id
// where it was; a dragged subset such as a five-row preview keeps the rows it does not show.
// withSubsetOrder 在完整列表中原位重排已显示子集的标识，其他标识保持原位；被拖动的子集（如五行预览）不会丢掉未显示的行。
export function withSubsetOrder(all: readonly string[], orderedSubset: readonly string[]): string[] {
  const subset = new Set(orderedSubset);
  let next = 0;
  return all.map((id) => (subset.has(id) ? orderedSubset[next++] : id));
}

// pinsWithSessionOrder rewrites the complete pin list with some pinned sessions in a new relative order, since
// the pinned-section order request must name every shown pin exactly once.
// pinsWithSessionOrder 以新的相对顺序重写完整置顶列表中的部分置顶会话，因为置顶分区排序请求必须恰好列出每个已显示的置顶项。
export function pinsWithSessionOrder(pins: readonly ProjectTreePin[], orderedSessionIds: readonly string[]): ProjectTreePin[] {
  const ordered = new Set(orderedSessionIds);
  let next = 0;
  return pins.map((pin) => (
    pin.kind === "session" && ordered.has(pin.id) ? { kind: "session", id: orderedSessionIds[next++] } : pin
  ));
}
