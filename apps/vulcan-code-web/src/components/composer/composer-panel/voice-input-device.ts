import * as React from "react";

// Device ids are scoped to this browser profile, so the microphone choice stays GUI-local.
// 设备标识只在当前浏览器配置内有效，因此麦克风选择保存在 GUI 本地。
const VOICE_INPUT_DEVICE_KEY = "vulcan.gui.voice-input-device.v1";

// Browsers list their own default and communications aliases; the picker offers one "system default" instead.
// 浏览器会列出自身的 default 与 communications 别名；选择器改为提供一个“系统默认”。
const ALIAS_DEVICE_IDS = new Set(["default", "communications"]);

// AudioInputDevice is one microphone the browser reports.
// AudioInputDevice 是浏览器报告的一个麦克风。
export interface AudioInputDevice {
  deviceId: string;
  // Empty until the page has been granted microphone access.
  // 页面获得麦克风权限之前为空。
  label: string;
}

// readVoiceInputDevice returns the remembered microphone, or null for the system default.
// readVoiceInputDevice 返回记住的麦克风；系统默认时返回 null。
export function readVoiceInputDevice(): string | null {
  try {
    return window.localStorage.getItem(VOICE_INPUT_DEVICE_KEY) || null;
  } catch {
    // An unavailable local store keeps the system default.
    // 本地存储不可用时使用系统默认。
    return null;
  }
}

// saveVoiceInputDevice remembers an explicit microphone choice; null returns to the system default.
// saveVoiceInputDevice 记住明确的麦克风选择；null 表示回到系统默认。
export function saveVoiceInputDevice(deviceId: string | null): void {
  try {
    if (deviceId) window.localStorage.setItem(VOICE_INPUT_DEVICE_KEY, deviceId);
    else window.localStorage.removeItem(VOICE_INPUT_DEVICE_KEY);
  } catch {
    // The choice still applies to the current page through component state.
    // 该选择仍通过组件状态在当前页面生效。
  }
}

// voiceInputAudioConstraints requires the chosen microphone exactly; a missing device fails instead of switching.
// voiceInputAudioConstraints 精确要求所选麦克风；设备缺失时报错而不是改用其他设备。
export function voiceInputAudioConstraints(deviceId: string | null): MediaStreamConstraints {
  return { audio: deviceId ? { deviceId: { exact: deviceId } } : true };
}

// useAudioInputDevices lists microphones while `active`, following plug and unplug events.
// useAudioInputDevices 在 `active` 期间列出麦克风，并跟随插拔事件更新。
export function useAudioInputDevices(active: boolean): AudioInputDevice[] {
  const [devices, setDevices] = React.useState<AudioInputDevice[]>([]);
  React.useEffect(() => {
    const media = typeof navigator === "undefined" ? undefined : navigator.mediaDevices;
    if (!active || !media?.enumerateDevices) return undefined;
    let disposed = false;
    const refresh = () => {
      void media.enumerateDevices().then((all) => {
        if (disposed) return;
        setDevices(all
          .filter((device) => device.kind === "audioinput" && device.deviceId && !ALIAS_DEVICE_IDS.has(device.deviceId))
          .map((device) => ({ deviceId: device.deviceId, label: device.label })));
      }, () => { if (!disposed) setDevices([]); });
    };
    refresh();
    media.addEventListener?.("devicechange", refresh);
    return () => {
      disposed = true;
      media.removeEventListener?.("devicechange", refresh);
    };
  }, [active]);
  return devices;
}
