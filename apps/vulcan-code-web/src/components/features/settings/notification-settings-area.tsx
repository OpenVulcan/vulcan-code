import * as React from "react";
import { Bell } from "lucide-react";
import { PanelTitle, StatusPill } from "@/components/shared/status";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useI18n } from "@/i18n";
import {
  loadNotificationPreferences,
  saveNotificationPreferences,
} from "@/notifications/preferences";
import type { NotificationPreferencesV1 } from "@/notifications/types";
import { shellClient } from "@/service/webui-service-runtime";
import type { ShellNotificationPermission } from "@/shell";

// NotificationSettingsArea manages per-client shell notification preferences and authorization.
// NotificationSettingsArea 管理每个客户端的壳通知偏好与授权。
export function NotificationSettingsArea() {
  // t localizes all notification settings labels.
  // t 本地化全部通知设置标签。
  const { t } = useI18n();
  // capability is static metadata for the current shell instance.
  // capability 是当前壳实例的静态元数据。
  const capability = shellClient.getNotificationCapability();
  // preferences stores the complete device-local V1 document.
  // preferences 保存完整的设备本地 V1 文档。
  const [preferences, setPreferences] = React.useState<NotificationPreferencesV1>(loadNotificationPreferences);
  // permission stores the latest non-prompting or user-requested authorization result.
  // permission 保存最近一次无提示查询或用户请求的授权结果。
  const [permission, setPermission] = React.useState<ShellNotificationPermission>(
    capability.supported ? "default" : "unsupported",
  );
  // requestingPermission prevents overlapping authorization prompts.
  // requestingPermission 防止授权提示重叠。
  const [requestingPermission, setRequestingPermission] = React.useState(false);

  React.useEffect(() => {
    // cancelled prevents a late shell response from updating an unmounted settings page.
    // cancelled 防止延迟壳响应更新已卸载的设置页。
    let cancelled = false;
    // unsubscribe releases the shell focus and visibility observer after it becomes available.
    // unsubscribe 在壳焦点与可见性观察器可用后负责释放它。
    let unsubscribe: (() => void) | null = null;
    // refreshPermission reads authorization without prompting after focus or visibility changes.
    // refreshPermission 在焦点或可见性变化后无提示读取授权。
    async function refreshPermission(): Promise<void> {
      try {
        // nextPermission is the exact non-prompting shell authorization snapshot.
        // nextPermission 是精确的无提示壳授权快照。
        const nextPermission = await shellClient.getNotificationPermission();
        if (!cancelled) {
          setPermission(nextPermission);
        }
      } catch (error) {
        console.warn("Notification permission query failed.", error);
      }
    }
    void refreshPermission();
    void shellClient.subscribeNotificationPresentationChange(() => {
      void refreshPermission();
    }).then((nextUnsubscribe) => {
      if (cancelled) {
        nextUnsubscribe();
      } else {
        unsubscribe = nextUnsubscribe;
      }
    }).catch((error) => {
      console.warn("Notification permission refresh subscription failed.", error);
    });
    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, []);

  // replacePreferences commits one complete document to React and device-local storage.
  // replacePreferences 将一份完整文档提交到 React 与设备本地存储。
  const replacePreferences = React.useCallback((next: NotificationPreferencesV1) => {
    try {
      saveNotificationPreferences(next);
      setPreferences(next);
    } catch (error) {
      console.warn("Notification preference persistence failed.", error);
    }
  }, []);

  // handleLocalChannelChange requests permission only from this explicit checkbox action.
  // handleLocalChannelChange 仅通过本次明确的复选框操作请求权限。
  const handleLocalChannelChange = React.useCallback(async (enabled: boolean) => {
    if (!enabled) {
      replacePreferences({
        ...preferences,
        channels: { local: { enabled: false } },
      });
      return;
    }
    if (permission === "denied" || permission === "unsupported") {
      return;
    }
    setRequestingPermission(true);
    try {
      // nextPermission is the exact result of the user-triggered shell authorization request.
      // nextPermission 是用户触发壳授权请求的精确结果。
      const nextPermission = permission === "granted"
        ? permission
        : await shellClient.requestNotificationPermission();
      setPermission(nextPermission);
      replacePreferences({
        ...preferences,
        channels: { local: { enabled: nextPermission === "granted" } },
      });
    } catch (error) {
      console.warn("Notification permission request failed.", error);
    } finally {
      setRequestingPermission(false);
    }
  }, [permission, preferences, replacePreferences]);

  // updateCategories replaces one category preference while preserving every other domain.
  // updateCategories 替换单个类别偏好，同时保留其他全部领域。
  const updateCategories = React.useCallback((field: "conversationCompleted" | "conversationFailed", enabled: boolean) => {
    replacePreferences({
      ...preferences,
      categories: { ...preferences.categories, [field]: enabled },
    });
  }, [preferences, replacePreferences]);

  // updateTitlePrivacy replaces only the local lock-screen title disclosure preference.
  // updateTitlePrivacy 仅替换本地锁屏标题披露偏好。
  const updateTitlePrivacy = React.useCallback((enabled: boolean) => {
    replacePreferences({
      ...preferences,
      privacy: { showConversationTitle: enabled },
    });
  }, [preferences, replacePreferences]);

  // permissionLabel maps the normalized shell permission to localized status text.
  // permissionLabel 将标准化壳权限映射为本地化状态文案。
  const permissionLabel = t(`settings.notifications.permission.${permission}`);
  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<Bell size={17} />} title={t("settings.notifications")} />
      <div className="settings-form-grid">
        <article className="settings-card">
          <StatusPill
            status={capability.supported ? "ready" : "warning"}
            label={capability.supported
              ? t("settings.notifications.supported")
              : t("settings.notifications.unsupported")}
          />
          <strong>{t("settings.notifications.localChannel")}</strong>
          <p>{t("settings.notifications.localChannel.description")}</p>
          <label className="settings-form-field">
            <span>{t("settings.notifications.permission")}: {permissionLabel}</span>
            <Switch
              checked={preferences.channels.local.enabled}
              disabled={!capability.supported || permission === "denied" || requestingPermission}
              onCheckedChange={(checked) => void handleLocalChannelChange(checked)}
            />
          </label>
          {capability.supported && permission === "default" ? (
            <Button
              disabled={requestingPermission}
              onClick={() => void handleLocalChannelChange(true)}
              type="button"
              variant="outline"
            >
              {requestingPermission
                ? t("settings.notifications.requesting")
                : t("settings.notifications.requestPermission")}
            </Button>
          ) : null}
        </article>
        <article className="settings-card">
          <strong>{t("settings.notifications.categories")}</strong>
          <label className="settings-form-field">
            <span>{t("settings.notifications.completed")}</span>
            <Switch
              checked={preferences.categories.conversationCompleted}
              onCheckedChange={(checked) => updateCategories("conversationCompleted", checked)}
            />
          </label>
          <label className="settings-form-field">
            <span>{t("settings.notifications.failed")}</span>
            <Switch
              checked={preferences.categories.conversationFailed}
              onCheckedChange={(checked) => updateCategories("conversationFailed", checked)}
            />
          </label>
        </article>
        <article className="settings-card">
          <strong>{t("settings.notifications.privacy")}</strong>
          <p>{t("settings.notifications.privacy.description")}</p>
          <label className="settings-form-field">
            <span>{t("settings.notifications.showTitle")}</span>
            <Switch
              checked={preferences.privacy.showConversationTitle}
              onCheckedChange={updateTitlePrivacy}
            />
          </label>
        </article>
        <article className="settings-card">
          <strong>{t("settings.notifications.limitations")}</strong>
          <p>{capability.shellKind === "browser"
            ? t("settings.notifications.browserLimitation")
            : t("settings.notifications.desktopLimitation")}</p>
        </article>
      </div>
    </section>
  );
}
