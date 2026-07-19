import { decideLocalNotification } from "@/notifications/policy";
import type {
  NotificationDeliveryEndpoint,
  NotificationDeliveryResult,
  NotificationIntentV1,
  NotificationPreferencesV1,
  NotificationPresentationContext,
  NotificationTextResolver,
} from "@/notifications/types";
import type { ShellClient } from "@/shell";

// LocalShellNotificationEndpoint delivers eligible intents through the current client shell.
// LocalShellNotificationEndpoint 通过当前客户端壳投递符合条件的意图。
export class LocalShellNotificationEndpoint implements NotificationDeliveryEndpoint {
  // kind identifies this endpoint as local client delivery.
  // kind 将本端点标识为本地客户端投递。
  readonly kind = "local" as const;

  // shell owns permission, presentation facts, and system notification presentation.
  // shell 持有权限、界面事实与系统通知展示能力。
  private readonly shell: ShellClient;

  // constructor binds the endpoint to exactly one client shell.
  // constructor 将端点绑定到唯一客户端壳。
  constructor(shell: ShellClient) {
    this.shell = shell;
  }

  // deliver evaluates current shell facts and presents one eligible notification.
  // deliver 评估当前壳事实并展示一条符合条件的通知。
  async deliver(
    intent: NotificationIntentV1,
    preferences: NotificationPreferencesV1,
    presentation: NotificationPresentationContext,
    resolveText: NotificationTextResolver,
  ): Promise<NotificationDeliveryResult> {
    // capability is static host metadata and never interprets conversation state.
    // capability 是静态宿主元数据，且不会解释会话状态。
    const capability = this.shell.getNotificationCapability();
    if (!capability.supported) {
      return { endpointKind: this.kind, status: "unsupported", reason: "shell_unsupported" };
    }
    try {
      // permission is read without prompting because background events must never request authorization.
      // permission 以无提示方式读取，因为后台事件绝不能请求授权。
      const permission = await this.shell.getNotificationPermission();
      // shellState captures current focus and visibility at event handling time.
      // shellState 捕获事件处理时的当前焦点与可见性。
      const shellState = await this.shell.getNotificationPresentationState();
      // decision is the pure shared policy result for this local endpoint.
      // decision 是本地端点的纯共享策略结果。
      const decision = decideLocalNotification(
        intent,
        preferences,
        presentation,
        shellState,
        capability,
        permission,
      );
      if (!decision.allowed) {
        // status separates absent capability and authorization from ordinary policy suppression.
        // status 将能力缺失和授权问题与普通策略抑制区分开。
        const status = decision.reason === "shell_unsupported"
          ? "unsupported"
          : decision.reason === "permission_denied"
            ? "permissionDenied"
            : "suppressed";
        return { endpointKind: this.kind, status, reason: decision.reason };
      }
      // text is localized only after policy authorizes system presentation.
      // text 仅在策略允许系统展示后执行本地化。
      const text = resolveText(intent);
      await this.shell.showLocalNotification({
        id: intent.intentId,
        title: text.title,
        body: text.body,
      });
      return { endpointKind: this.kind, status: "delivered", reason: "delivered" };
    } catch (error) {
      // reason preserves a stable failure category without leaking the error into conversation execution.
      // reason 保留稳定失败类别，且不会把错误泄漏到会话执行流程。
      const reason = error instanceof Error ? error.message : String(error);
      console.warn("Local notification delivery failed.", error);
      return { endpointKind: this.kind, status: "failed", reason };
    }
  }
}
