import * as React from "react";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { PageEmptyState, PageHint, PageNotice, PageSection } from "@/components/page";
import { useI18n } from "@/i18n";
import { PluginDataBlockView } from "./plugin-data-blocks";
import { formatDataValue, readPath } from "./plugin-data-view";
import { SelectControl } from "./plugin-host-components";
import type { PluginSettingsHost } from "./plugin-settings-host";
import { formatOptionValue, type PluginSettingsSection } from "./plugin-settings-layout";

// PluginDataSection loads its read action when first shown and renders the declared blocks.
// PluginDataSection 在首次显示时读取其 Action，并渲染已声明的数据块。
// Arguments and periods change locally; only argument changes, refreshes and declared topics issue requests.
// 参数与周期在本地切换；只有参数变化、刷新与已声明主题才会发起请求。
export function PluginDataSection(props: { section: PluginSettingsSection; host: PluginSettingsHost; footer?: React.ReactNode }) {
  const { locale, t } = useI18n();
  const { section, host } = props;
  const source = section.data!;
  const [argumentsValue, setArgumentsValue] = React.useState<Record<string, string | number>>(() =>
    Object.fromEntries((source.arguments ?? []).map((argument) => [argument.name, argument.default])));
  const [result, setResult] = React.useState<unknown>(null);
  const [loadedAtMs, setLoadedAtMs] = React.useState<number | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState("");
  // sequence discards superseded reads; mounted drops completions after unmount.
  // sequence 丢弃被取代的读取；mounted 在卸载后丢弃完成结果。
  const sequence = React.useRef(0);
  const mounted = React.useRef(true);

  const load = React.useCallback(async (refresh: boolean) => {
    const current = ++sequence.current;
    setLoading(true);
    setError("");
    try {
      const next = await host.invoke<unknown>(source.action, argumentsValue, { refresh });
      if (!mounted.current || current !== sequence.current) return;
      setResult(next);
      setLoadedAtMs(Date.now());
    } catch (reason) {
      if (mounted.current && current === sequence.current) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (mounted.current && current === sequence.current) setLoading(false);
    }
  }, [argumentsValue, host, source.action]);

  React.useEffect(() => {
    mounted.current = true;
    void load(false);
    return () => { mounted.current = false; sequence.current += 1; };
  }, [load]);

  React.useEffect(() => {
    const disposers = (source.refresh_topics ?? []).map((topic) => host.subscribe(topic, () => void load(false)));
    return () => disposers.forEach((dispose) => dispose());
  }, [host, load, source.refresh_topics]);

  const actions = [
    ...(source.arguments ?? []).map((argument) => <SelectControl ariaLabel={host.message(argument.label_message_key)} className="plugin-settings-select"
      disabled={loading} key={argument.name}
      onValueChange={(selected) => {
        const option = argument.options.find((candidate) => String(candidate.value) === selected);
        if (option) setArgumentsValue((current) => ({ ...current, [argument.name]: option.value }));
      }}
      options={argument.options.map((option) => ({
        value: String(option.value),
        label: option.label_message_key ? host.message(option.label_message_key) : formatOptionValue(option.value, argument.value_format, locale),
      }))}
      value={String(argumentsValue[argument.name])} />),
    ...(source.refreshable ? [<Button disabled={loading} key="refresh" onClick={() => void load(true)} size="sm" type="button" variant="outline">
      {t(loading && result !== null ? "plugins.data.refreshing" : "plugins.data.refresh")}</Button>] : []),
  ];
  const unavailable = source.available_path !== undefined && result !== null && readPath(result, source.available_path) !== true;
  const reason = source.error_path ? readPath(result, source.error_path) : undefined;

  let body: React.ReactNode;
  if (result === null && loading) {
    body = <StreamPlaceholder label={t("plugins.data.loading")} shape="tiles" count={4} />;
  } else if (unavailable) {
    body = <PageEmptyState description={typeof reason === "string" && reason ? reason : undefined} title={host.message(source.unavailable_message_key ?? "")} />;
  } else if (result !== null) {
    body = (section.blocks ?? []).map((block, index) => <PluginDataBlockView block={block} key={`${block.type}:${index}`} message={host.message} result={result} />);
  }

  return <PageSection actions={actions.length > 0 ? actions : undefined}
    description={section.description_message_key ? host.message(section.description_message_key) : undefined}
    footer={props.footer} title={host.message(section.title_message_key)}>
    {source.show_loaded_at ? <PageHint className="plugin-data-meta">
      {`${t("plugins.data.lastLoaded")}: ${loadedAtMs === null ? t("plugins.data.never") : formatDataValue(loadedAtMs, "timestamp_ms", locale)}`}
    </PageHint> : null}
    {error ? <PageNotice actions={<Button disabled={loading} onClick={() => void load(true)} size="sm" type="button" variant="outline">{t("plugins.mount.retry")}</Button>}
      details={error} detailsLabel={t("plugins.mount.errorDetails")} tone="error">{t("plugins.data.loadFailed")}</PageNotice> : null}
    {body}
  </PageSection>;
}
