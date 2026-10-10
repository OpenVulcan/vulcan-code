import type { Translate, TranslationKey } from "@/i18n";
import type { JudgeDevice } from "@/types";

// Labels of stable codes sent by the service, shared by the settings page and the setup guide.
// 服务发送的稳定代码对应的标签，由设置页与初始化引导共用。
export const DEVICE_KEYS: Record<JudgeDevice, TranslationKey> = {
  cuda: "settings.permissionJudge.device.cuda",
  webgpu: "settings.permissionJudge.device.webgpu",
  cpu: "settings.permissionJudge.device.cpu",
};
export const REASON_KEYS: Record<string, TranslationKey> = {
  runtime_missing: "settings.permissionJudge.reason.runtimeMissing",
  unsupported_platform: "settings.permissionJudge.reason.unsupportedPlatform",
  driver_missing: "settings.permissionJudge.reason.driverMissing",
  driver_too_old: "settings.permissionJudge.reason.driverTooOld",
  onnxruntime_missing: "settings.permissionJudge.reason.onnxruntimeMissing",
  cuda_libraries_missing: "settings.permissionJudge.reason.cudaLibrariesMissing",
};
export const PHASE_KEYS: Record<string, TranslationKey> = {
  not_loaded: "settings.permissionJudge.phase.notLoaded",
  warming: "settings.permissionJudge.phase.warming",
  ready: "settings.permissionJudge.phase.ready",
  failed: "settings.permissionJudge.phase.failed",
};
// Policy categories and modes come from the model in use; codes outside these lists show as is.
// 策略类别与档位来自使用中的模型；不在这些清单中的代码按原样显示。
export const OPTION_KEYS: Record<string, TranslationKey> = {
  cross_project_writes: "projectSettings.permissionJudge.option.crossProjectWrites",
  database_reset: "projectSettings.permissionJudge.option.databaseReset",
  project_reset: "projectSettings.permissionJudge.option.projectReset",
};
export const MODE_KEYS: Record<string, TranslationKey> = {
  allow: "projectSettings.permissionJudge.mode.allow",
  review: "projectSettings.permissionJudge.mode.review",
  confirm: "projectSettings.permissionJudge.mode.confirm",
};

// label translates a known code, or shows the code itself.
// label 翻译已知代码，否则显示代码本身。
export function label(t: Translate, keys: Record<string, TranslationKey>, code: string): string {
  return Object.hasOwn(keys, code) ? t(keys[code]) : code;
}
