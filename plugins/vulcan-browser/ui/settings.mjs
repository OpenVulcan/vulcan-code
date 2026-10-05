// TIMEOUT_FIELDS names only the three Playwright MCP timeout arguments declared by this package.
// TIMEOUT_FIELDS 仅列出此插件包声明的三个 Playwright MCP 超时参数。
const TIMEOUT_FIELDS = [
  ["actionTimeoutMs", "actionTimeoutLabel"],
  ["navigationTimeoutMs", "navigationTimeoutLabel"],
  ["settleTimeoutMs", "settleTimeoutLabel"],
];

// validSettings mirrors the package JSON Schema before an action reaches Core validation.
// validSettings 在 Action 到达 Core 校验前镜像插件包的 JSON Schema 约束。
// draft is the complete editable configuration; returns whether every field is valid.
// draft 是完整的可编辑配置；返回所有字段是否有效。
function validSettings(draft) {
  return draft !== null
    && /^[1-9][0-9]{2,5}$/.test(draft.actionTimeoutMs)
    && /^[1-9][0-9]{2,5}$/.test(draft.navigationTimeoutMs)
    && /^(0|[1-9][0-9]{0,5})$/.test(draft.settleTimeoutMs)
    && ["error", "warning", "info", "debug"].includes(draft.consoleLevel);
}

// createPluginPanel binds the browser settings form to this plugin's declared Core actions.
// createPluginPanel 将浏览器设置表单绑定到此插件声明的 Core Action。
// sdk provides the host React instance, translated text, components, and plugin-scoped invocation.
// sdk 提供宿主 React 实例、翻译文本、组件及插件范围的调用能力。
// Returns a React component mounted inside the plugin details page.
// 返回挂载于插件详情页的 React 组件。
export function createPluginPanel(sdk) {
  // React is provided by the host so the plugin never bundles a second renderer.
  // React 由宿主提供，插件不打包第二份渲染器。
  const React = sdk.React;
  const Button = sdk.components.Button;
  // StreamPlaceholder is the shared host loading silhouette shown while Core settings load.
  // StreamPlaceholder 是共享宿主加载轮廓，在读取 Core 设置期间显示。
  const StreamPlaceholder = sdk.components.StreamPlaceholder;

  // BrowserSettingsPanel owns only a local draft; Core remains the sole persistence authority.
  // BrowserSettingsPanel 仅持有本地草稿；Core 仍是唯一的持久化权威。
  // Returns the interactive settings form and current save or load status.
  // 返回交互式设置表单及当前保存或加载状态。
  return function BrowserSettingsPanel() {
    const [draft, setDraft] = React.useState(null);
    const [loading, setLoading] = React.useState(true);
    const [saving, setSaving] = React.useState(false);
    const [error, setError] = React.useState("");
    const [saved, setSaved] = React.useState(false);
    const mounted = React.useRef(false);
    const readSequence = React.useRef(0);
    const readRequest = React.useRef(null);
    const saveBusy = React.useRef(false);

    // load reads the complete current configuration, sharing one in-flight read across effect replay.
    // load 读取完整的当前配置，并在 Effect 重放期间共用一次在途读取。
    // Returns after the action settles; stale or unmounted responses cannot replace a newer draft.
    // 在 Action 结束后返回；过期或已卸载的响应不能替换新草稿。
    const load = React.useCallback(async () => {
      const sequence = ++readSequence.current;
      setLoading(true);
      setError("");
      try {
        if (readRequest.current === null) readRequest.current = sdk.invoke("settings-get", {});
        const request = readRequest.current;
        const current = await request.finally(() => {
          if (readRequest.current === request) readRequest.current = null;
        });
        if (!mounted.current || sequence !== readSequence.current) return;
        if (!validSettings(current)) throw new Error(sdk.t("settingsResponseInvalid", "Playwright settings response is invalid."));
        setDraft(current);
        setSaved(false);
      } catch (reason) {
        if (mounted.current && sequence === readSequence.current) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      } finally {
        if (mounted.current && sequence === readSequence.current) setLoading(false);
      }
    }, []);

    React.useEffect(() => {
      mounted.current = true;
      void load();
      return () => {
        mounted.current = false;
        readSequence.current += 1;
      };
    }, [load]);

    // save submits one complete draft and keeps rejected edits visible for an explicit retry.
    // save 提交一份完整草稿，并在拒绝后保留可见编辑供显式重试。
    // Returns after Core either commits or rejects the configuration.
    // 在 Core 提交或拒绝配置后返回。
    async function save() {
      if (saveBusy.current || !validSettings(draft)) return;
      saveBusy.current = true;
      setSaving(true);
      setError("");
      setSaved(false);
      try {
        const persisted = await sdk.invoke("settings-update", draft);
        if (!mounted.current) return;
        if (!validSettings(persisted)) throw new Error(sdk.t("settingsPersistedInvalid", "Persisted Playwright settings are invalid."));
        setDraft(persisted);
        setSaved(true);
      } catch (reason) {
        if (mounted.current) setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        saveBusy.current = false;
        if (mounted.current) setSaving(false);
      }
    }

    // field renders one accessible timeout input without converting its string-valued contract.
    // field 渲染一个可访问的超时输入，不转换其字符串值契约。
    // key and labelKey select the package schema field and translation; returns a form label.
    // key 与 labelKey 选择插件 Schema 字段和翻译；返回表单标签。
    function field(key, labelKey) {
      return React.createElement("label", { className: "flex flex-col gap-1 text-sm", key },
        React.createElement("span", null, sdk.t(labelKey, key)),
        React.createElement("input", {
          "aria-label": sdk.t(labelKey, key),
          className: "w-full rounded border bg-background px-2 py-1",
          disabled: saving,
          inputMode: "numeric",
          onChange: (event) => {
            setDraft((current) => ({ ...current, [key]: event.target.value }));
            setSaved(false);
          },
          type: "text",
          value: draft[key],
        }));
    }

    if (loading && draft === null) return React.createElement(StreamPlaceholder, { className: "p-3", label: sdk.t("settingsLabel", "Playwright Settings"), shape: "fields", count: 5 });
    if (draft === null) return React.createElement("div", { className: "flex flex-col gap-2 p-3" },
      React.createElement("p", { role: "alert" }, error),
      React.createElement(Button, { onClick: () => void load(), type: "button" }, sdk.t("settingsRetry", "Retry loading")));
    return React.createElement("div", { className: "flex flex-col gap-3 p-3" },
      React.createElement("h3", { className: "font-medium" }, sdk.t("settingsLabel", "Playwright Settings")),
      React.createElement("p", { className: "text-sm text-muted-foreground" },
        sdk.t("settingsHint", "These settings apply when a new AI browser connection starts.")),
      ...TIMEOUT_FIELDS.map(([key, labelKey]) => field(key, labelKey)),
      React.createElement("label", { className: "flex flex-col gap-1 text-sm" },
        React.createElement("span", null, sdk.t("consoleLevelLabel", "Console level")),
        React.createElement("select", {
          "aria-label": sdk.t("consoleLevelLabel", "Console level"),
          className: "w-full rounded border bg-background px-2 py-1",
          disabled: saving,
          onChange: (event) => {
            setDraft((current) => ({ ...current, consoleLevel: event.target.value }));
            setSaved(false);
          },
          value: draft.consoleLevel,
        }, ...["error", "warning", "info", "debug"].map((level) =>
          React.createElement("option", { key: level, value: level }, sdk.t(`consoleLevel_${level}`, level))))),
      !validSettings(draft) ? React.createElement("p", { role: "alert" },
        sdk.t("settingsInvalid", "Enter valid timeouts before saving.")) : null,
      error ? React.createElement("p", { role: "alert" }, error) : null,
      saved ? React.createElement("p", { role: "status" },
        sdk.t("settingsSaved", "Settings saved. New browser connections use the updated values.")) : null,
      React.createElement(Button, {
        disabled: saving || loading || !validSettings(draft),
        onClick: () => void save(),
        type: "button",
      }, sdk.t("settingsSave", "Save settings")));
  };
}
