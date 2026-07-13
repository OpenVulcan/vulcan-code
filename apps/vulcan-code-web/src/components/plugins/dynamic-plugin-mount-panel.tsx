import * as React from "react";
import { Badge } from "@/components/ui/badge";
import { StatusPill } from "@/components/shared/status";
import { useI18n } from "@/i18n";
import { loadPersistentPluginUiModule } from "@/service/plugin-ui-module-runtime";
import { invokePluginAction, loadPluginUiModule } from "@/service/skills-actions-service";
import type { PluginSummary } from "@/types";

// PluginUiSdk is the stable host context exposed to one authorized dynamic plugin module.
// PluginUiSdk 是暴露给单个已授权动态插件模块的稳定宿主上下文。
interface PluginUiSdk {
  // Host React instance keeps plugin components inside the current renderer.
  // 宿主 React 实例确保插件组件留在当前渲染器内。
  React: typeof React;
  // Invokes one action declared by the owning plugin manifest.
  // 调用所属插件清单声明的单个 Action。
  invoke: <Result>(actionId: string, argumentsValue: unknown) => Promise<Result>;
  // Active Vulcan WebUI locale inherited by plugin content.
  // 插件内容继承的当前 Vulcan WebUI 语言。
  locale: string;
  // Stable host components preserve the active design system and theme.
  // 稳定宿主组件保持当前设计系统与主题。
  components: {
    // Shared Vulcan badge component.
    // Vulcan 共享徽标组件。
    Badge: typeof Badge;
    // Shared Vulcan status component.
    // Vulcan 共享状态组件。
    StatusPill: typeof StatusPill;
  };
  // Formats one numeric value through the active host locale.
  // 使用当前宿主语言格式化单个数值。
  formatNumber: (value: number) => string;
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
  // sdk binds every action invocation to the selected plugin identity and host context.
  // sdk 把每次 Action 调用绑定到所选插件身份与宿主上下文。
  const sdk = React.useMemo<Omit<PluginUiSdk, "t">>(() => ({
    React,
    invoke: <Result,>(actionId: string, argumentsValue: unknown) => (
      invokePluginAction<Result>(props.plugin.id, actionId, argumentsValue, props.workspacePath)
    ),
    locale,
    components: { Badge, StatusPill },
    formatNumber: (value: number) => new Intl.NumberFormat(locale).format(value),
  }), [locale, props.plugin.id, props.workspacePath]);

  React.useEffect(() => {
    // cancelled prevents a stale module from replacing a newer plugin selection.
    // cancelled 防止旧模块覆盖更新后的插件选择。
    let cancelled = false;
    setPanel(null);
    setError("");
    void loadPersistentPluginUiModule(props.plugin.id, props.mount.id, loadPluginUiModule)
      .then(({ namespace: moduleNamespace, source: moduleSource }) => {
        if (cancelled) {
          return;
        }
        if (moduleSource.pluginId !== props.plugin.id || moduleSource.mountId !== props.mount.id) {
          throw new Error("Plugin UI module identity did not match the requested mount.");
        }
        // factory is the exact manifest-declared export selected by the service.
        // factory 是服务选择的精确清单声明导出。
        const factory = moduleNamespace[moduleSource.exportName];
        if (typeof factory !== "function") {
          throw new Error(`Plugin UI export \`${moduleSource.exportName}\` is not a function.`);
        }
        // component is created only after exact source identity and export shape are verified.
        // component 仅在精确源码身份与导出结构校验后创建。
        // moduleSdk binds translations from the same authenticated package generation as the loaded UI source.
        // moduleSdk 把与已加载界面源码同一认证包代次的翻译绑定到 SDK。
        const moduleSdk: PluginUiSdk = {
          ...sdk,
          t: (key, fallback) => localizedPluginMessage(moduleSource.messagesByLocale, locale, key) ?? fallback,
        };
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
    };
  }, [locale, props.mount.id, props.plugin.id, sdk]);

  if (error) {
    return <div className="skill-empty-state warning">{error}</div>;
  }
  if (Panel === null) {
    return <div className="skill-empty-state">{t("skills.plugins.loadingPage")}</div>;
  }
  return <Panel />;
}
