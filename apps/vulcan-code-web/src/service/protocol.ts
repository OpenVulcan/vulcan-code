// ServiceParams carries transport-neutral request parameters.
// ServiceParams 承载与传输实现无关的请求参数。
export type ServiceParams = Record<string, unknown>;

// ServiceRequest describes one logical RPC request sent to the Core service.
// ServiceRequest 描述发送给 Core 服务的一次逻辑 RPC 请求。
export interface ServiceRequest {
  // id is the client-generated request identifier.
  // id 是客户端生成的请求标识。
  id: string;
  // method is the protocol method name, for example startup.snapshot.
  // method 是协议方法名，例如 startup.snapshot。
  method: string;
  // params contains optional method arguments.
  // params 包含可选的方法参数。
  params?: ServiceParams;
}

// ServiceRequestOptions carries optional transport controls for one request.
// ServiceRequestOptions 承载单次请求的可选传输控制项。
export interface ServiceRequestOptions {
  // signal aborts the underlying request when the caller no longer needs it.
  // signal 在调用方不再需要结果时中止底层请求。
  signal?: AbortSignal;
  // keepalive lets the request finish after the page unloads, for a write made while leaving; transports that
  // cannot keep it alive send it normally.
  // keepalive 让请求在页面卸载后仍能完成，用于离开页面时的写入；无法保活的传输按普通请求发送。
  keepalive?: boolean;
}

// ServiceErrorPayload describes a normalized service error.
// ServiceErrorPayload 描述标准化后的服务错误。
export interface ServiceErrorPayload {
  // code is a stable machine-readable error code.
  // code 是稳定的机器可读错误码。
  code: string;
  // message is the user-facing or developer-facing error text.
  // message 是面向用户或开发者的错误文本。
  message: string;
  // detail stores optional structured error context.
  // detail 保存可选的结构化错误上下文。
  detail?: unknown;
  // retryable indicates whether retrying the request may succeed.
  // retryable 表示重试请求是否可能成功。
  retryable?: boolean;
}

// ServiceResponse describes one logical RPC response from the Core service.
// ServiceResponse 描述 Core 服务返回的一次逻辑 RPC 响应。
export interface ServiceResponse<T = unknown> {
  // id matches the original request identifier.
  // id 对应原始请求标识。
  id: string;
  // ok indicates whether the service method succeeded.
  // ok 表示服务方法是否执行成功。
  ok: boolean;
  // result stores the successful response payload.
  // result 保存成功响应载荷。
  result?: T;
  // error stores the normalized failure payload.
  // error 保存标准化失败载荷。
  error?: ServiceErrorPayload;
}

// ServiceEventEnvelope wraps a pushed event with its protocol topic.
// ServiceEventEnvelope 使用协议主题包装服务端推送事件。
export interface ServiceEventEnvelope<T = unknown> {
  // topic identifies the event stream, for example workbench.delta.
  // topic 标识事件流，例如 workbench.delta。
  topic: string;
  // payload carries the event-specific data.
  // payload 承载事件特定数据。
  payload: T;
}

// ServiceEventHandler receives one typed event payload.
// ServiceEventHandler 接收一个类型化事件载荷。
export type ServiceEventHandler<T = unknown> = (payload: T) => void;

// ServiceUnsubscribe removes a previously registered event handler.
// ServiceUnsubscribe 移除之前注册的事件处理函数。
export type ServiceUnsubscribe = () => void;

// ServiceTransport defines the transport boundary used by the WebUI.
// ServiceTransport 定义 WebUI 使用的传输边界。
export interface ServiceTransport {
  // request sends one RPC call and resolves with the typed result payload.
  // request 发送一次 RPC 调用并解析为类型化结果载荷。
  request<T>(method: string, params?: ServiceParams, options?: ServiceRequestOptions): Promise<T>;

  // subscribe registers an event handler for one topic.
  // subscribe 为指定主题注册事件处理函数。
  subscribe<T>(topic: string, handler: ServiceEventHandler<T>): Promise<ServiceUnsubscribe>;

  // dispose releases transport resources such as socket or native listeners.
  // dispose 释放传输资源，例如 socket 或原生监听器。
  dispose?(): void;
}

// ServiceTransportUnavailableError reports that the selected transport cannot run here.
// ServiceTransportUnavailableError 表示当前环境无法运行所选传输。
export class ServiceTransportUnavailableError extends Error {
  // constructor builds a stable unavailable-transport error.
  // constructor 构造稳定的传输不可用错误。
  constructor(message: string) {
    super(message);
    this.name = "ServiceTransportUnavailableError";
  }
}
