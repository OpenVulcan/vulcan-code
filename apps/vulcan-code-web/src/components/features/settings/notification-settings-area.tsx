import * as React from "react";
import { Bell, Send } from "lucide-react";
import { PanelTitle, StatusPill } from "@/components/shared/status";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useI18n } from "@/i18n";
import { loadNotificationPreferences, saveNotificationPreferences } from "@/notifications/preferences";
import type { NotificationPreferencesV1 } from "@/notifications/types";
import { shellClient } from "@/service/webui-service-runtime";
import type { ShellNotificationPermission } from "@/shell";

// NotificationSettingsArea separates durable user intent, native permission and delivery feedback.
// NotificationSettingsArea 分离持久化用户意图、原生权限与投递反馈。
// Parameters: none. Returns device-local settings and a user-triggered test action.
// 参数：无。返回设备本地设置与用户触发的测试操作。
export function NotificationSettingsArea() {
  // t and locale select the active language.
  // t 与 locale 选择当前语言。
  const { t, locale } = useI18n();
  // text provides localized copy for precise delivery states.
  // text 为精确投递状态提供本地化文案。
  const text = (zh: string, en: string) => locale === "zh-CN" ? zh : en;
  // capability describes this shell without claiming permission has been granted.
  // capability 描述当前壳，不将支持能力当成已经获得权限。
  const capability = shellClient.getNotificationCapability();
  // preferences contains the persisted intent and per-category choices.
  // preferences 包含持久化启用意图与分类选择。
  const [preferences, setPreferences] = React.useState<NotificationPreferencesV1>(loadNotificationPreferences);
  // preferencesRef prevents an asynchronous permission response from overwriting newer choices.
  // preferencesRef 防止异步权限响应覆盖较新的选择。
  const preferencesRef = React.useRef(preferences);
  // permission is unknown until an authoritative shell query completes.
  // permission 在权威壳查询完成前保持未知。
  const [permission, setPermission] = React.useState<ShellNotificationPermission | null>(capability.supported ? null : "unsupported");
  // busy and busyRef prevent overlapping user-triggered permission or test actions.
  // busy 与 busyRef 防止用户触发的权限或测试操作重叠。
  const [busy, setBusy] = React.useState(false);
  const busyRef = React.useRef(false);
  // error and feedback retain concise actionable results outside the permission value.
  // error 与 feedback 在权限值之外保留简短可操作结果。
  const [error, setError] = React.useState("");
  // Permission-query failure is independent from preference saving and notification delivery failures.
  // 权限查询失败独立于偏好保存和通知投递失败。
  const [permissionError, setPermissionError] = React.useState("");
  const [feedback, setFeedback] = React.useState("");
  // permissionRevision rejects stale focus-refresh responses.
  // permissionRevision 拒绝过期的焦点刷新响应。
  const permissionRevision = React.useRef(0);

  React.useEffect(() => {
    // cancelled and unsubscribe protect the lifecycle of the native observer.
    // cancelled 与 unsubscribe 保护原生观察器生命周期。
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    // refreshPermission checks the native authority without prompting or rewriting saved intent.
    // refreshPermission 检查原生权威，不弹出授权或改写已保存意图。
    // Parameters: none. Returns after a non-prompting permission read.
    // 参数：无。返回值：无提示权限读取后完成。
    async function refreshPermission(): Promise<void> {
      // revision identifies this exact permission request.
      // revision 标识此次精确权限请求。
      const revision = ++permissionRevision.current;
      try {
        // nextPermission is the current native or browser permission fact.
        // nextPermission 是当前原生或浏览器权限事实。
        const nextPermission = await shellClient.getNotificationPermission();
        if (!cancelled && revision === permissionRevision.current) {
          setPermission(nextPermission);
          setPermissionError("");
        }
      } catch (cause) {
        if (!cancelled && revision === permissionRevision.current) {
          setPermission(null);
          setPermissionError(cause instanceof Error ? cause.message : String(cause));
        }
      }
    }
    void refreshPermission();
    void shellClient.subscribeNotificationPresentationChange(() => {
      if (!busyRef.current) void refreshPermission();
    }).then((next) => {
      if (cancelled) next(); else unsubscribe = next;
    }).catch((cause) => {
      if (!cancelled) setError(cause instanceof Error ? cause.message : String(cause));
    });
    return () => { cancelled = true; ++permissionRevision.current; unsubscribe?.(); };
  }, []);

  // replacePreferences writes first and updates the interface only when persistence succeeds.
  // replacePreferences 先写存储，仅在持久化成功时更新界面。
  // Parameters: next is the complete validated preference document. Returns whether saving succeeded.
  // 参数：next 是完整有效的偏好文档。返回值：是否保存成功。
  const replacePreferences = (next: NotificationPreferencesV1): boolean => {
    try {
      saveNotificationPreferences(next);
      preferencesRef.current = next;
      setPreferences(next);
      setError("");
      setFeedback("");
      return true;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      return false;
    }
  };

  // requestPermission queries first so a previously granted desktop permission never prompts again.
  // requestPermission 先查询，使此前已经获得的桌面权限不重复请求。
  // Parameters: none. Returns after the explicit user authorization action.
  // 参数：无。返回值：用户明确授权操作结束后完成。
  const requestPermission = async (): Promise<void> => {
    if (busyRef.current || !capability.supported) return;
    busyRef.current = true;
    setBusy(true);
    ++permissionRevision.current;
    setError("");
    setPermissionError("");
    try {
      // currentPermission comes from the shell authority rather than rendered state.
      // currentPermission 来自壳权威，而不是已渲染状态。
      const currentPermission = await shellClient.getNotificationPermission();
      // nextPermission requests only an undecided permission from this explicit action.
      // nextPermission 仅在本次明确操作中请求尚未决定的权限。
      const nextPermission = currentPermission === "default"
        ? await shellClient.requestNotificationPermission()
        : currentPermission;
      setPermission(nextPermission);
    } catch (cause) {
      setPermission(null);
      setPermissionError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  // changeEnabled persists user intent independently from authorization, preserving it across restarts.
  // changeEnabled 将用户意图与授权分开持久化，在重启后继续保留。
  // Parameters: enabled is the explicit new preference. Returns after any user-triggered permission request.
  // 参数：enabled 是明确的新偏好。返回值：用户触发的可选权限请求结束后完成。
  const changeEnabled = async (enabled: boolean): Promise<void> => {
    if (!replacePreferences({ ...preferencesRef.current, channels: { local: { enabled } } })) return;
    if (enabled) await requestPermission();
  };

  // testNotification deliberately bypasses conversation-foreground suppression only for this test button.
  // testNotification 仅为本测试按钮明确跳过会话前台抑制。
  // Parameters: none. Returns after the shell accepts or rejects this test request.
  // 参数：无。返回值：壳接受或拒绝此次测试请求后完成。
  const testNotification = async (): Promise<void> => {
    if (busyRef.current || !preferencesRef.current.channels.local.enabled) return;
    busyRef.current = true;
    setBusy(true);
    setError("");
    setFeedback("");
    ++permissionRevision.current;
    // permissionChecked distinguishes a failed preflight read from a rejected OS delivery.
    // permissionChecked 区分发送前权限读取失败与系统投递拒绝。
    let permissionChecked = false;
    try {
      // currentPermission rechecks authorization immediately before sending.
      // currentPermission 在发送前立即重验授权。
      const currentPermission = await shellClient.getNotificationPermission();
      permissionChecked = true;
      setPermission(currentPermission);
      setPermissionError("");
      if (currentPermission !== "granted") return;
      await shellClient.showLocalNotification({
        id: "notification-settings-test",
        title: "Vulcan Code",
        body: text("通知测试：会话完成后将在此提醒。", "Notification test: completed conversations will appear here."),
      });
      setFeedback(text("测试已提交给系统。若没有出现，请检查系统通知与勿扰设置。", "The test was submitted to the system. If it does not appear, check system notification and Do Not Disturb settings."));
    } catch (cause) {
      if (permissionChecked) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } else {
        setPermission(null);
        setPermissionError(cause instanceof Error ? cause.message : String(cause));
      }
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  // enabled records user intent; effective additionally requires current permission.
  // enabled 记录用户意图；effective 还要求当前权限。
  const enabled = preferences.channels.local.enabled;
  const effective = enabled && permission === "granted";
  // The status describes the current permission fact instead of leaving failed queries in a waiting state.
  // 状态描述当前权限事实，不让失败查询停留在等待状态。
  const permissionLabel = permissionError ? text("权限检查失败", "Permission check failed") : permission === null ? text("正在检查", "Checking") : t(`settings.notifications.permission.${permission}`);
  // Keep the actionable permission failure visible even when an unrelated preference was saved successfully.
  // 即使无关偏好保存成功，也继续展示可操作的权限失败。
  const displayedError = permissionError || error;
  return <section className="settings-preferences-page settings-detail-panel">
    <PanelTitle icon={<Bell size={17} />} title={t("settings.notifications")} />
    <section className="settings-preferences-section">
      <div className="settings-preference-row">
        <div><h2>{t("settings.notifications.localChannel")}</h2><p className="settings-muted-line">{text("保存在当前客户端，下次启动自动恢复。", "Saved on this client and restored at the next launch.")}</p></div>
        <Switch aria-label={t("settings.notifications.localChannel")} checked={enabled}
          disabled={!capability.supported || busy} onCheckedChange={(checked) => void changeEnabled(checked)} />
      </div>
      <div className="settings-card-actions">
        <StatusPill status={effective ? "ready" : enabled ? "warning" : "pending"}
          label={effective ? text("已启用", "Enabled") : enabled ? permissionError ? text("检查失败", "Check failed") : permission === "denied" ? text("系统已阻止", "Blocked by system") : text("等待权限", "Awaiting permission") : text("已关闭", "Off")} />
        <span className="settings-muted-line">{t("settings.notifications.permission")}：{permissionLabel}</span>
      </div>
      {enabled && permission !== "granted" && capability.supported ? <div className="settings-card-actions">
        <Button type="button" variant="outline" disabled={busy} onClick={() => void requestPermission()}>
          {permission === "denied" || permissionError ? text("重新检查权限", "Recheck permission") : t("settings.notifications.requestPermission")}
        </Button>
        {permission === "denied" ? <span className="settings-muted-line">{text("请在系统或浏览器设置中允许通知，再重新检查。", "Allow notifications in system or browser settings, then recheck.")}</span> : null}
      </div> : null}
      <div className="settings-card-actions"><Button type="button" variant="outline" disabled={busy || !effective} onClick={() => void testNotification()}><Send size={14} />{text("发送测试通知", "Send test notification")}</Button></div>
      {feedback ? <p role="status" className="settings-muted-line">{feedback}</p> : null}
    </section>
    <section className="settings-preferences-section">
      <h2>{t("settings.notifications.categories")}</h2>
      {(["conversationCompleted", "conversationFailed"] as const).map((category) => (
        <label className="settings-preference-row" key={category}>
          <span>{t(category === "conversationCompleted" ? "settings.notifications.completed" : "settings.notifications.failed")}</span>
          <Switch checked={preferences.categories[category]} onCheckedChange={(checked) => replacePreferences({
            ...preferencesRef.current, categories: { ...preferencesRef.current.categories, [category]: checked },
          })} />
        </label>
      ))}
      <p className="settings-muted-line">{text("正在前台阅读的同一会话不会重复弹出完成提醒；测试通知不受此限制。", "The conversation you are reading in the foreground does not trigger a duplicate completion alert. Test notifications are unaffected.")}</p>
    </section>
    <section className="settings-preferences-section">
      <h2>{t("settings.notifications.privacy")}</h2>
      <label className="settings-preference-row">
        <span>{t("settings.notifications.showTitle")}</span>
        <Switch checked={preferences.privacy.showConversationTitle} onCheckedChange={(checked) => replacePreferences({
          ...preferencesRef.current, privacy: { showConversationTitle: checked },
        })} />
      </label>
      <p className="settings-muted-line">{t("settings.notifications.privacy.description")}</p>
    </section>
    {displayedError ? <div className="settings-save-error" role="alert"><p>{permissionError ? text("无法读取系统通知权限，请重新检查。", "System notification permission could not be read. Recheck permission.") : text("操作未完成，请查看详情后重试。", "The operation did not complete. Review the details and retry.")}</p><details><summary>{text("查看详情", "View details")}</summary><pre>{displayedError}</pre></details></div> : null}
    <details className="settings-muted-line"><summary>{t("settings.notifications.limitations")}</summary>
      <p>{capability.shellKind === "browser" ? t("settings.notifications.browserLimitation") : t("settings.notifications.desktopLimitation")}</p>
    </details>
  </section>;
}
