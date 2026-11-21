import { useEffect, useState } from "react";
import { formatMessage, useI18n } from "@/i18n";
import { serviceClient } from "@/service/webui-service-runtime";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { RequestContextInspector } from "./request-context-inspector";
import "./request-context-dialog.css";

// One row corresponds to a field or an item in a field of the encoded provider body.
// 每行对应编码后供应商请求正文中的一个字段或字段下的条目。
export interface RequestContextNode {
  /** Wire field and array index, if applicable. */
  /** 传输字段及适用时的数组下标。 */
  field: string;
  index: number | null;
  /** Optional role and item type as sent to the provider. */
  /** 发送给供应商的可选角色与条目类型。 */
  role: string | null;
  kind: string | null;
  /** Secret-filtered preview and whether it reached its budget. */
  /** 通过凭据过滤的预览，以及它是否达到限额。 */
  content: string;
  /** Decoded bounded message text, when the provider shape has a textual body. */
  /** 供应商结构存在正文时提供已解码的有界消息文本。 */
  text?: string | null;
  /** Bounded visible reasoning prefixed to this message or invocation. */
  /** 此消息或调用之前的有界可见思考。 */
  reasoning?: string;
  /** Bounded invocations and results resolved from this node's protocol fields. */
  /** 从此节点协议字段解析得到的有界调用及返回。 */
  toolParts?: RequestContextToolPart[];
  truncated: boolean;
}

/** One tool invocation or result shown with its exact name and readable body. */
/** 一条工具调用或返回，包含精确工具名及可读内容。 */
export interface RequestContextToolPart {
  /** Name supplied by the invocation or result. */
  /** 调用或返回提供的工具名称。 */
  name: string | null;
  /** True when the payload is the model-visible result. */
  /** 若载荷是模型可见结果，则为 true。 */
  result: boolean;
  /** Bounded arguments or return payload. */
  /** 有界参数或返回载荷。 */
  content: string | null;
}

// The latest snapshot pairs transport identity with its ordered rows.
// 最近一次快照交付配对的传输身份及有序行。
export interface RequestContextSnapshot extends RequestContextRows {
  /** Stable facts frozen for the physical request. */
  /** 本次物理请求冻结的稳定事实。 */
  dispatch: {
    executionId: string;
    apiCallId: string;
    model: string;
    protocol: string;
    dispatchedAtMs: number;
    purpose: "main_generation" | "retry_generation" | "compaction" | "continue" | "subagent" | "question_execution";
  };
}

// Shared row layout for actual dispatch and simulated assembly.
// 为实际派发和模拟组装共用的行布局。
export interface RequestContextRows {
  /** Encoded node count and the bounded rows returned by Core. */
  /** 编码节点数量与 Core 返回的有限行。 */
  nodeCount: number;
  /** Local counts frozen at the request frontier, plus the last verbatim retention reason. */
  /** 请求前沿的本地计数，以及最近一次保留原文的原因。 */
  economy?: { retained: number; summarized: number; retired: number; lastRetainedReason: { turnId: number; reason: string } | null } | null;
  nodes: RequestContextNode[];
  /** Initial prompt sources matched against the displayed message bodies. */
  /** 与当前展示消息正文匹配的初始提示词来源。 */
  promptOrigins?: RequestContextPromptOrigin[];
  /** Local assembly could not resolve tool definitions for this session. */
  /** 本地组装无法解析此会话的工具定义。 */
  toolsUnavailable?: boolean;
}

/** Diagnostic identity of a producer that contributed to the initial prompt. */
/** 向初始提示词提供内容的生产者诊断标识。 */
export interface RequestContextPromptOrigin {
  /** Stable semantic classification emitted by Core. */
  /** Core 提供的稳定语义分类。 */
  kind: string;
  /** Optional producer name or source path. */
  /** 可选的生产者名称或来源路径。 */
  source: string | null;
  /** Exact outer wrapper when the whole fragment carries one. */
  /** 完整片段存在外层信封时的精确标签。 */
  tag?: string | null;
}

// A lightweight unchanged response avoids repeatedly shipping large context bodies.
// 轻量的未更新响应避免重复传输大型上下文正文。
type RequestContextResult =
  | { status: "available"; snapshot: RequestContextSnapshot; assistantResponse?: RequestContextAssistantResponse | null; economy?: RequestContextRows["economy"] }
  | { status: "unchanged" | "empty" };

// Finalized assistant text from the same physical call as the inspected request.
// 与受检请求属于同一物理调用的已完成 assistant 正文。
export interface RequestContextAssistantResponse {
  /** Bounded visible model text and its truncation status. */
  /** 有界可见模型正文及其截断状态。 */
  content: string;
  /** Visible reasoning from the same completed physical request. */
  /** 来自同一次已完成实发请求的可见思考。 */
  reasoning?: string;
  truncated: boolean;
}

// Poll only while the inspector is mounted and wait for the previous read before scheduling another.
// 仅在检查面板挂载期间轮询，等上次读取结束后才安排下一次。
const CONTEXT_REFRESH_MS = 1000;

// Purpose keys mirror the kinds already recorded by V1.
// 用途目录键对应 V1 已记录的种类。
const PURPOSE_KEY = {
  main_generation: "requestContext.generation",
  retry_generation: "requestContext.retry",
  compaction: "requestContext.compaction",
  continue: "requestContext.continue",
  subagent: "requestContext.subagent",
  question_execution: "requestContext.question",
} as const satisfies Record<RequestContextSnapshot["dispatch"]["purpose"], string>;

/** Shows the latest request for `sessionId`, assembling a preview when Core reports no dispatch. */
/** 展示 `sessionId` 的最新请求，Core 报告没有实发记录时组装预览。 */
export function RequestContextDialog({ sessionId, onClose }: { sessionId: string; onClose: () => void }) {
  const { locale, t } = useI18n();
  // Undefined is loading; null means Core observed no request for this session in its current lifetime.
  // undefined 为加载中；null 表示本次 Core 运行尚未观察到该会话的任何请求。
  const [snapshot, setSnapshot] = useState<RequestContextSnapshot | null>();
  // The matching response can arrive after the request is displayed.
  // 对应回复可在请求已展示后到达。
  const [assistantResponse, setAssistantResponse] = useState<RequestContextAssistantResponse | null>(null);
  const [failed, setFailed] = useState(false);
  // One local preview is requested per period in which Core reports an empty dispatch slot.
  // Core 每次进入无实发记录的状态时，只请求一次本机预览。
  const [simulation, setSimulation] = useState<RequestContextRows | null>();

  useEffect(() => {
    // One effect owns the timer and latest execution ID for an exact session selection.
    // 一个副作用拥有该精确会话选择的计时器及最新执行标识。
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let knownExecutionId: string | undefined;
    // Poll the same API for its response until the backend proves it is finalized.
    // 持续查询同一个 API 的回复，直到后端证实它已完成。
    let knownAssistantResponse = false;

    /** Reads and applies at most one replacement snapshot per poll. */
    /** 每次轮询至多读取并应用一份替换快照。 */
    async function readLatest(): Promise<void> {
      try {
        const result = await serviceClient.request<RequestContextResult>("service_latest_request_context_v1", {
          sessionId, knownExecutionId, knownAssistantResponse,
        });
        if (!active) return;
        setFailed(false);
        if (result.status === "available") {
          knownExecutionId = result.snapshot.dispatch.executionId;
          knownAssistantResponse = result.assistantResponse != null;
          setSnapshot({ ...result.snapshot, economy: result.economy });
          setAssistantResponse(result.assistantResponse ?? null);
          setSimulation(undefined);
        } else if (result.status === "empty") {
          knownExecutionId = undefined;
          knownAssistantResponse = false;
          setSnapshot(null);
          setAssistantResponse(null);
        }
      } catch {
        if (active) setFailed(true);
      } finally {
        if (active) timer = setTimeout(() => void readLatest(), CONTEXT_REFRESH_MS);
      }
    }

    void readLatest();
    return () => { active = false; clearTimeout(timer); };
  }, [sessionId]);

  // The actual-request poll continues while the fallback is loading or visible.
  // 模拟预览加载和显示期间，继续轮询实发请求。
  const simulated = snapshot === null;
  useEffect(() => {
    if (!simulated) return;
    // Retire the fallback when an actual request arrives or the dialog closes.
    // 实发请求到达或弹窗关闭时，结束模拟预览。
    let active = true;
    void serviceClient.request<RequestContextRows>("service_simulate_request_context_v1", { sessionId }).then(
      (rows) => { if (active) setSimulation(rows); },
      () => { if (active) setSimulation(null); },
    );
    return () => { active = false; };
  }, [sessionId, simulated]);

  // Header facts identify the exact physical attempt, even when several belong to one business turn.
  // 即使同一业务回合包含多次尝试，标题区仍标明当前精确物理尝试。
  const rows = simulated ? simulation : snapshot;
  const dispatch = snapshot?.dispatch;
  // Only an actual dispatch owns a separate finalized assistant reply.
  // 只有实发请求才持有独立的已完成助手回复。
  const response = dispatch ? assistantResponse : null;
  // An empty preview is reported in the description alongside the other states.
  // 空预览与其他状态一同在说明行显示。
  const empty = rows?.nodes.length === 0 && response === null;
  // Report read or assembly failures on the description line without hiding previously loaded rows.
  // 在说明行显示读取或组装失败，并继续展示已加载节点。
  const error = failed || (simulated && simulation === null);
  const status = failed ? t("requestContext.error") : simulated && simulation === null
    ? t("requestContext.simulationError") : rows === undefined ? t("requestContext.loading")
      : empty ? t(simulated ? "requestContext.empty" : "requestContext.noNodes") : null;

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="vk-request-context-dialog" size="fullscreen" layout="document" scrollMode="content-scroll">
        <DialogHeader>
          <DialogTitle>{t("requestContext.title")}</DialogTitle>
          <DialogDescription className="vk-request-context-meta">
            {dispatch ? (
              <>
                <strong>{dispatch.model}</strong>
                <code title={dispatch.executionId}>API #{dispatch.apiCallId}</code>
                <span>{t(PURPOSE_KEY[dispatch.purpose])}</span>
                <code>{dispatch.protocol}</code>
                <time dateTime={new Date(dispatch.dispatchedAtMs).toISOString()}>
                  {new Date(dispatch.dispatchedAtMs).toLocaleString(locale)}
                </time>
              </>
            ) : simulated ? <strong>{t("requestContext.simulate")}</strong> : null}
            {rows ? <span>{formatMessage(t("requestContext.nodeCount"), { count: rows.nodeCount })}</span> : null}
            {rows?.economy && <span>{formatMessage(t("economy.coverage"), { retained: rows.economy.retained, summarized: rows.economy.summarized, retired: rows.economy.retired })}</span>}
            {rows?.economy?.lastRetainedReason && <code>#{rows.economy.lastRetainedReason.turnId} · {rows.economy.lastRetainedReason.reason}</code>}
            {status ? <span role={error ? "alert" : "status"}>{status}</span> : null}
          </DialogDescription>
        </DialogHeader>
        <div className="vk-dialog-body vk-request-context-body">
          {rows && !empty ? (
            <RequestContextInspector key={simulated ? "simulation" : dispatch?.executionId}
              rows={rows} response={response} />
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}
