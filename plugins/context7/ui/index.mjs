// CONTEXT7_DASHBOARD_URL is the fixed official account and API-key management entry.
// CONTEXT7_DASHBOARD_URL 是固定的官方账户与 API Key 管理入口。
const CONTEXT7_DASHBOARD_URL = "https://context7.com/dashboard";

// CONTEXT7_API_KEY_DOCS_URL is the fixed official API-key instruction page.
// CONTEXT7_API_KEY_DOCS_URL 是固定的官方 API Key 说明页面。
const CONTEXT7_API_KEY_DOCS_URL = "https://context7.com/docs/howto/api-keys";

// CONTEXT7_USAGE_DOCS_URL is the fixed official usage explanation page.
// CONTEXT7_USAGE_DOCS_URL 是固定的官方用量说明页面。
const CONTEXT7_USAGE_DOCS_URL = "https://context7.com/docs/howto/usage";

// RANGE_OPTIONS lists every bounded local statistics window accepted by the plugin action schema.
// RANGE_OPTIONS 列出插件 Action Schema 接受的全部有界本地统计时间窗。
const RANGE_OPTIONS = [7, 30, 90, 365];

// TIMEOUT_OPTIONS lists every concise timeout choice offered by the mounted settings interface.
// TIMEOUT_OPTIONS 列出挂载设置界面提供的全部简洁超时选项。
const TIMEOUT_OPTIONS = [10000, 30000, 60000];

// RETENTION_OPTIONS lists every local event retention period accepted by the configuration schema.
// RETENTION_OPTIONS 列出配置 Schema 接受的全部本地事件保留期。
const RETENTION_OPTIONS = [30, 90, 180, 365];

// PANEL_STYLE keeps the uncompiled plugin UI theme-aware without adding a package-owned stylesheet.
// PANEL_STYLE 让未编译插件界面保持主题感知，同时不增加插件自有样式表。
const PANEL_STYLE = {
  display: "grid",
  gap: "12px",
  padding: "4px 0 20px",
};

// CARD_STYLE uses only inherited design tokens so light and dark themes remain readable.
// CARD_STYLE 仅使用继承的设计 Token，确保浅色与深色主题均可读。
const CARD_STYLE = {
  border: "1px solid var(--border)",
  borderRadius: "var(--radius)",
  background: "var(--card)",
  color: "var(--card-foreground)",
  display: "grid",
  gap: "10px",
  padding: "14px",
};

// ROW_STYLE aligns bounded settings controls while allowing narrow layouts to wrap.
// ROW_STYLE 对齐有界设置控件，并允许窄布局自动换行。
const ROW_STYLE = {
  alignItems: "center",
  display: "flex",
  flexWrap: "wrap",
  gap: "8px",
};

// INPUT_STYLE renders a native secret input through inherited host colors.
// INPUT_STYLE 使用继承的宿主颜色渲染原生密钥输入框。
const INPUT_STYLE = {
  background: "var(--background)",
  border: "1px solid var(--input)",
  borderRadius: "8px",
  color: "var(--foreground)",
  flex: "1 1 260px",
  minHeight: "36px",
  minWidth: "180px",
  padding: "0 10px",
};

// MUTED_STYLE renders secondary explanations with the active theme's muted foreground.
// MUTED_STYLE 使用当前主题的弱化前景色渲染次要说明。
const MUTED_STYLE = { color: "var(--muted-foreground)", fontSize: "12px", lineHeight: 1.55 };

// GRID_STYLE creates responsive metric columns without relying on build-time utility discovery.
// GRID_STYLE 创建响应式指标列，不依赖构建期工具类发现。
const GRID_STYLE = { display: "grid", gap: "10px", gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))" };

// LINK_STYLE renders host-mediated external navigation as conventional underlined text links.
// LINK_STYLE 将宿主管理的外部导航渲染为常规带下划线文字链接。
const LINK_STYLE = {
  color: "var(--primary)",
  cursor: "pointer",
  fontSize: "13px",
  textDecoration: "underline",
  textUnderlineOffset: "3px",
};

// Create the Context7 settings and usage panel through the stable host UI SDK.
// 通过稳定宿主 UI SDK 创建 Context7 设置与用量面板。
//
// sdk supplies the host React runtime, components, localization, actions, events, and notifications.
// sdk 提供宿主 React 运行时、组件、本地化、Action、事件与通知。
// Returns one React component integrated into the Vulcan Plugin detail page.
// 返回一个集成到 Vulcan Plugin 详情页的 React 组件。
export function createPluginPanel(sdk) {
  // React is the exact host renderer and must never be bundled by this plugin.
  // React 是精确宿主渲染器，当前插件绝不能自行打包。
  const React = sdk.React;
  // h is the host React element factory used by this uncompiled ESM module.
  // h 是当前未编译 ESM 模块使用的宿主 React 元素工厂。
  const h = React.createElement;
  // Badge is the shared host badge component.
  // Badge 是共享宿主徽标组件。
  const Badge = sdk.components.Badge;
  // Button is the shared host action component.
  // Button 是共享宿主操作组件。
  const Button = sdk.components.Button;
  // MetricCard is the shared host KPI component.
  // MetricCard 是共享宿主 KPI 组件。
  const MetricCard = sdk.components.MetricCard;
  // SelectControl is the shared host-themed select component.
  // SelectControl 是共享宿主主题下拉组件。
  const SelectControl = sdk.components.SelectControl;
  // EmptyState is the shared host empty presentation.
  // EmptyState 是共享宿主空状态展示组件。
  const EmptyState = sdk.components.EmptyState;
  // text resolves one plugin locale key with an explicit English source fallback.
  // text 使用明确的英语源码回退解析一个插件语言键。
  const text = (key, fallback) => sdk.t(key, fallback);

  // interpolate replaces exact named placeholders without evaluating templates.
  // interpolate 替换精确命名的占位符，不执行模板。
  //
  // value is one localized string and fields contains literal replacement values.
  // value 是一条本地化字符串，fields 包含字面量替换值。
  // Returns the deterministically interpolated user-facing string.
  // 返回确定性插值后的用户可见字符串。
  function interpolate(value, fields) {
    // output accumulates literal replacements in stable property order.
    // output 按稳定属性顺序累积字面量替换。
    let output = value;
    for (const [key, fieldValue] of Object.entries(fields)) {
      output = output.replaceAll(`{{${key}}}`, String(fieldValue));
    }
    return output;
  }

  // formatQuotaResetDate renders one compact locale-specific calendar date without time text.
  // formatQuotaResetDate 渲染一个不含时间文本的紧凑区域化日历日期。
  //
  // unixSeconds is the exact Context7 quota reset Unix timestamp in seconds.
  // unixSeconds 是 Context7 配额重置时间的精确 Unix 秒时间戳。
  // Returns YYYY-MM-DD for Chinese or a standard abbreviated English date.
  // 中文返回 YYYY-MM-DD，英文返回标准缩写日期。
  function formatQuotaResetDate(unixSeconds) {
    // date is interpreted in the user's local timezone, matching the previous UI behavior.
    // date 使用用户本地时区解释，与之前界面行为保持一致。
    const date = new Date(unixSeconds * 1000);
    if (sdk.locale.toLowerCase().startsWith("zh")) {
      // pad ensures every Chinese numeric month and day occupies exactly two digits.
      // pad 确保中文数字月份与日期始终占用两位。
      const pad = (value) => String(value).padStart(2, "0");
      return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
    }
    return new Intl.DateTimeFormat("en-US", { day: "numeric", month: "short", year: "numeric" }).format(date);
  }

  // Context7PluginPanel owns only non-secret rendered state; saved keys are never returned by actions.
  // Context7PluginPanel 仅持有非密钥渲染状态；Action 绝不会返回已保存 Key。
  //
  // Returns the complete mounted settings and usage interface.
  // 返回完整的挂载设置与用量界面。
  function Context7PluginPanel() {
    // config is the complete schema-validated non-secret plugin configuration.
    // config 是经过 Schema 校验的完整非密钥插件配置。
    const [config, setConfig] = React.useState(null);
    // savedConfig and configDirty retain unsaved user settings across background usage refreshes.
    // savedConfig 和 configDirty 在后台用量刷新期间保留用户未保存的设置。
    const savedConfig = React.useRef(null);
    const configDirty = React.useRef(false);
    // credential is the non-secret status projection returned by the plugin action.
    // credential 是插件 Action 返回的非密钥状态投影。
    const [credential, setCredential] = React.useState(null);
    // usage contains current-generation quota and local aggregate statistics.
    // usage 包含当前代次配额与本地聚合统计。
    const [usage, setUsage] = React.useState(null);
    // rangeDays is the selected bounded local statistics window.
    // rangeDays 是选定的有界本地统计时间窗。
    const [rangeDays, setRangeDays] = React.useState(30);
    // apiKey contains only the current unsaved password-field input.
    // apiKey 只包含当前尚未保存的密码输入框内容。
    const [apiKey, setApiKey] = React.useState("");
    // loading indicates one complete settings and usage refresh.
    // loading 表示一次完整设置与用量刷新正在进行。
    const [loading, setLoading] = React.useState(true);
    // saving serializes user-initiated mutations and connection tests.
    // saving 串行化用户发起的变更与连接测试。
    const [saving, setSaving] = React.useState(false);
    // mutationPending closes the same-tick gap before React renders disabled buttons.
    // mutationPending 关闭 React 渲染禁用按钮前同一轮事件中的重复提交窗口。
    const mutationPending = React.useRef(false);
    // deleteArmed requires a deliberate second click before credential deletion.
    // deleteArmed 要求再次明确点击后才删除凭据。
    const [deleteArmed, setDeleteArmed] = React.useState(false);
    // error is the bounded host diagnostic shown without secret data.
    // error 是不含密钥数据的有界宿主诊断。
    const [error, setError] = React.useState(null);
    // loadSequence identifies the latest complete refresh so stale concurrent responses cannot overwrite it.
    // loadSequence 标识最新一次完整刷新，防止过期并发响应覆盖它。
    const loadSequence = React.useRef(0);
    // initialLoadStarted prevents React StrictMode from submitting the initial bootstrap twice.
    // initialLoadStarted 防止 React StrictMode 重复提交首次 Bootstrap。
    const initialLoadStarted = React.useRef(false);
    // loadedRangeDays tracks the statistics range represented by the latest automatic usage request.
    // loadedRangeDays 跟踪最近一次自动用量请求所代表的统计范围。
    const loadedRangeDays = React.useRef(rangeDays);

    // loadAll refreshes configuration, credential status, and usage through declared actions.
    // loadAll 通过已声明 Action 刷新配置、凭据状态与用量。
    //
    // notifyFailure controls whether a user-visible toast accompanies the inline error.
    // notifyFailure 控制是否在行内错误之外显示用户可见 Toast。
    // Returns a promise that resolves after the single bootstrap projection is synchronized.
    // 返回一个在单次 Bootstrap 投影完成同步后解决的 Promise。
    const loadAll = React.useCallback(async (notifyFailure = false) => {
      // requestSequence is the immutable identity of this refresh attempt.
      // requestSequence 是当前刷新尝试的不可变身份。
      const requestSequence = loadSequence.current + 1;
      loadSequence.current = requestSequence;
      setLoading(true);
      setError(null);
      try {
        // result is the complete schema-validated page projection from one serialized plugin action.
        // result 是通过单次串行插件 Action 返回的完整且经 Schema 校验的页面投影。
        const result = await sdk.invoke("bootstrap-get", { rangeDays });
        if (requestSequence !== loadSequence.current) return;
        savedConfig.current = result.settings;
        if (!configDirty.current) setConfig(result.settings);
        setCredential(result.credential);
        setUsage(result.usage);
        setError(result.usageError);
      } catch (loadError) {
        if (requestSequence !== loadSequence.current) return;
        // message preserves only the host-rendered bounded diagnostic.
        // message 仅保留宿主渲染的有界诊断。
        const message = loadError instanceof Error ? loadError.message : String(loadError);
        setError(message);
        if (notifyFailure) sdk.notifications.error(text("loadError", "Context7 data could not be loaded."));
      } finally {
        if (requestSequence === loadSequence.current) setLoading(false);
      }
    }, [rangeDays]);

    React.useEffect(() => {
      if (initialLoadStarted.current) return;
      initialLoadStarted.current = true;
      void loadAll(false);
    }, [loadAll]);

    // loadUsage refreshes only the range-dependent projection after the initial bootstrap.
    // loadUsage 仅在首次 Bootstrap 后刷新依赖统计范围的投影。
    const loadUsage = React.useCallback(async (selectedRangeDays) => {
      // requestSequence prevents a slower earlier range query from replacing newer data.
      // requestSequence 防止较慢的旧范围查询覆盖更新数据。
      const requestSequence = loadSequence.current + 1;
      loadSequence.current = requestSequence;
      setLoading(true);
      setError(null);
      try {
        const result = await sdk.invoke("usage-get", { rangeDays: selectedRangeDays });
        if (requestSequence !== loadSequence.current) return;
        setCredential(result.credential);
        setUsage(result);
      } catch (loadError) {
        if (requestSequence !== loadSequence.current) return;
        const message = loadError instanceof Error ? loadError.message : String(loadError);
        setError(message);
      } finally {
        if (requestSequence === loadSequence.current) setLoading(false);
      }
    }, []);

    React.useEffect(() => {
      if (config === null || loadedRangeDays.current === rangeDays) return;
      loadedRangeDays.current = rangeDays;
      void loadUsage(rangeDays);
    }, [config, loadUsage, rangeDays]);

    React.useEffect(() => sdk.events.subscribe("context7.data", () => void loadAll(false)), [loadAll]);

    // runMutation serializes one user mutation, refreshes projections, and reports its result.
    // runMutation 串行执行一次用户变更、刷新投影并报告结果。
    //
    // operation performs the declared plugin action and successMessage is the localized toast text.
    // operation 执行已声明插件 Action，successMessage 是本地化 Toast 文案。
    // settingsMutation clears the dirty flag only after an acknowledged settings save; returns success or failure.
    // settingsMutation 仅在设置保存确认后清除草稿标记；返回成功或失败。
    async function runMutation(operation, successMessage, settingsMutation = false) {
      if (mutationPending.current) return false;
      mutationPending.current = true;
      setSaving(true);
      setError(null);
      try {
        await operation();
        if (settingsMutation) configDirty.current = false;
        sdk.notifications.success(successMessage);
        await loadAll(false);
        return true;
      } catch (mutationError) {
        // message is the exact bounded host action failure.
        // message 是宿主 Action 返回的精确有界失败信息。
        const message = mutationError instanceof Error ? mutationError.message : String(mutationError);
        setError(message);
        sdk.notifications.error(text("operationFailed", "The operation failed. Please retry."));
        return false;
      } finally {
        setSaving(false);
        mutationPending.current = false;
      }
    }

    // saveCredential persists only the current password input and immediately clears component state.
    // saveCredential 仅持久化当前密码输入，并立即清空组件状态。
    //
    // Returns a promise that resolves after the credential action completes.
    // 返回一个在凭据 Action 完成后解决的 Promise。
    async function saveCredential() {
      if (mutationPending.current || !credential || apiKey.length === 0) return;
      // submittedKey snapshots the secret so React state can be cleared before the network refresh.
      // submittedKey 快照密钥，使 React 状态可在网络刷新前清空。
      const submittedKey = apiKey;
      setApiKey("");
      await runMutation(
        () => sdk.invoke("credential-set", { apiKey: submittedKey }),
        text("apiKeySaved", "Context7 API key saved."),
      );
    }

    // deleteCredential requires a second click and then switches the plugin to anonymous mode.
    // deleteCredential 要求二次点击确认，随后把插件切换为匿名模式。
    //
    // Returns a promise that resolves after deletion or immediately after arming confirmation.
    // 返回一个在删除完成后或确认状态启用后立即解决的 Promise。
    async function deleteCredential() {
      if (mutationPending.current || !credential?.configured) return;
      if (!deleteArmed) {
        setDeleteArmed(true);
        return;
      }
      setDeleteArmed(false);
      await runMutation(
        () => sdk.invoke("credential-delete", {}),
        text("apiKeyDeleted", "Context7 API key deleted; anonymous mode is active."),
      );
    }

    // testConnection performs the explicit quota-consuming probe through the current auth mode.
    // testConnection 通过当前鉴权模式执行明确消耗额度的探测。
    //
    // Returns a promise that resolves after the test and refresh finish.
    // 返回一个在测试与刷新完成后解决的 Promise。
    async function testConnection() {
      await runMutation(
        () => sdk.invoke("connection-test", {}),
        text("connectionSuccess", "Context7 connection succeeded."),
      );
    }

    // saveSettings persists the complete non-secret configuration object.
    // saveSettings 持久化完整的非密钥配置对象。
    //
    // Returns a promise that resolves after the configuration update completes.
    // 返回一个在配置更新完成后解决的 Promise。
    async function saveSettings() {
      if (!config || !configDirty.current) return;
      await runMutation(
        () => sdk.invoke("settings-update", config),
        text("settingsSaved", "Context7 settings saved."),
        true,
      );
    }

    // updateSettings changes only the three declared settings and tracks whether explicit saving is needed.
    // updateSettings 仅变更三个已声明设置，并跟踪是否需要明确保存。
    // nextConfig is the complete configuration schema; returns no value and never writes to storage.
    // nextConfig 是完整配置 Schema；无返回值且绝不写入存储。
    function updateSettings(nextConfig) {
      if (mutationPending.current) return;
      configDirty.current = savedConfig.current === null
        || nextConfig.defaultFastMode !== savedConfig.current.defaultFastMode
        || nextConfig.requestTimeoutMs !== savedConfig.current.requestTimeoutMs
        || nextConfig.retentionDays !== savedConfig.current.retentionDays;
      setConfig(nextConfig);
    }

    // openOfficial opens one source-defined Context7 URL through the host shell boundary.
    // openOfficial 通过宿主壳边界打开一个源码定义的 Context7 地址。
    //
    // url is one of the fixed official constants declared by this module.
    // url 是当前模块声明的固定官方常量之一。
    // Returns a promise that resolves after opening or reporting the bounded host error.
    // 返回一个在打开完成或报告有界宿主错误后解决的 Promise。
    async function openOfficial(url) {
      try {
        await sdk.openExternalUrl(url);
      } catch (openError) {
        // message is the exact bounded shell diagnostic shown both inline and as a toast.
        // message 是同时以内联和 Toast 形式展示的精确有界 Shell 诊断。
        const message = openError instanceof Error ? openError.message : String(openError);
        setError(message);
        sdk.notifications.error(message);
      }
    }

    // Render one conventional text link while routing navigation through the host security boundary.
    // 渲染一个常规文字链接，同时通过宿主安全边界执行导航。
    //
    // url is one fixed official Context7 target and label is its localized visible text.
    // url 是一个固定 Context7 官方目标，label 是其本地化可见文本。
    // Returns one accessible underlined anchor element.
    // 返回一个可访问的带下划线锚点元素。
    function officialLink(url, label) {
      return h("a", {
        href: url,
        onClick: (event) => {
          event.preventDefault();
          void openOfficial(url);
        },
        style: LINK_STYLE,
      }, label);
    }

    if (loading && !config) {
      return h("div", { style: PANEL_STYLE }, h(EmptyState, { title: text("loading", "Loading Context7 settings…") }));
    }

    // validationMessage maps the exact credential validation state to localized text.
    // validationMessage 把精确凭据验证状态映射为本地化文本。
    const validationMessage = credential?.validationStatus === "valid"
      ? text("credentialValid", "The configured API key was validated successfully.")
      : credential?.validationStatus === "invalid"
        ? text("credentialInvalid", "The configured API key was rejected by Context7.")
        : credential?.validationStatus === "untested"
          ? text("credentialUntested", "The configured API key has not been tested yet.")
          : text("credentialUnknown", "No API key validation result is available.");
    // quota is the latest snapshot for the current authentication generation only.
    // quota 是仅属于当前鉴权代次的最新快照。
    const quota = usage?.quotaSnapshot ?? null;
    // stats remains absent until a successful local aggregate has been received.
    // stats 在成功接收本地聚合前保持缺失。
    const stats = usage?.localStats ?? null;

    return h("section", { style: PANEL_STYLE },
      h("header", null,
        h("div", null,
          h("h2", { style: { fontSize: "18px", fontWeight: 650, margin: 0 } }, text("title", "Context7 documentation")),
          h("p", { style: { ...MUTED_STYLE, margin: "4px 0 0" } }, text("subtitle", "Configure authentication and inspect request quota and local usage.")),
        ),
      ),
      error ? h("div", { role: "alert", style: CARD_STYLE },
        h("div", { style: { ...ROW_STYLE, justifyContent: "space-between" } },
          h("strong", { style: { color: "var(--destructive)", fontSize: "13px" } }, text("loadError", "Context7 data could not be loaded.")),
          h(Button, { disabled: loading || saving, onClick: () => void loadAll(true), size: "sm", type: "button", variant: "outline" }, text("retry", "Retry")),
        ),
        h("details", { style: MUTED_STYLE },
          h("summary", { style: { cursor: "pointer" } }, text("errorDetails", "Error details")),
          h("pre", { style: { margin: "8px 0 0", maxHeight: "160px", overflow: "auto", whiteSpace: "pre-wrap", overflowWrap: "anywhere" } }, error)),
      ) : null,
      h("details", { style: { color: "var(--muted-foreground)", fontSize: "12px" } },
        h("summary", { style: { cursor: "pointer", padding: "4px 0" } }, text("aboutTitle", "What is Context7?")),
        h("div", { style: { display: "grid", gap: "8px", padding: "8px 0" } },
        h("p", { style: { margin: 0, lineHeight: 1.65 } }, text("aboutDescription", "Context7 retrieves current, version-aware library documentation for coding agents and supplies relevant excerpts directly to the active task.")),
        h("p", { style: { ...MUTED_STYLE, margin: 0, fontSize: "13px" } }, text("allowanceDescription", "Anonymous mode provides 200 requests. A free API key provides 1,000 requests and account usage visibility in the official dashboard.")),
        h("div", { style: ROW_STYLE },
          h("span", { style: MUTED_STYLE }, text("quickLinks", "Quick links")),
          officialLink(CONTEXT7_DASHBOARD_URL, text("dashboardLink", "Context7 dashboard")),
          officialLink(CONTEXT7_API_KEY_DOCS_URL, text("apiKeyDocsLink", "Get a free API key")),
          officialLink(CONTEXT7_USAGE_DOCS_URL, text("usageDocsLink", "API and usage documentation")),
        ),
        ),
      ),
      h("article", { style: CARD_STYLE },
        h("div", { style: ROW_STYLE },
          h("strong", null, text("credentialTitle", "Authentication")),
          h(Badge, { variant: credential?.configured ? "default" : "outline" }, !credential ? text("loading", "Loading…") : credential.configured ? text("apiKeyMode", "API key mode") : text("anonymousMode", "Anonymous mode")),
        ),
        credential ? h("p", { style: { ...MUTED_STYLE, margin: 0 } }, credential.configured ? `${text("configured", "API key configured")} · ${validationMessage}` : text("notConfigured", "No API key configured; requests use the anonymous allowance.")) : null,
        h("div", { style: ROW_STYLE },
          h("input", {
            "aria-label": text("apiKeyMode", "API key mode"),
            autoComplete: "off",
            disabled: saving || !credential,
            onChange: (event) => setApiKey(event.target.value),
            placeholder: text("apiKeyPlaceholder", "ctx7sk-…"),
            spellCheck: false,
            style: INPUT_STYLE,
            type: "password",
            value: apiKey,
          }),
          h(Button, { disabled: saving || !credential || apiKey.length === 0, onClick: () => void saveCredential(), size: "sm", type: "button" }, credential?.configured ? text("replaceApiKey", "Replace API key") : text("saveApiKey", "Save API key")),
          credential?.configured ? h(Button, { disabled: saving, onClick: () => void deleteCredential(), size: "sm", type: "button", variant: deleteArmed ? "destructive" : "outline" }, deleteArmed ? text("confirmDeleteApiKey", "Confirm deletion") : text("deleteApiKey", "Delete API key")) : null,
        ),
        h("div", { style: ROW_STYLE },
          h(Button, { disabled: saving || !credential, onClick: () => void testConnection(), size: "sm", type: "button", variant: "outline" }, text("connectionTest", "Test connection")),
          h("span", { style: MUTED_STYLE }, text("connectionTestHint", "Testing consumes one Context7 request from the current allowance.")),
        ),
      ),
      config ? h("details", { style: { border: "1px solid var(--border)", borderRadius: "var(--radius)", padding: "12px 14px", background: "var(--card)" } },
        h("summary", { style: { cursor: "pointer", fontWeight: 600 } }, text("behaviorTitle", "Request behavior")),
        h("div", { style: { display: "grid", gap: "10px", paddingTop: "12px" } },
        h("label", { style: ROW_STYLE },
          h("input", { checked: config.defaultFastMode, disabled: saving, onChange: (event) => updateSettings({ ...config, defaultFastMode: event.target.checked }), type: "checkbox" }),
          h("span", null, text("defaultFastMode", "Fast mode by default")),
        ),
        h("p", { style: { ...MUTED_STYLE, margin: 0 } }, text("defaultFastModeHint", "Skip Context7 LLM reranking for lower latency and potentially lower relevance.")),
        h("div", { style: ROW_STYLE },
          h("span", null, text("timeout", "Request timeout")),
          h(SelectControl, {
            ariaLabel: text("timeout", "Request timeout"),
            disabled: saving,
            onValueChange: (value) => updateSettings({ ...config, requestTimeoutMs: Number(value) }),
            options: TIMEOUT_OPTIONS.map((value) => ({ value: String(value), label: `${value / 1000} s` })),
            value: String(config.requestTimeoutMs),
          }),
          h("span", null, text("retention", "Local statistics retention")),
          h(SelectControl, {
            ariaLabel: text("retention", "Local statistics retention"),
            disabled: saving,
            onValueChange: (value) => updateSettings({ ...config, retentionDays: Number(value) }),
            options: RETENTION_OPTIONS.map((value) => ({ value: String(value), label: interpolate(text("days", "{{days}} days"), { days: value }) })),
            value: String(config.retentionDays),
          }),
          h(Button, { disabled: saving || !configDirty.current, onClick: () => void saveSettings(), size: "sm", type: "button" }, saving ? text("saving", "Saving…") : text("saveSettings", "Save settings")),
        ),
        ),
      ) : null,
      h("article", { style: CARD_STYLE },
        h("div", { style: ROW_STYLE },
          h("strong", null, text("usageTitle", "Usage")),
          h("span", { style: MUTED_STYLE }, text("usageRange", "Local range")),
          h(SelectControl, {
            ariaLabel: text("usageRange", "Local range"),
            disabled: loading,
            onValueChange: (value) => setRangeDays(Number(value)),
            options: RANGE_OPTIONS.map((value) => ({ value: String(value), label: interpolate(text("days", "{{days}} days"), { days: value }) })),
            value: String(rangeDays),
          }),
          h(Button, { disabled: loading || saving, onClick: () => void loadAll(true), size: "sm", type: "button", variant: "outline" }, text("refresh", "Refresh")),
        ),
        h("strong", { style: { fontSize: "13px" } }, text("localStatsTitle", "Local request statistics")),
        h("div", { style: GRID_STYLE },
          h(MetricCard, { label: text("requests", "Requests"), value: stats ? sdk.formatNumber(stats.requests) : "-" }),
          h(MetricCard, { label: text("successes", "Successful"), value: stats ? sdk.formatNumber(stats.successes) : "-" }),
          h(MetricCard, { label: text("failures", "Failed"), value: stats ? sdk.formatNumber(stats.failures) : "-" }),
          h(MetricCard, { label: text("averageDuration", "Average duration"), value: stats && stats.requests > 0 ? `${Math.round(stats.averageDurationMs)} ms` : "-" }),
        ),
        h("p", { style: { ...MUTED_STYLE, margin: 0 } }, text("localUsageNotice", "Local statistics include only requests made by this Vulcan Code plugin.")),
        h("strong", { style: { fontSize: "13px", marginTop: "4px" } }, text("quotaTitle", "Latest Context7 quota snapshot")),
        quota ? h("div", { style: GRID_STYLE },
          h(MetricCard, { label: text("quotaTier", "Tier"), value: quota.quotaTier ?? "—" }),
          h(MetricCard, { label: text("quotaRemaining", "Remaining"), value: typeof quota.quotaRemaining !== "number" ? "—" : sdk.formatNumber(quota.quotaRemaining) }),
          h(MetricCard, { label: text("quotaLimit", "Limit"), value: typeof quota.quotaLimit !== "number" ? "—" : sdk.formatNumber(quota.quotaLimit) }),
          h(MetricCard, { label: text("quotaReset", "Reset"), value: typeof quota.quotaResetUnixSec !== "number" ? "—" : formatQuotaResetDate(quota.quotaResetUnixSec) }),
        ) : h("span", { title: text("noQuota", "No quota response has been observed for the current authentication generation."), "aria-label": text("noQuota", "No quota response has been observed for the current authentication generation.") }, "—"),
        h("p", { style: { ...MUTED_STYLE, margin: 0 } }, text("officialUsageNotice", "Complete account usage, billing periods, seats, and cost remain available in the official Context7 dashboard.")),
      ),
    );
  }

  return Context7PluginPanel;
}
