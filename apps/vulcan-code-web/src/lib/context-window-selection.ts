import type {
  ContextWindowSelection,
  PublicAiContextCompactionWindowNode,
} from "@/types";

// CustomContextWindowInputError names why one typed custom window cannot be used.
// CustomContextWindowInputError 说明一个输入的自定义窗口不能使用的原因。
export type CustomContextWindowInputError = "format" | "belowMinimum" | "aboveModel";

// parseContextWindowTokens reads a plain integer or an integer followed by K (1K = 1000).
// parseContextWindowTokens 读取纯整数或后接 K 的整数（1K = 1000）。
// Parameters: text is the raw user input; returns the token count, or null when the shape is not accepted.
// 参数：text 是原始输入；返回 Token 数，形状不被接受时返回 null。
export function parseContextWindowTokens(text: string): number | null {
  const match = /^\s*(\d+)\s*([kK]?)\s*$/.exec(text);
  if (!match) return null;
  const tokens = Number(match[1]) * (match[2] ? 1_000 : 1);
  return Number.isSafeInteger(tokens) && tokens > 0 ? tokens : null;
}

// validateCustomContextWindowInput applies the backend-projected minimum and the model's own window.
// validateCustomContextWindowInput 应用后端投影的下限与模型自身窗口。
// Parameters: text is the raw input, minTokens the projected minimum, modelContextLimit the declared model window;
// returns the accepted tokens or the reason they are rejected.
// 参数：text 为原始输入，minTokens 为投影下限，modelContextLimit 为模型声明窗口；返回接受的 Token 数或拒绝原因。
export function validateCustomContextWindowInput(
  text: string,
  minTokens: number,
  modelContextLimit: number,
): { tokens: number } | { error: CustomContextWindowInputError } {
  const tokens = parseContextWindowTokens(text);
  if (tokens === null) return { error: "format" };
  if (tokens < minTokens) return { error: "belowMinimum" };
  if (!contextWindowFitsModel(tokens, modelContextLimit)) return { error: "aboveModel" };
  return { tokens };
}

// contextWindowFitsModel mirrors the backend fit rule: a window may equal the model window but never exceed it.
// contextWindowFitsModel 镜像后端适配规则：窗口可以等于模型窗口，但不得超过。
export function contextWindowFitsModel(tokens: number, modelContextLimit: number): boolean {
  return tokens <= modelContextLimit;
}

// offeredContextWindowNodes lists the fixed nodes a picker offers for one model window. A node equal to the
// window duplicates "follow the model" and is hidden, unless it is the current selection.
// offeredContextWindowNodes 列出选择器针对某模型窗口提供的固定节点。等于模型窗口的节点与“跟随模型”重复而隐藏，
// 当前已选中时除外。
export function offeredContextWindowNodes<Node extends { id: string; tokens: number }>(
  nodes: Node[],
  modelContextLimit: number | null,
  selectedNodeId: string | null,
): Node[] {
  if (modelContextLimit == null) return [];
  return nodes.filter((node) => node.id === selectedNodeId
    ? contextWindowFitsModel(node.tokens, modelContextLimit)
    : node.tokens < modelContextLimit);
}

// contextWindowSelectionTokens resolves a selection against the current backend node catalog.
// contextWindowSelectionTokens 根据当前后端节点目录解析一个选择。
// Returns null for no selection or a node id the catalog no longer declares.
// 无选择或目录已不再声明该节点时返回 null。
export function contextWindowSelectionTokens(
  selection: ContextWindowSelection | null | undefined,
  nodes: PublicAiContextCompactionWindowNode[],
): number | null {
  if (!selection) return null;
  if ("custom" in selection) return selection.custom;
  return nodes.find((node) => node.id === selection.node)?.tokens ?? null;
}

// sameContextWindowSelection compares two selections by value.
// sameContextWindowSelection 按值比较两个选择。
export function sameContextWindowSelection(
  left: ContextWindowSelection | null | undefined,
  right: ContextWindowSelection | null | undefined,
): boolean {
  if (!left || !right) return !left && !right;
  if ("custom" in left) return "custom" in right && left.custom === right.custom;
  return "node" in right && left.node === right.node;
}

// formatContextWindowTokens renders a window as M or K when exact, otherwise rounds to whole K.
// formatContextWindowTokens 在整除时把窗口渲染为 M 或 K，否则四舍五入到整 K。
export function formatContextWindowTokens(tokens: number): string {
  if (tokens >= 1_000_000 && tokens % 1_000_000 === 0) {
    return `${tokens / 1_000_000}M`;
  }
  return `${Math.round(tokens / 1_000)}K`;
}

// formatCustomContextWindowInput renders a custom value back into the accepted input shape without rounding.
// formatCustomContextWindowInput 将自定义值还原为可接受的输入形状，不做舍入。
export function formatCustomContextWindowInput(tokens: number): string {
  return tokens % 1_000 === 0 ? `${tokens / 1_000}K` : String(tokens);
}

// formatContextWindowSelectionLabel shows custom values exactly and node values in their compact form.
// formatContextWindowSelectionLabel 精确显示自定义值，节点值使用紧凑形式。
export function formatContextWindowSelectionLabel(selection: ContextWindowSelection, tokens: number): string {
  return "custom" in selection ? formatCustomContextWindowInput(selection.custom) : formatContextWindowTokens(tokens);
}
