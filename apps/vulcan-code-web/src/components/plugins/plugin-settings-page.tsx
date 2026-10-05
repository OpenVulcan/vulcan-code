import * as React from "react";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { PageActionBar, PageBody, PageNotice, PageRow, PageSection, PageTabPanel, PageTabs, PageToolbar } from "@/components/page";
import { useI18n } from "@/i18n";
import { PluginDataSection } from "./plugin-data-section";
import { SettingsCredentialRow, SettingsFieldRow, SettingsInlineOption, type PluginMessage } from "./plugin-settings-controls";
import { PluginSettingsHeaderActions } from "./plugin-settings-header";
import type { PluginSettingsHost } from "./plugin-settings-host";
import {
  isFieldVisible, layoutFields, propertySchema, sameConfig, sectionFields, valueIssue,
  type PluginCredentialStatus, type PluginSettingsAction, type PluginSettingsCredential, type PluginSettingsLayout, type PluginSettingsSection,
} from "./plugin-settings-layout";

export type { PluginSettingsHost };

// RowNotice is the latest inline outcome of one credential or action row.
// RowNotice 是单个凭据或操作行的最新行内结果。
type RowNotice = { tone: "success" | "error"; text: string; details?: string };

// reasonText extracts a bounded diagnostic from a rejected action.
// reasonText 从被拒绝的 Action 中提取有界诊断。
function reasonText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

// createPluginSettingsPage binds one validated layout to a host page component, mirroring module factories.
// createPluginSettingsPage 将一个已校验版面绑定为宿主页面组件，与模块工厂形态一致。
export function createPluginSettingsPage(layout: PluginSettingsLayout, configSchema: unknown, host: PluginSettingsHost): React.ComponentType {
  return function PluginSettingsPage() {
    return <PluginSettingsPageBody configSchema={configSchema} host={host} layout={layout} />;
  };
}

// PluginSettingsPageBody owns the draft shared by every tab; Core stays the only persistence authority.
// PluginSettingsPageBody 持有所有标签共享的草稿；Core 仍是唯一持久化权威。
function PluginSettingsPageBody(props: { layout: PluginSettingsLayout; configSchema: unknown; host: PluginSettingsHost }) {
  const { t } = useI18n();
  const { layout, host } = props;
  const fields = React.useMemo(() => layoutFields(layout), [layout]);
  const credentials = React.useMemo(() => layout.sections.flatMap((section) =>
    (section.items ?? []).filter((item): item is PluginSettingsCredential => item.type === "credential")), [layout]);
  const tabs = layout.tabs ?? [];
  const [activeTab, setActiveTab] = React.useState(tabs[0]?.id ?? "");
  const [saved, setSaved] = React.useState<Record<string, unknown> | null>(null);
  const [draft, setDraft] = React.useState<Record<string, unknown> | null>(null);
  const [statuses, setStatuses] = React.useState<Record<string, PluginCredentialStatus | null>>({});
  const [loading, setLoading] = React.useState(layout.config !== undefined || credentials.length > 0);
  const [loadError, setLoadError] = React.useState("");
  const [busy, setBusy] = React.useState<string | null>(null);
  const [saveNotice, setSaveNotice] = React.useState<RowNotice | null>(null);
  const [rowNotices, setRowNotices] = React.useState<Record<string, RowNotice>>({});
  // sequence discards responses of superseded loads; mounted drops responses after unmount.
  // sequence 丢弃被取代读取的响应；mounted 在卸载后丢弃响应。
  const sequence = React.useRef(0);
  const mounted = React.useRef(true);
  // busyRef closes the same-tick double-submit gap before React renders disabled controls.
  // busyRef 在 React 渲染禁用控件前关闭同一轮的重复提交窗口。
  const busyRef = React.useRef(false);

  const load = React.useCallback(async () => {
    if (!layout.config && credentials.length === 0) return;
    const current = ++sequence.current;
    setLoading(true);
    setLoadError("");
    try {
      const [config, ...credentialStatuses] = await Promise.all([
        layout.config ? host.invoke<Record<string, unknown>>(layout.config.read_action, {}) : Promise.resolve(null),
        ...credentials.map((item) => host.invoke<PluginCredentialStatus>(item.status_action, {})),
      ]);
      if (!mounted.current || current !== sequence.current) return;
      setSaved(config);
      setDraft(config);
      setStatuses(Object.fromEntries(credentials.map((item, index) => [item.id, credentialStatuses[index] ?? null])));
    } catch (reason) {
      if (mounted.current && current === sequence.current) setLoadError(reasonText(reason));
    } finally {
      if (mounted.current && current === sequence.current) setLoading(false);
    }
  }, [credentials, host, layout.config]);

  React.useEffect(() => {
    mounted.current = true;
    void load();
    return () => { mounted.current = false; sequence.current += 1; };
  }, [load]);

  // run serializes one user-initiated mutation and reports whether it succeeded.
  // run 串行执行一次用户发起的变更，并报告是否成功。
  async function run(key: string, operation: () => Promise<void>, failure: (reason: unknown) => void): Promise<boolean> {
    if (busyRef.current) return false;
    busyRef.current = true;
    setBusy(key);
    try {
      await operation();
      return true;
    } catch (reason) {
      if (mounted.current) failure(reason);
      return false;
    } finally {
      busyRef.current = false;
      if (mounted.current) setBusy(null);
    }
  }

  const rowFailure = (id: string) => (reason: unknown) =>
    setRowNotices((notices) => ({ ...notices, [id]: { tone: "error", text: t("plugins.settings.actionFailed"), details: reasonText(reason) } }));
  const rowSuccess = (id: string, text: string) => setRowNotices((notices) => ({ ...notices, [id]: { tone: "success", text } }));

  async function save() {
    if (!layout.config || draft === null) return;
    const writeAction = layout.config.write_action;
    setSaveNotice(null);
    await run("config", async () => {
      const persisted = await host.invoke<Record<string, unknown>>(writeAction, draft);
      if (!mounted.current) return;
      setSaved(persisted);
      setDraft(persisted);
      setSaveNotice({ tone: "success", text: t("plugins.settings.saved") });
    }, (reason) => setSaveNotice({ tone: "error", text: t("plugins.settings.saveFailed"), details: reasonText(reason) }));
  }

  const settingsPending = loading && saved === null && Object.keys(statuses).length === 0;
  const settingsFailed = loadError !== "" && saved === null && Object.keys(statuses).length === 0;
  const dirty = draft !== null && saved !== null && !sameConfig(draft, saved);
  const visibleFields = fields.filter((field) => draft !== null && isFieldVisible(field, draft));
  const invalid = visibleFields.some((field) => valueIssue(propertySchema(props.configSchema, field.key), draft?.[field.key]) !== null);
  const notice = (id: string) => {
    const current = rowNotices[id];
    return current ? <PageNotice details={current.details} detailsLabel={t("plugins.mount.errorDetails")} tone={current.tone}>{current.text}</PageNotice> : null;
  };
  const loadNotice = loadError ? <PageNotice actions={<Button disabled={loading} onClick={() => void load()} size="sm" type="button" variant="outline">{t("plugins.mount.retry")}</Button>}
    details={loadError} detailsLabel={t("plugins.mount.errorDetails")} tone="error">{t("plugins.settings.loadFailed")}</PageNotice> : null;
  const setField = (key: string, value: unknown) => {
    setDraft((current) => current === null ? current : { ...current, [key]: value });
    setSaveNotice(null);
  };
  const links = (section: PluginSettingsSection) => section.links && section.links.length > 0
    ? <>{section.links.map((link) => <Button key={link.url} onClick={() => void host.openExternalUrl(link.url)} size="sm" type="button" variant="link">
      {host.message(link.label_message_key)}</Button>)}</>
    : undefined;

  // renderItems renders the rows of a settings section once the configuration and credential statuses are known.
  // renderItems 在配置与凭据状态就绪后渲染设置区块的各行。
  function renderItems(section: PluginSettingsSection): React.ReactNode {
    if (settingsPending) return <StreamPlaceholder className="w-full min-w-0" label={t("skills.plugins.loadingPage")} shape="fields" count={sectionFields(section).length || 2} />;
    if (settingsFailed) return null;
    if (section.layout === "inline") {
      return <div className="plugin-settings-inline">
        {sectionFields(section).map((field) => draft === null ? null : <SettingsInlineOption disabled={busy !== null} field={field} key={field.key}
          message={host.message} onChange={(value) => setField(field.key, value)} value={draft[field.key]} />)}
      </div>;
    }
    return (section.items ?? []).map((item) => {
      if (item.type === "field") {
        if (draft === null || !isFieldVisible(item, draft)) return null;
        return <SettingsFieldRow disabled={busy !== null} field={item} key={item.key} message={host.message}
          onChange={(value) => setField(item.key, value)} schema={propertySchema(props.configSchema, item.key)} value={draft[item.key]} />;
      }
      if (item.type === "credential") {
        return <React.Fragment key={item.id}>
          <SettingsCredentialRow busy={busy !== null} item={item} message={host.message} status={statuses[item.id] ?? null}
            onDelete={() => run(item.id, async () => {
              const status = await host.invoke<PluginCredentialStatus>(item.delete_action!, {});
              setStatuses((current) => ({ ...current, [item.id]: status }));
              rowSuccess(item.id, t("plugins.settings.credentialDeleted"));
            }, rowFailure(item.id))}
            onSave={(secret) => run(item.id, async () => {
              const status = await host.invoke<PluginCredentialStatus>(item.set_action, { [item.set_argument]: secret });
              setStatuses((current) => ({ ...current, [item.id]: status }));
              rowSuccess(item.id, t("plugins.settings.credentialSaved"));
            }, rowFailure(item.id))} />
          {notice(item.id)}
        </React.Fragment>;
      }
      return <SettingsActionRow busy={busy !== null} item={item} key={item.id} message={host.message} notice={notice(item.id)}
        onRun={() => void run(item.id, async () => {
          await host.invoke(item.action, {});
          rowSuccess(item.id, item.success_message_key ? host.message(item.success_message_key) : t("plugins.settings.actionDone"));
          // A test or refresh may change credential validation, so statuses are re-read quietly.
          // 测试或刷新可能改变凭据校验状态，因此静默重新读取状态。
          if (credentials.length > 0) void load();
        }, rowFailure(item.id))} />;
    });
  }

  // renderSection dispatches one section to the data view or the settings rows.
  // renderSection 将一个区块分派给数据视图或设置行。
  function renderSection(section: PluginSettingsSection): React.ReactNode {
    if (section.data) return <PluginDataSection footer={links(section)} host={host} key={section.id} section={section} />;
    // A failed configuration read leaves only its notice; data sections stay independent of it.
    // 配置读取失败时只保留其提示；数据区块不受其影响。
    if (settingsFailed) return null;
    return <PageSection description={section.description_message_key ? host.message(section.description_message_key) : undefined}
      footer={links(section)} key={section.id} layout={section.layout === "inline" ? "padded" : "rows"} title={host.message(section.title_message_key)}>
      {renderItems(section)}
    </PageSection>;
  }

  const byId = new Map(layout.sections.map((section) => [section.id, section]));
  // editable reports whether the visible part of the page holds fields, which decides the save bar.
  // editable 表示页面可见部分是否包含字段，决定是否显示保存栏。
  const editable = (tabs.length === 0 ? layout.sections : (tabs.find((tab) => tab.id === activeTab)?.sections ?? []).map((id) => byId.get(id)!))
    .some((section) => sectionFields(section).length > 0);
  const headerActions = layout.header
    ? <PluginSettingsHeaderActions config={settingsPending ? "loading" : saved !== null ? "ready" : "unavailable"} header={layout.header} host={host} /> : null;

  return <PageBody>
    {tabs.length > 0
      ? <PageTabs actions={headerActions} ariaLabel={t("plugins.settings.tabs")} onValueChange={setActiveTab}
        options={tabs.map((tab) => ({ value: tab.id, label: host.message(tab.title_message_key) }))} value={activeTab} />
      : headerActions ? <PageToolbar actions={headerActions} /> : null}
    {loadNotice}
    {tabs.length > 0
      ? tabs.map((tab) => <PageTabPanel active={tab.id === activeTab} key={tab.id}>{tab.sections.map((id) => renderSection(byId.get(id)!))}</PageTabPanel>)
      : layout.sections.map(renderSection)}
    {saveNotice && editable && <PageNotice details={saveNotice.details} detailsLabel={t("plugins.mount.errorDetails")} tone={saveNotice.tone}>{saveNotice.text}</PageNotice>}
    {layout.config && draft !== null && editable && <PageActionBar actions={<>
      <Button disabled={!dirty || busy !== null} onClick={() => { setDraft(saved); setSaveNotice(null); }} size="sm" type="button" variant="ghost">
        {t("plugins.settings.discard")}</Button>
      <Button disabled={!dirty || invalid || busy !== null} onClick={() => void save()} size="sm" type="button">
        {t(busy === "config" ? "plugins.settings.saving" : "plugins.settings.save")}</Button>
    </>} pending={dirty}>
      {invalid ? t("plugins.settings.invalid") : dirty ? t("plugins.settings.unsaved") : null}
    </PageActionBar>}
  </PageBody>;
}

// SettingsActionRow renders one argument-free action as a labelled row with an outline button.
// SettingsActionRow 将一个无参数操作渲染为带标签的行和描边按钮。
function SettingsActionRow(props: { item: PluginSettingsAction; busy: boolean; message: PluginMessage; notice: React.ReactNode; onRun: () => void }) {
  const label = props.message(props.item.label_message_key);
  return <>
    <PageRow description={props.item.description_message_key ? props.message(props.item.description_message_key) : undefined} label={label}>
      <Button disabled={props.busy} onClick={props.onRun} size="sm" type="button" variant="outline">{label}</Button>
    </PageRow>
    {props.notice}
  </>;
}
