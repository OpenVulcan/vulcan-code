import * as React from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { PLUGIN_UI_SDK_COMPONENTS, type PluginUiSdkComponents } from "./plugin-ui-sdk-components";
import { useI18n } from "@/i18n";
import { createPluginSettingsPage } from "./plugin-settings-page";
import { parsePluginSettingsLayout } from "./plugin-settings-layout";
import { invalidatePersistentPluginUiModules, loadPersistentPluginUiModule } from "@/service/plugin-ui-module-runtime";
import { createPluginQueryClient, type PluginQueryOptions } from "@/service/plugin-ui-query-runtime";
import { BROWSER_SERVICE_CONNECTION_EVENT, type BrowserServiceConnectionDetail } from "@/service/browser-service-discovery";
import {
  invokePluginAction,
  loadPluginUiModule,
  readPluginTextAsset,
} from "@/service/skills-actions-service";
import {
  openExternalUrl,
} from "@/service/project-actions-service";
import {
  latestPluginUiInvalidationRevision,
  subscribePluginUiInvalidation,
} from "@/service/plugin-ui-events";
import type { PluginSummary } from "@/types";
import { detectShellHost, type ShellHostMarker } from "@/host-shell/runtime";

// PluginUiSdk is the stable host context exposed to one authorized dynamic plugin module.
// PluginUiSdk 是暴露给单个已授权动态插件模块的稳定宿主上下文。
interface PluginUiSdk {
  // Host React instance keeps plugin components inside the current renderer.
  // 宿主 React 实例确保插件组件留在当前渲染器内。
  React: typeof React;
  // Host-selected route and actual renderer host identify the UI context without plugin inference.
  // 宿主选中的路由与实际渲染器宿主标识界面上下文，无须插件自行猜测。
  context: {
    // The persisted VulcanCode conversation selected for this mount, when present.
    // 此挂载项选中的已持久化 VulcanCode 会话，如存在。
    conversationId: string | null;
    // The actual host type detected from the current renderer bridge.
    // 从当前渲染器桥接检测到的实际宿主类型。
    clientHost: ShellHostMarker;
  };
  // Invokes one action declared by the owning plugin manifest.
  // 调用所属插件清单声明的单个 Action。
  invoke: <Result>(actionId: string, argumentsValue: unknown, options?: PluginQueryOptions) => Promise<Result>;
  // Reads one exact UTF-8 text asset declared in the owning plugin manifest.
  // 读取所属插件清单中声明的一个精确 UTF-8 文本资源。
  readTextAsset: (path: string) => Promise<string>;
  // Opens one host-authorized external URL through the active shell.
  // 通过当前宿主壳打开一个经宿主授权的外部地址。
  openExternalUrl: (url: string) => Promise<void>;
  // Active Vulcan WebUI locale inherited by plugin content.
  // 插件内容继承的当前 Vulcan WebUI 语言。
  locale: string;
  // Stable host components preserve the active design system and theme.
  // 稳定宿主组件保持当前设计系统与主题。
  components: PluginUiSdkComponents;
  // Revision-only event utilities keep plugin UI data synchronized without carrying business payloads.
  // 仅含修订号的事件工具在不携带业务载荷的前提下同步插件界面数据。
  events: {
    // Subscribes to one current-plugin invalidation topic and returns synchronous cleanup.
    // 订阅一个当前插件失效主题并返回同步清理函数。
    subscribe: (topic: string, listener: (revision: number) => void) => () => void;
    // Returns the highest revision already observed for one current-plugin topic.
    // 返回一个当前插件主题已观测到的最高修订号。
    latestRevision: (topic: string) => number;
  };
  // Formats one numeric value through the active host locale.
  // 使用当前宿主语言格式化单个数值。
  formatNumber: (value: number) => string;
  // Host notifications let plugins report bounded user-initiated action results.
  // 宿主通知允许插件报告有边界的用户发起操作结果。
  notifications: {
    // Shows one successful operation toast.
    // 显示一条操作成功 Toast。
    success: (message: string) => void;
    // Shows one failed operation toast.
    // 显示一条操作失败 Toast。
    error: (message: string) => void;
  };
  // Resolves one plugin-owned localized message with an explicit source fallback.
  // 使用明确的源码回退解析一条插件自有本地化文案。
  t: (key: string, fallback: string) => string;
}

// PluginUiFactory creates one React component from the stable host SDK.
// PluginUiFactory 使用稳定宿主 SDK 创建一个 React 组件。
type PluginUiFactory = (sdk: PluginUiSdk) => React.ComponentType;

// localizedPluginMessage resolves exact-locale, same-language, and English plugin messages in order.
// localizedPluginMessage 依次解析完整语言、同语言与英语插件文案。
//
// Parameters: messagesByLocale is the validated plugin locale bundle, locale is the host locale, and key is the stable message id.
// 参数：messagesByLocale 是已校验插件语言包，locale 是宿主语言，key 是稳定消息标识。
// Returns: the first non-empty matching message or null when the plugin did not declare it.
// 返回值：返回首个非空匹配文案；插件未声明时返回 null。
function localizedPluginMessage(
  messagesByLocale: Record<string, Record<string, string>>,
  locale: string,
  key: string,
): string | null {
  const language = locale.split("-")[0];
  const candidates = [
    messagesByLocale[locale],
    Object.entries(messagesByLocale).find(([candidate]) => candidate === language || candidate.startsWith(`${language}-`))?.[1],
    messagesByLocale.en,
  ];
  for (const messages of candidates) {
    const message = messages?.[key]?.trim();
    if (message) {
      return message;
    }
  }
  return null;
}

// DynamicPluginMountPanelProps binds one service-authorized UI contribution to its host context.
// DynamicPluginMountPanelProps 把一个经服务授权的界面贡献绑定到宿主上下文。
interface DynamicPluginMountPanelProps {
  // Exact enabled plugin owning the mount.
  // 拥有挂载项的精确已启用插件。
  plugin: PluginSummary;
  // Exact mount selected from the service projection.
  // 从服务投影选择的精确挂载项。
  mount: NonNullable<PluginSummary["extension"]>["uiMounts"][number];
  // Optional trusted workspace context forwarded only to manifest-declared plugin actions.
  // 仅转发给清单声明插件 Action 的可选可信工作区上下文。
  workspacePath?: string | null;
  // Current persisted conversation selected by the host for conversation-scoped UI.
  // 宿主为会话范围界面选中的当前已持久化会话。
  conversationId?: string | null;
}

// DynamicPluginMountPanel loads and renders one authenticated uncompressed plugin ESM mount.
// DynamicPluginMountPanel 加载并渲染一个经过认证的未压缩插件 ESM 挂载项。
export function DynamicPluginMountPanel(props: DynamicPluginMountPanelProps) {
  // locale lets the mounted module inherit the active Vulcan language and number formatting.
  // locale 使挂载模块继承当前 Vulcan 语言与数字格式。
  const { locale, t } = useI18n();
  // Panel stores the component returned by the authenticated module factory.
  // Panel 保存经过认证的模块工厂返回的组件。
  const [Panel, setPanel] = React.useState<React.ComponentType | null>(null);
  // error stores bounded module-load or Action diagnostics.
  // error 保存有边界的模块加载或 Action 诊断。
  const [error, setError] = React.useState("");
  // retryRevision represents an explicit reload of the authenticated module after a failure.
  // retryRevision 表示失败后显式重新加载已认证模块。
  const [retryRevision, setRetryRevision] = React.useState(0);
  // Reopen the module boundary after transport recovery, even when the selected plugin is unchanged.
  // 传输恢复后重新打开模块边界，即使所选插件没有改变。
  React.useEffect(() => {
    let interrupted = false;
    const listener = (event: Event) => {
      const status = (event as CustomEvent<BrowserServiceConnectionDetail>).detail.status;
      if (status !== "connected") interrupted = true;
      else if (interrupted) { interrupted = false; setRetryRevision(revision => revision + 1); }
    };
    window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, listener);
    return () => window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, listener);
  }, []);
  // sdk binds every action invocation to the selected plugin identity and host context.
  // sdk 把每次 Action 调用绑定到所选插件身份与宿主上下文。
  const sdk = React.useMemo<Omit<PluginUiSdk, "t" | "invoke">>(() => ({
    React,
    context: {
      conversationId: props.conversationId ?? null,
      clientHost: detectShellHost(),
    },
    readTextAsset: async (path: string) => {
      // asset is authenticated against the current plugin identity and manifest allowlist.
      // asset 会根据当前插件身份与清单允许列表进行认证。
      const asset = await readPluginTextAsset(props.plugin.id, path);
      return asset.content;
    },
    openExternalUrl,
    locale,
    components: PLUGIN_UI_SDK_COMPONENTS,
    events: {
      subscribe: (topic, listener) => subscribePluginUiInvalidation(props.plugin.id, topic, listener),
      latestRevision: (topic) => latestPluginUiInvalidationRevision(props.plugin.id, topic),
    },
    formatNumber: (value: number) => new Intl.NumberFormat(locale).format(value),
    notifications: {
      success: (message: string) => toast.success(message),
      error: (message: string) => toast.error(message),
    },
  }), [locale, props.conversationId, props.plugin.id, props.workspacePath]);

  React.useEffect(() => {
    // cancelled prevents a stale module from replacing a newer plugin selection.
    // cancelled 防止旧模块覆盖更新后的插件选择。
    let cancelled = false;
    // Subscription cleanup leaves shared backend work alive for later mounts.
    // 订阅清理保留共享后端任务，供之后的挂载复用。
    let disposeQueries: (() => void) | undefined;
    setPanel(null);
    setError("");
    const extension = props.plugin.extension;
    if (!extension) { setError("Plugin UI requires a manifest-owned extension."); return; }
    // Load plugin-only styles at the actual mount boundary and wait before exposing any plugin component.
    // 在实际挂载边界加载插件专用样式，并等待完成后再展示任何插件组件。
    // Each retry creates a fresh import request rather than retaining a rejected application-level promise.
    // 每次重试重新发起导入请求，不保留应用层已拒绝 Promise。
    void Promise.all([
      loadPersistentPluginUiModule(props.plugin.id, props.mount.id, extension.runtimeIdentity, loadPluginUiModule),
      import("@/styles/plugin-host-components.css"),
    ]).then(([{ namespace: moduleNamespace, source: moduleSource }]) => {
        if (cancelled) {
          return;
        }
        if (moduleSource.pluginId !== props.plugin.id || moduleSource.mountId !== props.mount.id) {
          throw new Error("Plugin UI module identity did not match the requested mount.");
        }
        // moduleSdk binds translations from the same authenticated package generation as the loaded UI source.
        // moduleSdk 把与已加载界面源码同一认证包代次的翻译绑定到 SDK。
        const moduleSdk: PluginUiSdk = {
          ...sdk,
          notifications: {
            success: message => { if (!cancelled) sdk.notifications.success(message); },
            error: message => { if (!cancelled) sdk.notifications.error(message); },
          },
          invoke: (() => {
            const client = createPluginQueryClient({ pluginId: props.plugin.id, runtimeIdentity: moduleSource.runtimeIdentity,
              workspacePath: props.workspacePath ?? null, conversationId: props.conversationId ?? null }, moduleSource.actionPolicies,
              (actionId, argumentsValue, cancellation) => invokePluginAction(props.plugin.id, actionId, argumentsValue, moduleSource.runtimeIdentity, props.workspacePath, cancellation));
            disposeQueries = client.dispose;
            return client.invoke;
          })(),
          t: (key, fallback) => localizedPluginMessage(moduleSource.messagesByLocale, locale, key) ?? fallback,
        };
        // Settings layouts render through the host page kit; only modules run a plugin factory.
        // 设置版面经宿主页面套件渲染；只有模块才运行插件工厂。
        if (moduleSource.kind === "settings") {
          const settings = createPluginSettingsPage(parsePluginSettingsLayout(moduleSource.source), moduleSource.configSchema,
            { invoke: moduleSdk.invoke, message: key => moduleSdk.t(key, key), openExternalUrl: moduleSdk.openExternalUrl,
              readTextAsset: moduleSdk.readTextAsset, subscribe: (topic, listener) => moduleSdk.events.subscribe(topic, () => listener()) });
          if (!cancelled) setPanel(() => settings);
          return;
        }
        // factory is the exact manifest-declared export, used only after source identity is verified.
        // factory 是精确的清单声明导出，仅在源码身份校验后使用。
        const factory = moduleSource.exportName === undefined ? undefined : moduleNamespace[moduleSource.exportName];
        if (typeof factory !== "function") {
          throw new Error(`Plugin UI export \`${moduleSource.exportName}\` is not a function.`);
        }
        const component = (factory as PluginUiFactory)(moduleSdk);
        if (typeof component !== "function") {
          throw new Error("Plugin UI factory did not return a React component.");
        }
        if (!cancelled) {
          setPanel(() => component);
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      });
    return () => {
      cancelled = true;
      disposeQueries?.();
    };
  }, [locale, props.mount.id, props.plugin.id, props.plugin.extension?.runtimeIdentity, props.workspacePath, props.conversationId, sdk, retryRevision]);

  if (error) {
    return <div className="space-y-2 rounded-md border border-border bg-card p-4 text-sm text-foreground" role="alert">
      <p>{t("plugins.mount.unableToLoadPlugin")}</p>
      <Button size="sm" variant="outline" onClick={() => {
        invalidatePersistentPluginUiModules(props.plugin.id);
        setRetryRevision((revision) => revision + 1);
      }}>{t("plugins.mount.retry")}</Button>
      <details><summary>{t("plugins.mount.errorDetails")}</summary>
        <pre className="max-w-full whitespace-pre-wrap break-words text-xs text-muted-foreground">{error}</pre>
      </details>
    </div>;
  }
  if (Panel === null) {
    return <StreamPlaceholder fill className="w-full min-w-0 p-4" shape="rows" label={t("skills.plugins.loadingPage")} />;
  }
  return <Panel />;
}
