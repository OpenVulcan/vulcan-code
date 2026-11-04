import { serviceClient } from "@/service/webui-service-runtime";
import type { AppUpdateSettings, AppUpdateSnapshot } from "@/service/app-update-events";

// Calls of the in-app updater (vulcan-service-api `app_update`).
// 应用内更新器的调用（vulcan-service-api `app_update`）。

// Each call returns the snapshot right after the service accepted it; progress arrives as events.
// 每个调用在服务接受后立即返回快照；进度以事件送达。

export function loadAppUpdateStatus(): Promise<AppUpdateSnapshot> {
  return serviceClient.request<AppUpdateSnapshot>("service_app_update_status", {});
}

export function checkAppUpdate(): Promise<AppUpdateSnapshot> {
  return serviceClient.request<AppUpdateSnapshot>("service_check_app_update", {});
}

export function downloadAppUpdate(): Promise<AppUpdateSnapshot> {
  return serviceClient.request<AppUpdateSnapshot>("service_download_app_update", {});
}

export function cancelAppUpdate(): Promise<AppUpdateSnapshot> {
  return serviceClient.request<AppUpdateSnapshot>("service_cancel_app_update", {});
}

export function restartToUpdate(): Promise<AppUpdateSnapshot> {
  return serviceClient.request<AppUpdateSnapshot>("service_restart_to_update", {});
}

export function repairAppInstall(): Promise<AppUpdateSnapshot> {
  return serviceClient.request<AppUpdateSnapshot>("service_repair_app_install", {});
}

export function dismissAppUpdateNotice(): Promise<AppUpdateSnapshot> {
  return serviceClient.request<AppUpdateSnapshot>("service_dismiss_app_update_notice", {});
}

export function saveAppUpdateSettings(settings: AppUpdateSettings): Promise<AppUpdateSnapshot> {
  return serviceClient.request<AppUpdateSnapshot>("service_save_app_update_settings", { settings });
}
