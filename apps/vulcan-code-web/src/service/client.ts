import type {
  ServiceEventHandler,
  ServiceParams,
  ServiceTransport,
  ServiceUnsubscribe,
} from "@/service/protocol";

// ServiceClientOptions configures the WebUI service client.
// ServiceClientOptions 配置 WebUI 服务客户端。
export interface ServiceClientOptions {
  // transport is the concrete runtime bridge used by the client.
  // transport 是客户端使用的具体运行时桥接实现。
  transport: ServiceTransport;
}

// ServiceClient is the transport-neutral RPC and event facade used by UI code.
// ServiceClient 是 UI 代码使用的传输无关 RPC 与事件门面。
export class ServiceClient {
  // transport stores the active service transport.
  // transport 保存当前激活的服务传输。
  private readonly transport: ServiceTransport;

  // constructor creates a client over one concrete transport.
  // constructor 基于一个具体传输创建客户端。
  constructor(options: ServiceClientOptions) {
    this.transport = options.transport;
  }

  // request sends one logical service method call.
  // request 发送一次逻辑服务方法调用。
  request<T>(method: string, params?: ServiceParams): Promise<T> {
    return this.transport.request<T>(method, params);
  }

  // subscribe registers one event topic handler.
  // subscribe 注册一个事件主题处理函数。
  subscribe<T>(topic: string, handler: ServiceEventHandler<T>): Promise<ServiceUnsubscribe> {
    return this.transport.subscribe<T>(topic, handler);
  }

  // dispose releases any transport resources owned by this client.
  // dispose 释放此客户端持有的传输资源。
  dispose(): void {
    this.transport.dispose?.();
  }
}

// createServiceClient creates the default facade around a concrete transport.
// createServiceClient 围绕具体传输创建默认门面。
export function createServiceClient(transport: ServiceTransport): ServiceClient {
  return new ServiceClient({ transport });
}
