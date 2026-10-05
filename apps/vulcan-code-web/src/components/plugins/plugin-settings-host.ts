// PluginMessage resolves one plugin-owned message key validated by Core against the fallback locale.
// PluginMessage 解析一个经 Core 依据回退语言校验的插件自有消息键。
export type PluginMessage = (key: string) => string;

// PluginSettingsHost is the plugin-scoped host surface a declarative page uses; every call is bound to one plugin.
// PluginSettingsHost 是声明式页面使用的插件范围宿主能力；每次调用都绑定到一个插件。
export interface PluginSettingsHost {
  // invoke calls one action declared by the owning plugin manifest; refresh bypasses a completed cache entry.
  // invoke 调用所属插件清单声明的单个 Action；refresh 绕过已完成的缓存条目。
  invoke: <Result>(actionId: string, argumentsValue: unknown, options?: { refresh?: boolean }) => Promise<Result>;
  // message resolves one plugin-owned message key in the active locale.
  // message 以当前语言解析一个插件自有消息键。
  message: PluginMessage;
  // openExternalUrl opens one declared link through the host URL boundary.
  // openExternalUrl 通过宿主地址边界打开一个已声明链接。
  openExternalUrl: (url: string) => Promise<void>;
  // readTextAsset reads one UTF-8 package file listed in the manifest `required_files`.
  // readTextAsset 读取一个列在清单 `required_files` 中的 UTF-8 包内文件。
  readTextAsset: (path: string) => Promise<string>;
  // subscribe listens to one current-plugin invalidation topic and returns synchronous cleanup.
  // subscribe 监听一个当前插件失效主题，并返回同步清理函数。
  subscribe: (topic: string, listener: () => void) => () => void;
}
