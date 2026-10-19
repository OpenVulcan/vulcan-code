import * as React from "react";
import { Send } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { StatusPill } from "@/components/shared/status";
import { Button } from "@/ui-kit/primitives/button";
import { Switch } from "@/ui-kit/primitives/switch";
import { useI18n } from "@/i18n";
import { loadNotificationPreferences, saveNotificationPreferences } from "@/notifications/preferences";
import type { NotificationPreferencesV1 } from "@/notifications/types";
import { shellClient } from "@/service/webui-service-runtime";
import type { ShellNotificationPermission } from "@/host-shell";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import "@/ui-kit/styles/features/settings-forms.css"

// NotificationSettingsArea separates durable user intent, native permission and delivery feedback.
// NotificationSettingsArea 分离持久化用户意图、原生权限与投递反馈。
// Parameters: none. Returns device-local settings and a user-triggered test action.
// 参数：无。返回设备本地设置与用户触发的测试操作。
export function NotificationSettingsArea() {
  // t selects the active language.
  // t 选择当前语言。
  const { t } = useI18n();
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
  // error retains preference failures independently from the permission value.
  // error 独立于权限值保留偏好保存失败。
  const [error, setError] = React.useState("");
  // Permission-query failure is independent from preference saving and notification delivery failures.
  // 权限查询失败独立于偏好保存和通知投递失败。
  const [permissionError, setPermissionError] = React.useState("");
  // permissionRevision rejects stale focus-refresh responses.
  // permissionRevision 拒绝过期的焦点刷新响应。
  const permissionRevision = React.useRef(0);

  React.useEffect(() => {
    // cancelled and unsubscribe protect the lifecycle of the native observer.
    // cancelled 与 unsubscribe 保护原生观察器生命周期。
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    // refreshPermission only reads authorization; browsers require a separate user gesture to request it.
    // refreshPermission 只读取授权状态；浏览器申请权限必须由单独的用户操作触发。
    // Parameters: none. Returns when the current client permission resolves.
    // 参数：无。返回值：当前客户端权限状态读取结束后完成。
    async function refreshPermission(): Promise<void> {
      // revision identifies this exact permission request.
      // revision 标识此次精确权限请求。
      const revision = ++permissionRevision.current;
      try {
        // nextPermission is the current native or browser permission fact.
        // nextPermission 是当前原生或浏览器权限事实。
        const nextPermission = await shellClient.getNotificationPermission();
        if (cancelled || revision !== permissionRevision.current) return;
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
      return true;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      return false;
    }
  };

  // requestPermission starts the current client's permission request inside the explicit user gesture.
  // requestPermission 在明确的用户操作内发起当前客户端的权限申请。
  // Parameters: none. Returns after the browser or desktop host resolves the request.
  // 参数：无。返回值：浏览器或桌面宿主完成申请后结束。
  const requestPermission = async (): Promise<void> => {
    if (busyRef.current || !capability.supported) return;
    busyRef.current = true;
    setBusy(true);
    ++permissionRevision.current;
    setError("");
    setPermissionError("");
    try {
      // The shell call must occur before any await so a Web Notification prompt retains user activation.
      // 壳调用必须发生在任何 await 之前，使网页通知弹框保留用户激活资格。
      const nextPermission = await shellClient.requestNotificationPermission();
      setPermission(nextPermission);
    } catch (cause) {
      setPermission(null);
      setPermissionError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  // recheckPermission reads an existing system or browser policy after the user changes it externally.
  // recheckPermission 在用户从外部更改系统或浏览器策略后重新读取既有权限。
  // Parameters: none. Returns after the current client reports its permission.
  // 参数：无。返回值：当前客户端报告权限后结束。
  const recheckPermission = async (): Promise<void> => {
    if (busyRef.current || !capability.supported) return;
    busyRef.current = true;
    setBusy(true);
    ++permissionRevision.current;
    setPermissionError("");
    try {
      setPermission(await shellClient.getNotificationPermission());
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
      if (currentPermission !== "granted") {
        toast.error(t("settings.notifications.noNotificationPermissionYet"));
        return;
      }
      await shellClient.showLocalNotification({
        id: "notification-settings-test",
        title: "Vulcan Code",
        body: t("settings.notifications.notificationsWorkYouLl"),
      });
      toast.success(t("settings.notifications.sentCheckTheCorner"));
    } catch (cause) {
      toast.error(t("settings.notifications.theTestNotificationDidn"), {
        description: cause instanceof Error ? cause.message : String(cause),
      });
      // A failed delivery remains inspectable in settings after the transient toast disappears.
      // 投递失败在瞬时提示消失后仍可于设置页查看。
      setError(cause instanceof Error ? cause.message : String(cause));
      if (!permissionChecked) {
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
  const permissionLabel = permissionError ? t("settings.notifications.permissionCheckFailed") : permission === null ? t("settings.notifications.checking") : t(`settings.notifications.permission.${permission}`);
  // Keep the actionable permission failure visible even when an unrelated preference was saved successfully.
  // 即使无关偏好保存成功，也继续展示可操作的权限失败。
  const displayedError = permissionError || error;
  return <section className="settings-preferences-page settings-detail-panel notification-settings-page">
    <section className="settings-preferences-section">
      <div className="settings-preference-row">
        <div><h2>{t("settings.notifications.localChannel")}</h2><p className="vk-form-note">{t("settings.notifications.showASystemNotification")}</p></div>
        <Switch aria-label={t("settings.notifications.localChannel")} checked={enabled}
          disabled={!capability.supported || busy} onCheckedChange={(checked) => void changeEnabled(checked)} />
      </div>
      {/* Keep permission feedback and its test action together without adding another settings row.
          将权限反馈与测试操作放在一起，避免额外堆叠设置行。 */}
      <div className="notification-permission-row">
        <div className="vk-form-inline">
        <StatusPill status={effective ? "ready" : enabled ? "warning" : "pending"}
          label={effective ? t("settings.notifications.enabled") : enabled ? permissionError ? t("settings.notifications.checkFailed") : permission === "denied" ? t("settings.notifications.blockedBySystem") : t("settings.notifications.waitingForPermission") : t("settings.notifications.off")} />
        <span className="vk-form-note">{t("settings.notifications.permission")}：{permissionLabel}</span>
        </div>
        <Button type="button" size="sm" variant="outline" disabled={busy || !effective} onClick={() => void testNotification()}><Send size={14} />{t("settings.notifications.sendATest")}</Button>
      </div>
      {/* One action per state: ask when never asked, otherwise recheck, since re-asking after a denial is silently ignored.
          每种状态只给一个操作：从未询问时申请，否则重新检查；被拒绝后再次申请会被系统静默忽略。 */}
      {enabled && (permission === "default" || permission === "denied" || permissionError) && capability.supported ? <div className="vk-form-inline">
        <Button type="button" variant="outline" disabled={busy} onClick={() => void (permission === "default" && !permissionError ? requestPermission() : recheckPermission())}>
          {permission === "default" && !permissionError ? t("settings.notifications.allowNotifications") : t("settings.notifications.checkAgain")}
        </Button>
        {permission === "denied" ? <span className="vk-form-note">{t("settings.notifications.notificationsAreBlockedAllow")}</span> : null}
      </div> : null}
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
      <p className="vk-form-note">{t("settings.notifications.noNotificationForThe")}</p>
    </section>
    <section className="settings-preferences-section">
      <h2>{t("settings.notifications.privacy")}</h2>
      <label className="settings-preference-row">
        <div>
          <span>{t("settings.notifications.showTitle")}</span>
          <p className="vk-form-note">{t("settings.notifications.privacy.description")}</p>
        </div>
        <Switch checked={preferences.privacy.showConversationTitle} onCheckedChange={(checked) => replacePreferences({
          ...preferencesRef.current, privacy: { showConversationTitle: checked },
        })} />
      </label>
    </section>
    {displayedError ? <SettingsErrorNotice className="settings-save-error" detail={displayedError} message={permissionError ? t("settings.notifications.couldnTReadThe") : t("settings.notifications.thatDidnTFinish")} /> : null}
    {/* Platform limits stay visible as a regular information section, with no disclosure control.
        平台能力边界作为普通说明区常显，不使用折叠控件。 */}
    <section className="settings-preferences-section notification-capability-note">
      <h2>{t("settings.notifications.limitations")}</h2>
      <p className="vk-form-note">{capability.shellKind === "browser" ? t("settings.notifications.browserLimitation") : t("settings.notifications.desktopLimitation")}</p>
    </section>
  </section>;
}
