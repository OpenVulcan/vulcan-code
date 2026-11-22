import type { RequestContextAssistantResponse, RequestContextNode, RequestContextRows, RequestContextSnapshot } from "./request-context-dialog";

/** One displayed node with a session-local React key independent of its current array position. */
/** 单个展示节点，其会话内 React 键独立于当前数组位置。 */
export interface RequestContextEntry extends RequestContextNode {
  /** Monotonic key for the lifetime of the open inspector. */
  /** 检查面板打开期间分配的单调递增键。 */
  readonly entryId: number;
}

/** Latest displayed request and the identity counter shared by successive polls of one session. */
/** 最近展示的请求及其在同一会话的历次轮询间共用的身份计数。 */
export interface RequestContextView extends Omit<RequestContextRows, "nodes"> {
  /** Visible entries in wire order, ending with the finalized response if available. */
  /** 按传输顺序排列的可见条目，已有最终回复时将其放在末尾。 */
  nodes: RequestContextEntry[];
  /** Physical request identity when served from a dispatch. */
  /** 数据来自实发请求时的物理请求标识。 */
  dispatch?: RequestContextSnapshot["dispatch"];
  /** Next unused key, including after earlier entries disappear. */
  /** 下一个尚未分配的键，即使早期条目消失也不复用。 */
  nextId: number;
  /** Count omitted by Core before the optional assistant response is appended. */
  /** 在添加可选助手回复前 Core 省略的节点数。 */
  omitted: number;
}

/** Compares every diagnostic field before reusing a row object and its memoized render. */
/** 在复用行对象及其渲染结果前比较全部诊断字段。 */
function unchanged(old: RequestContextEntry, node: RequestContextNode): boolean {
  return old.field === node.field && old.index === node.index && old.content === node.content
    && old.text === node.text && old.role === node.role && old.kind === node.kind
    && old.reasoning === node.reasoning && old.truncated === node.truncated
    && old.toolParts?.length === node.toolParts?.length && (node.toolParts ?? []).every((part, index) => {
      const prior = old.toolParts?.[index];
      return prior !== undefined && part.name === prior.name && part.result === prior.result && part.content === prior.content;
    });
}

/** Reuses unchanged entries and assigns new keys only to newly visible nodes in the supplied request. */
/** 复用未变化的条目，仅为给定请求中新出现的节点分配键。 */
export function reconcileRequestContext(
  previous: RequestContextView | null | undefined, rows: RequestContextRows,
  dispatch?: RequestContextSnapshot["dispatch"], response?: RequestContextAssistantResponse | null,
): RequestContextView {
  // Appends share most of their prefix. Reuse it without indexing full message and schema bodies.
  // 追加通常共享大部分前缀，直接复用，无需为这些消息和 Schema 正文构建索引。
  const nodes: RequestContextEntry[] = [];
  if (previous) {
    for (const node of rows.nodes) {
      const old = previous.nodes[nodes.length];
      if (!old || !unchanged(old, node)) break;
      nodes.push(old);
    }
  }
  // Consume duplicate messages in wire order; keep large raw strings out of React keys.
  // 依照传输顺序匹配重复消息，避免将大型原始字符串用作 React 键。
  const prior = new Map<string, RequestContextEntry[]>();
  let previousResponse: RequestContextEntry | undefined;
  const prefixLength = nodes.length;
  for (let i = (previous?.nodes.length ?? 0) - 1; previous && i >= prefixLength; i--) {
    const entry = previous.nodes[i];
    if (entry.field === "response") {
      previousResponse = entry;
      continue;
    }
    const identity = entry.field + "\0" + entry.content;
    const matches = prior.get(identity);
    if (matches) matches.push(entry);
    else prior.set(identity, [entry]);
  }
  let nextId = previous?.nextId ?? 0;
  // Include the finalized response in the same list so its detail stays mounted when it becomes history.
  // 将已完成答复纳入同一列表，使其进入历史后仍保留已挂载的详情。
  const incoming: RequestContextNode[] = response ? [...rows.nodes, {
    field: "response", index: null, role: "assistant", kind: null, content: response.content,
    text: response.content, reasoning: response.reasoning, truncated: response.truncated,
  }] : rows.nodes;
  const sameExecution = previous?.dispatch?.executionId === dispatch?.executionId;
  for (let i = prefixLength; i < incoming.length; i++) {
    const node = incoming[i];
    let old = prior.get(node.field + "\0" + node.content)?.pop();
    // A response belongs to its execution until it reappears as a new assistant history message.
    // 答复归属其物理请求，直到以新增助手历史消息的形式重新出现。
    if (!old && previousResponse && (node.field === "response"
      ? sameExecution
      : !sameExecution && (node.role === "assistant" || node.role === "model" || node.kind === "assistant_text")
        && !node.toolParts?.length && node.text === previousResponse.text)) {
      old = previousResponse;
      previousResponse = undefined;
    }
    nodes.push(old && unchanged(old, node) ? old : { ...node, entryId: old?.entryId ?? nextId++ });
  }
  return { ...rows, nodes, dispatch, nextId, omitted: rows.nodeCount - rows.nodes.length };
}
