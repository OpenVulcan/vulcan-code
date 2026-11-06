// Length-prefixed control framing shared by the Core browser manager and one browser host.
// Core 浏览器管理器与单个浏览器宿主共享的长度前缀控制帧。
//
// Frame layout is a 4-byte big-endian unsigned length followed by one UTF-8 JSON document.
// 帧结构为 4 字节大端无符号长度，后接一段 UTF-8 JSON 文档。

// Protocol bounds are generated from the Core definitions so both ends share one source.
// 协议边界值由 Core 定义生成，使两端共用同一来源。
import {
  BROWSER_HOST_LENGTH_PREFIX_BYTES,
  BROWSER_HOST_MAX_FRAME_BYTES,
  BROWSER_HOST_PROTOCOL_VERSION,
} from './protocol-constants.generated.js';

export { BROWSER_HOST_LENGTH_PREFIX_BYTES, BROWSER_HOST_MAX_FRAME_BYTES, BROWSER_HOST_PROTOCOL_VERSION };

/** Maximum pages one browser host retains at the same time; shared by the Electron and Chromium hosts. */
/** 单个浏览器宿主同时保留的最大页面数；Electron 与 Chromium 宿主共用。 */
export const BROWSER_HOST_MAX_PAGES = 8;

/** Maximum retained bytes for one browser download. */
/** 一份浏览器下载可保留的最大字节数。 */
export const BROWSER_HOST_MAX_DOWNLOAD_BYTES = 64 * 1024 * 1024;

/** Largest compressed image accepted before its base64 control envelope is built. */
/** 构建 base64 控制信封前允许的最大压缩图像。 */
export const BROWSER_HOST_MAX_IMAGE_BYTES = 2 * 1024 * 1024;

/** Largest accepted viewport width or height; the smallest is 1. */
/** 可接受的最大视口宽度或高度；最小值为 1。 */
export const BROWSER_HOST_MAX_VIEWPORT_DIMENSION = 4096;

/** First message written to standard output so Core can reach the host control listener. */
/** 写入标准输出的首条消息，使 Core 能够连接宿主控制监听器。 */
export interface BrowserHostEndpointFrame {
  /** Frame discriminator. */
  /** 帧判别字段。 */
  kind: 'hostControlReady';
  /** Control protocol version. */
  /** 控制协议版本。 */
  protocolVersion: number;
  /** Loopback host of the control listener. */
  /** 控制监听器的回环主机。 */
  controlHost: string;
  /** Ephemeral loopback port of the control listener. */
  /** 控制监听器的临时回环端口。 */
  controlPort: number;
  /** Operating-system process id of this host. */
  /** 本宿主的操作系统进程标识。 */
  pid: number;
}

/** First message Core sends on the control connection to prove channel ownership. */
/** Core 在控制连接上发送的首条消息，用于证明通道归属。 */
export interface BrowserHostCoreHello {
  /** Frame discriminator. */
  /** 帧判别字段。 */
  kind: 'coreHello';
  /** Control protocol version. */
  /** 控制协议版本。 */
  protocolVersion: number;
  /** One-time secret Core passed to this host at spawn time. */
  /** Core 在启动本宿主时传入的一次性密钥。 */
  connectionSecret: string;
  /** Session identity assigned by Core. */
  /** 由 Core 分配的会话标识。 */
  sessionId: string;
  /** Host generation assigned by Core. */
  /** 由 Core 分配的宿主代际。 */
  hostGeneration: string;
}

/** First message sent by the host so Core can verify its own session and generation. */
/** 宿主发送的首条消息，使 Core 能核对自身分配的会话与代际。 */
export interface BrowserHostHandshake {
  /** Control protocol version. */
  /** 控制协议版本。 */
  protocolVersion: number;
  /** Session identity assigned by Core. */
  /** 由 Core 分配的会话标识。 */
  sessionId: string;
  /** Host generation assigned by Core. */
  /** 由 Core 分配的宿主代际。 */
  hostGeneration: string;
  /** Operating-system process id of this host. */
  /** 本宿主的操作系统进程标识。 */
  pid: number;
  /** Runtime version string reported for diagnostics. */
  /** 用于诊断的运行时版本文本。 */
  runtimeVersion: string;
  /** Capability names this host actually provides. */
  /** 本宿主实际提供的能力名称。 */
  capabilities: string[];
  /** Local CDP endpoint reachable only from this machine. */
  /** 仅本机可访问的本地 CDP 端点。 */
}

/** Local debugging endpoint reported by the browser host. */
/** 浏览器宿主报告的本地调试端点。 */
export interface BrowserHostCdpEndpoint {
  /** Loopback host of the debugging listener. */
  /** 调试监听器的回环主机。 */
  host: string;
  /** Ephemeral loopback port of the debugging listener. */
  /** 调试监听器的临时回环端口。 */
  port: number;
  /** Browser-level WebSocket path from the debugging discovery file. */
  /** 来自调试发现文件的浏览器级 WebSocket 路径。 */
  browserWebSocketPath: string;
}

/** One request sent from Core to the host. */
/** Core 发送给宿主的一条请求。 */
export interface BrowserHostRequest {
  /** Correlation identity echoed by the matching response. */
  /** 由匹配响应回传的关联标识。 */
  requestId: string;
  /** Session identity that must match this host. */
  /** 必须匹配本宿主的会话标识。 */
  sessionId: string;
  /** Host generation that must match this host. */
  /** 必须匹配本宿主的宿主代际。 */
  hostGeneration: string;
  /** Operation name understood by the host. */
  /** 宿主理解的操作名称。 */
  operation: string;
  /** Operation parameters. */
  /** 操作参数。 */
  params: Record<string, unknown>;
}

/** One response sent from the host to Core. */
/** 宿主发送给 Core 的一条响应。 */
export interface BrowserHostResponse {
  /** Correlation identity copied from the request. */
  /** 从请求复制的关联标识。 */
  requestId: string;
  /** Whether the operation succeeded. */
  /** 操作是否成功。 */
  ok: boolean;
  /** Typed result when the operation succeeded. */
  /** 操作成功时的类型化结果。 */
  result?: unknown;
  /** Stable error code and message when the operation failed. */
  /** 操作失败时的稳定错误码与消息。 */
  error?: { code: string; message: string };
}

/** One unsolicited event emitted by the host. */
/** 宿主发出的一条非请求事件。 */
export interface BrowserHostEvent {
  /** Monotonic event sequence within this host lifetime. */
  /** 本宿主生命周期内单调的事件序号。 */
  eventSequence: string;
  /** Event kind understood by Core. */
  /** Core 理解的事件类型。 */
  kind: string;
  /** Typed event payload. */
  /** 类型化事件载荷。 */
  payload: unknown;
}

/** Encodes one JSON control frame with its big-endian length prefix. */
/** 使用大端长度前缀编码一个 JSON 控制帧。 */
export function encodeControlFrame(value: unknown): Buffer {
  // body carries the complete UTF-8 document without any trailing delimiter.
  // body 承载完整 UTF-8 文档，不带任何尾部分隔符。
  const body = Buffer.from(JSON.stringify(value), 'utf8');
  if (body.length > BROWSER_HOST_MAX_FRAME_BYTES) {
    throw new Error(
      `Control frame of ${body.length} bytes exceeds the ${BROWSER_HOST_MAX_FRAME_BYTES}-byte limit.`,
    );
  }
  const frame = Buffer.allocUnsafe(BROWSER_HOST_LENGTH_PREFIX_BYTES + body.length);
  frame.writeUInt32BE(body.length, 0);
  body.copy(frame, BROWSER_HOST_LENGTH_PREFIX_BYTES);
  return frame;
}

/** Incremental decoder that yields one JSON value per complete control frame. */
/** 增量解码器，每完成一个控制帧产出一个 JSON 值。 */
export class ControlFrameDecoder {
  // pending accumulates bytes that do not yet form one complete frame.
  // pending 累积尚未构成完整帧的字节。
  private pending: Buffer = Buffer.alloc(0);

  /** Appends received bytes and returns every frame completed by this chunk. */
  /** 追加接收到的字节，并返回本块完成的所有帧。 */
  push(chunk: Buffer): unknown[] {
    this.pending = this.pending.length === 0 ? chunk : Buffer.concat([this.pending, chunk]);
    const values: unknown[] = [];
    for (;;) {
      if (this.pending.length < BROWSER_HOST_LENGTH_PREFIX_BYTES) {
        return values;
      }
      const length = this.pending.readUInt32BE(0);
      if (length > BROWSER_HOST_MAX_FRAME_BYTES) {
        throw new Error(
          `Control frame of ${length} bytes exceeds the ${BROWSER_HOST_MAX_FRAME_BYTES}-byte limit.`,
        );
      }
      const end = BROWSER_HOST_LENGTH_PREFIX_BYTES + length;
      if (this.pending.length < end) {
        return values;
      }
      const body = this.pending.subarray(BROWSER_HOST_LENGTH_PREFIX_BYTES, end);
      this.pending = this.pending.subarray(end);
      values.push(JSON.parse(body.toString('utf8')));
    }
  }
}
