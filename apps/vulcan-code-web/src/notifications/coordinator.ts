import { createConversationNotificationIntent } from "@/notifications/intent";
import { LocalShellNotificationEndpoint } from "@/notifications/endpoints/local-shell-endpoint";
import { loadNotificationPreferences } from "@/notifications/preferences";
import { getNotificationPresentationContext } from "@/notifications/presentation-context";
import { notificationClientInstanceId } from "@/notifications/client-identity";
import {
  currentNotificationRouteStorage,
  loadNotificationRoutes,
  saveNotificationRoutes,
  type NotificationRouteStorage,
} from "@/notifications/route-storage";
import type {
  NotificationDeliveryEndpoint,
  NotificationDeliveryResult,
  NotificationPreferencesV1,
  NotificationPresentationContext,
  NotificationRouteV1,
  NotificationTextResolver,
} from "@/notifications/types";
import { shellClient } from "@/service/webui-service-runtime";
import type { ShellClient, ShellNotificationActivation } from "@/shell";
import type { ConversationRunSettledEvent } from "@/types";

// CONSUMED_EVENT_LIMIT bounds reconnect deduplication memory for one client lifetime.
// CONSUMED_EVENT_LIMIT 限制单个客户端生命周期内的重连去重内存。
const CONSUMED_EVENT_LIMIT = 512;

// NotificationRouteHandler opens one platform-neutral route in the active client UI.
// NotificationRouteHandler 在当前客户端界面中打开一个与平台无关的路由。
type NotificationRouteHandler = (route: NotificationRouteV1) => void;

// NotificationPreferencesLoader reads the current client-local preference document.
// NotificationPreferencesLoader 读取当前客户端本地偏好文档。
type NotificationPreferencesLoader = () => NotificationPreferencesV1;

// NotificationPresentationReader reads the current exact transcript surface.
// NotificationPresentationReader 读取当前精确转录界面。
type NotificationPresentationReader = () => NotificationPresentationContext;

// NotificationCoordinator derives intents, deduplicates events, and dispatches independent endpoints.
// NotificationCoordinator 派生意图、去重事件并分发独立端点。
export class NotificationCoordinator {
  // shell owns optional local notification activation events.
  // shell 持有可选的本地通知激活事件。
  private readonly shell: ShellClient;
  // endpoints contains the current local endpoint and future independently registered transports.
  // endpoints 包含当前本地端点及未来可独立注册的传输端点。
  private readonly endpoints: NotificationDeliveryEndpoint[];
  // loadPreferences reads preferences at event time for immediate settings consistency.
  // loadPreferences 在事件处理时读取偏好，确保设置立即一致。
  private readonly loadPreferences: NotificationPreferencesLoader;
  // readPresentation reads the exact current client surface at event time.
  // readPresentation 在事件处理时读取当前客户端精确界面。
  private readonly readPresentation: NotificationPresentationReader;
  // clientInstanceId is the only service-event notification owner accepted by this coordinator.
  // clientInstanceId 是当前协调器唯一接受的服务事件通知所有者。
  private readonly clientInstanceId: string;
  // routeStorage retains activation targets across a reload of this exact client.
  // routeStorage 跨此精确客户端的页面重载保留通知激活目标。
  private readonly routeStorage: NotificationRouteStorage | null;
  // consumedEventIds prevents repeated service events from producing repeated delivery.
  // consumedEventIds 防止重复服务事件产生重复投递。
  private readonly consumedEventIds = new Set<string>();
  // consumedEventOrder preserves bounded FIFO eviction order.
  // consumedEventOrder 保存有界先进先出淘汰顺序。
  private readonly consumedEventOrder: string[] = [];
  // routesByNotificationId correlates shell activation with platform-neutral navigation.
  // routesByNotificationId 将壳激活与平台无关导航关联。
  private readonly routesByNotificationId = new Map<string, NotificationRouteV1>();
  // pendingActivationIds retains known clicks until the React navigation owner mounts.
  // pendingActivationIds 在 React 导航所有者挂载前保留已知通知点击。
  private readonly pendingActivationIds = new Set<string>();
  // routeHandler stores the current React-owned navigation callback.
  // routeHandler 保存当前由 React 持有的导航回调。
  private routeHandler: NotificationRouteHandler | null = null;
  // activationStartPromise shares one in-flight or completed shell activation subscription.
  // activationStartPromise 共享一个进行中或已完成的壳激活订阅。
  private activationStartPromise: Promise<void> | null = null;
  // permissionStartPromise deduplicates mounts in this client lifetime, never across restarts.
  // permissionStartPromise 仅合并本次客户端生命周期内的挂载，不跨重启保存。
  private permissionStartPromise: Promise<void> | null = null;

  // constructor creates a coordinator with the current shell and explicit endpoint registry.
  // constructor 使用当前壳与显式端点注册表创建协调器。
  constructor(
    shell: ShellClient,
    endpoints: NotificationDeliveryEndpoint[] = [new LocalShellNotificationEndpoint(shell)],
    loadPreferences: NotificationPreferencesLoader = loadNotificationPreferences,
    readPresentation: NotificationPresentationReader = getNotificationPresentationContext,
    clientInstanceId: string = notificationClientInstanceId,
    routeStorage: NotificationRouteStorage | null = currentNotificationRouteStorage(),
  ) {
    this.shell = shell;
    this.endpoints = endpoints;
    this.loadPreferences = loadPreferences;
    this.readPresentation = readPresentation;
    this.clientInstanceId = clientInstanceId;
    this.routeStorage = routeStorage;
    for (const stored of loadNotificationRoutes(routeStorage, clientInstanceId)) {
      this.routesByNotificationId.set(stored.notificationId, stored.route);
    }
  }

  // start requests enabled notifications and subscribes to activation independently.
  // start 独立发起已启用通知的权限申请与激活订阅。
  // Parameters: none. Returns when both startup actions settle, rejecting actionable failures.
  // 参数：无。返回值：两项启动操作完成后结束，失败时向调用者报告。
  async start(): Promise<void> {
    await Promise.all([this.requestStartupPermission(), this.subscribeToActivation()]);
  }

  // requestStartupPermission uses saved intent, never a previous grant or denial, as its gate.
  // requestStartupPermission 以已保存的启用意图为门控，不以前次授权或拒绝结果为门控。
  // Parameters: none. Returns after this client's shell authorization request completes.
  // 参数：无。返回值：当前客户端壳的授权请求完成后结束。
  private async requestStartupPermission(): Promise<void> {
    if (!this.loadPreferences().channels.local.enabled || !this.shell.getNotificationCapability().supported) return;
    if (this.permissionStartPromise) return this.permissionStartPromise;
    // request is local to this coordinator; creating the next client always requests again.
    // request 仅属于当前协调器；下次创建客户端始终重新请求。
    const request = this.shell.requestNotificationPermission().then(() => undefined);
    this.permissionStartPromise = request;
    try {
      await request;
    } catch (error) {
      if (this.permissionStartPromise === request) this.permissionStartPromise = null;
      throw error;
    }
  }

  // subscribeToActivation subscribes once without coupling delivery routing to authorization.
  // subscribeToActivation 单次订阅激活事件，不将投递导航与授权结果耦合。
  // Parameters: none. Returns after subscription succeeds; failures allow a later retry.
  // 参数：无。返回值：订阅成功后结束；失败允许后续重试。
  private async subscribeToActivation(): Promise<void> {
    if (!this.shell.subscribeNotificationActivation) {
      return;
    }
    if (this.activationStartPromise) {
      return this.activationStartPromise;
    }
    // startPromise collapses concurrent React startup calls into one shell subscription.
    // startPromise 将并发 React 启动调用折叠为单个壳订阅。
    const startPromise = this.shell.subscribeNotificationActivation((activation) => {
      this.handleActivation(activation);
    }).then(() => undefined);
    this.activationStartPromise = startPromise;
    try {
      await startPromise;
    } catch (error) {
      if (this.activationStartPromise === startPromise) {
        this.activationStartPromise = null;
      }
      throw error;
    }
  }

  // setRouteHandler installs or clears the current UI navigation owner.
  // setRouteHandler 安装或清除当前界面导航所有者。
  setRouteHandler(handler: NotificationRouteHandler | null): void {
    this.routeHandler = handler;
    if (handler === null) return;
    for (const notificationId of this.pendingActivationIds) {
      this.pendingActivationIds.delete(notificationId);
      // route was validated and restored before its activation entered the pending set.
      // route 在激活进入待处理集合前已经经过验证并恢复。
      const route = this.routesByNotificationId.get(notificationId);
      if (route) handler(route);
    }
  }

  // handleConversationRunSettled consumes one service event and dispatches each registered endpoint once.
  // handleConversationRunSettled 消费一个服务事件，并向每个已注册端点分发一次。
  async handleConversationRunSettled(
    event: ConversationRunSettledEvent,
    conversationTitle: string | null,
    resolveText: NotificationTextResolver,
  ): Promise<NotificationDeliveryResult[]> {
    if (event.notificationClientId !== this.clientInstanceId) {
      return [{ endpointKind: "local", status: "suppressed", reason: "foreign_client" }];
    }
    if (!this.consumeEvent(event.eventId)) {
      return [{ endpointKind: "local", status: "suppressed", reason: "duplicate_event" }];
    }
    // preferences is read at event time so settings changes apply without coordinator remounting.
    // preferences 在事件处理时读取，使设置变更无需重新挂载协调器即可生效。
    const preferences = this.loadPreferences();
    // titleForIntent enforces the current privacy preference before serializable intent creation.
    // titleForIntent 在创建可序列化意图前执行当前隐私偏好。
    const titleForIntent = preferences.privacy.showConversationTitle ? conversationTitle : null;
    // intent is shared unchanged by every current or future endpoint.
    // intent 由所有当前或未来端点原样共享。
    const intent = createConversationNotificationIntent(event, titleForIntent);
    this.routesByNotificationId.set(intent.intentId, intent.route);
    this.trimRoutes();
    saveNotificationRoutes(this.routeStorage, this.clientInstanceId,
      [...this.routesByNotificationId].map(([notificationId, route]) => ({ notificationId, route })));
    // presentation is read once at event handling time to avoid cross-endpoint route races.
    // presentation 在事件处理时读取一次，避免跨端点路由竞态。
    const presentation = this.readPresentation();
    return Promise.all(this.endpoints.map((endpoint) => (
      endpoint.deliver(intent, preferences, presentation, resolveText)
    )));
  }

  // consumeEvent atomically records one source id and maintains the bounded FIFO set.
  // consumeEvent 原子记录一个来源标识并维护有界先进先出集合。
  private consumeEvent(eventId: string): boolean {
    if (this.consumedEventIds.has(eventId)) {
      return false;
    }
    this.consumedEventIds.add(eventId);
    this.consumedEventOrder.push(eventId);
    if (this.consumedEventOrder.length > CONSUMED_EVENT_LIMIT) {
      // oldestEventId is guaranteed by the preceding length check.
      // oldestEventId 由前置长度检查保证存在。
      const oldestEventId = this.consumedEventOrder.shift();
      if (oldestEventId !== undefined) {
        this.consumedEventIds.delete(oldestEventId);
      }
    }
    return true;
  }

  // trimRoutes keeps activation correlation bounded by the same client-lifetime limit.
  // trimRoutes 使用相同客户端生命周期上限约束激活关联。
  private trimRoutes(): void {
    while (this.routesByNotificationId.size > CONSUMED_EVENT_LIMIT) {
      // oldestNotificationId follows Map insertion order and is present while the map exceeds the limit.
      // oldestNotificationId 遵循 Map 插入顺序，并在映射超限时必然存在。
      const oldestNotificationId = this.routesByNotificationId.keys().next().value;
      if (typeof oldestNotificationId !== "string") {
        return;
      }
      this.routesByNotificationId.delete(oldestNotificationId);
      this.pendingActivationIds.delete(oldestNotificationId);
    }
  }

  // handleActivation forwards one shell click to the current UI route owner.
  // handleActivation 将一次壳点击转发给当前界面路由所有者。
  private handleActivation(activation: ShellNotificationActivation): void {
    // route is the exact intent route registered for the displayed notification id.
    // route 是为已展示通知标识注册的精确意图路由。
    const route = this.routesByNotificationId.get(activation.notificationId);
    if (!route) return;
    if (this.routeHandler) this.routeHandler(route);
    else this.pendingActivationIds.add(activation.notificationId);
  }
}

// notificationCoordinator is the single client-lifetime coordinator shared across React remounts.
// notificationCoordinator 是跨 React 重新挂载共享的单一客户端生命周期协调器。
export const notificationCoordinator = new NotificationCoordinator(shellClient);
