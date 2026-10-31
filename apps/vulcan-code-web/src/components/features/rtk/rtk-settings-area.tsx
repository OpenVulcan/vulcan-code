import * as React from "react";
import { MetricCard, MetricGrid, PageActionBar, PageBody, PageEmptyState, PageNotice, PageRow, PageSection, PageTabPanel, PageTable, PageTabs } from "@/components/page";
import { BarChart } from "@/components/plugins/plugin-bar-chart";
import { formatDataValue } from "@/components/plugins/plugin-data-view";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { clearRtkStatistics, loadRtkSettings, loadRtkStatistics, saveRtkSettings } from "@/service/rtk-service";
import type { RtkRouteMode, RtkSettings, RtkSettingsSnapshot, RtkStatisticsPeriod, RtkStatisticsSnapshot } from "@/types";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Switch } from "@/ui-kit/primitives/switch";
import { toast } from "@/ui-kit/primitives/toast";

// RtkTab is one section of the RTK page.
// RtkTab 是 RTK 页面的一个分区。
type RtkTab = "settings" | "routes" | "statistics";

// RtkPeriodKind selects the statistics period series.
// RtkPeriodKind 选择统计周期序列。
type RtkPeriodKind = "daily" | "weekly" | "monthly";

// RtkSettingsArea is the host-native RTK page: routing switches, the route checklist, and token-saving statistics.
// Settings apply from the next turn; statistics are read on demand from the bundled RTK.
// RtkSettingsArea 是宿主原生 RTK 页面：路由开关、路由清单与 Token 节省统计。设置从下一回合生效；统计按需读取
// 随包 RTK。
export function RtkSettingsArea() {
  const { t } = useI18n();
  const [tab, setTab] = React.useState<RtkTab>("settings");
  const [snapshot, setSnapshot] = React.useState<RtkSettingsSnapshot | null>(null);
  const [draft, setDraft] = React.useState<RtkSettings | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      const loaded = await loadRtkSettings();
      setSnapshot(loaded);
      setDraft(loaded.settings);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);
  const save = async () => {
    if (!draft) return;
    setSaving(true);
    try {
      const saved = await saveRtkSettings(draft);
      setSnapshot(saved);
      setDraft(saved.settings);
      toast.success(t("plugins.settings.saved"));
    } catch (error) {
      toast.error(t("plugins.settings.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  const dirty = snapshot !== null && draft !== null && !sameRtkSettings(draft, snapshot.settings);
  const tabs = [
    { value: "settings" as const, label: t("rtk.section.settings") },
    { value: "routes" as const, label: t("rtk.section.routes") },
    { value: "statistics" as const, label: t("rtk.section.statistics") },
  ];
  return (
    <PageBody>
      <PageTabs ariaLabel={t("plugins.settings.tabs")} onValueChange={setTab} options={tabs} value={tab} />
      {loadError ? (
        <PageNotice actions={<Button onClick={() => void load()} size="sm" type="button" variant="outline">{t("composer.capability.retry")}</Button>}
          details={loadError} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">
          {t("plugins.settings.loadFailed")}
        </PageNotice>
      ) : null}
      <PageTabPanel active={tab === "settings"}>
        {snapshot && draft ? <RtkSwitches disabled={saving} onChange={setDraft} settings={draft} />
          : loadError ? null : <StreamPlaceholder count={3} label={t("plugins.settings.statusLoading")} shape="rows" />}
      </PageTabPanel>
      <PageTabPanel active={tab === "routes"}>
        {snapshot && draft ? <RtkRoutes disabled={saving} onChange={setDraft} settings={draft} snapshot={snapshot} />
          : loadError ? null : <StreamPlaceholder count={3} label={t("plugins.settings.statusLoading")} shape="rows" />}
      </PageTabPanel>
      <PageTabPanel active={tab === "statistics"}>
        <RtkStatistics />
      </PageTabPanel>
      {snapshot && draft && tab !== "statistics" ? (
        <PageActionBar
          actions={<>
            <Button disabled={!dirty || saving} onClick={() => setDraft(snapshot.settings)} size="sm" type="button" variant="ghost">
              {t("plugins.settings.discard")}
            </Button>
            <Button data-rtk-save="" disabled={!dirty || saving} onClick={() => void save()} size="sm" type="button">
              {saving ? t("plugins.settings.saving") : t("plugins.settings.save")}
            </Button>
          </>}
          pending={dirty}
        >
          {dirty ? <span role="status">{t("plugins.settings.unsaved")}</span> : null}
        </PageActionBar>
      ) : null}
    </PageBody>
  );
}

// RtkSwitches renders the three boolean RTK settings.
// RtkSwitches 渲染三个布尔型 RTK 设置。
function RtkSwitches(props: { settings: RtkSettings; disabled: boolean; onChange: (settings: RtkSettings) => void }) {
  const { t } = useI18n();
  const rows: Array<{ key: "enabled" | "gitCommitContextEnabled" | "telemetryDisabled"; label: string; help: string }> = [
    { key: "enabled", label: t("rtk.enableRouting"), help: t("rtk.enableRoutingHelp") },
    { key: "gitCommitContextEnabled", label: t("rtk.gitContext"), help: t("rtk.gitContextHelp") },
    { key: "telemetryDisabled", label: t("rtk.disableTelemetry"), help: t("rtk.disableTelemetryHelp") },
  ];
  return (
    <PageSection layout="rows" title={t("rtk.section.settings")}>
      {rows.map((row) => (
        <PageRow description={row.help} htmlFor={`rtk-${row.key}`} key={row.key} label={row.label}>
          <Switch checked={props.settings[row.key]} data-rtk-setting={row.key} disabled={props.disabled} id={`rtk-${row.key}`}
            onCheckedChange={(checked) => props.onChange({ ...props.settings, [row.key]: checked })} />
        </PageRow>
      ))}
    </PageSection>
  );
}

// RtkRoutes renders the route mode and, outside `disabled`, the curated and extended route checklists.
// In `selected` mode a check adds to the allowlist; in `all` mode an uncheck adds to the denylist.
// RtkRoutes 渲染路由模式，以及非 `disabled` 时的精选与扩展路由清单。`selected` 模式下勾选加入允许列表；
// `all` 模式下取消勾选加入拒绝列表。
function RtkRoutes(props: { settings: RtkSettings; snapshot: RtkSettingsSnapshot; disabled: boolean; onChange: (settings: RtkSettings) => void }) {
  const { t } = useI18n();
  const { settings } = props;
  const universe = [...props.snapshot.curatedRoutes, ...props.snapshot.extendedRoutes];
  const checked = checkedRoutes(settings, universe);
  // update stores the next checked set through the active mode, keeping entries outside the offered catalog.
  // update 按当前模式保存新的勾选集合，保留可选目录之外的条目。
  const update = (next: Set<string>) => props.onChange(storedRoutes(settings, universe, next));
  const groups = [
    { id: "curated", title: t("rtk.routes.curated"), values: props.snapshot.curatedRoutes },
    { id: "extended", title: t("rtk.routes.extended"), values: props.snapshot.extendedRoutes },
  ];
  return (
    <PageSection description={t("rtk.routes.description")} layout="rows" title={t("rtk.routes.title")}>
      <PageRow description={t("rtk.routeMode.help")} label={t("rtk.routeMode.label")}>
        <Select disabled={props.disabled} onValueChange={(value) => props.onChange({ ...settings, routeMode: value as RtkRouteMode })} value={settings.routeMode}>
          <SelectTrigger aria-label={t("rtk.routeMode.label")} data-rtk-route-mode="" size="sm"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="disabled">{t("rtk.routeMode.disabled")}</SelectItem>
            <SelectItem value="selected">{t("rtk.routeMode.selected")}</SelectItem>
            <SelectItem value="all">{t("rtk.routeMode.all")}</SelectItem>
          </SelectContent>
        </Select>
      </PageRow>
      {settings.routeMode === "disabled" ? null : (
        <PageRow description={t(settings.routeMode === "selected" ? "rtk.routes.selectedHelp" : "rtk.routes.allHelp")} label={t("rtk.routes.allowed")} stacked>
          <div className="plugin-settings-checklist">
            <div className="plugin-settings-checklist-toolbar">
              <Badge variant="outline">{formatMessage(t("plugins.settings.selectedCount"), { selected: universe.filter((value) => checked.has(value)).length, total: universe.length })}</Badge>
            </div>
            {groups.map((group) => (
              <fieldset className="plugin-settings-checklist-group" disabled={props.disabled} key={group.id}>
                <legend className="vk-sr-only">{group.title}</legend>
                <div aria-hidden className="plugin-settings-checklist-header"><strong>{group.title}</strong></div>
                <div className="plugin-settings-checklist-actions">
                  <Button onClick={() => update(new Set([...checked, ...group.values]))} size="xs" type="button" variant="ghost">{t("plugins.settings.selectAll")}</Button>
                  <Button onClick={() => update(new Set([...checked].filter((value) => !group.values.includes(value))))} size="xs" type="button" variant="ghost">
                    {t("plugins.settings.clear")}</Button>
                </div>
                <div className="plugin-settings-checklist-options">
                  {group.values.map((value) => <label className="plugin-settings-checklist-option" data-rtk-route={value} key={value} title={value}>
                    <Checkbox checked={checked.has(value)} onCheckedChange={(state) => {
                      const next = new Set(checked);
                      if (state === true) next.add(value); else next.delete(value);
                      update(next);
                    }} />
                    <span>{value}</span>
                  </label>)}
                </div>
              </fieldset>
            ))}
          </div>
        </PageRow>
      )}
    </PageSection>
  );
}

// RtkStatistics loads statistics on first display and renders the summary, the period chart, and the by-command table.
// RtkStatistics 在首次显示时加载统计，并渲染汇总、周期图表与按命令表格。
function RtkStatistics() {
  const { locale, t } = useI18n();
  const [statistics, setStatistics] = React.useState<RtkStatisticsSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [loading, setLoading] = React.useState(false);
  const [period, setPeriod] = React.useState<RtkPeriodKind>("daily");
  const [confirmClear, setConfirmClear] = React.useState(false);
  const [clearing, setClearing] = React.useState(false);
  const load = React.useCallback(async () => {
    setLoading(true);
    setLoadError(null);
    try {
      setStatistics(await loadRtkStatistics());
    } catch (error) {
      setLoadError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);
  const clear = async () => {
    setClearing(true);
    try {
      setStatistics(await clearRtkStatistics());
      setConfirmClear(false);
      toast.success(t("rtk.stats.historyCleared"));
    } catch (error) {
      toast.error(t("plugins.settings.actionFailed"), { description: errorMessage(error) });
    } finally {
      setClearing(false);
    }
  };
  const status = statistics
    ? <Badge data-rtk-availability={statistics.available ? "ready" : "unavailable"} variant={statistics.available ? "success" : "warning"}>
      {t(statistics.available ? "rtk.availability.ready" : "rtk.availability.unavailable")}</Badge>
    : undefined;
  const refresh = <Button disabled={loading || clearing} onClick={() => void load()} size="sm" type="button" variant="outline">{t("knowledge.refresh")}</Button>;
  return (
    <>
      <PageSection actions={refresh} description={t("rtk.stats.description")} status={status} title={t("rtk.stats.title")}>
        {loadError ? (
          <PageNotice details={loadError} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">{t("rtk.stats.unavailableHelp")}</PageNotice>
        ) : !statistics || (loading && !statistics.available) ? (
          <StreamPlaceholder count={3} label={t("plugins.settings.statusLoading")} shape="rows" />
        ) : !statistics.available ? (
          <PageNotice details={statistics.error ?? undefined} detailsLabel={t("settings.errors.showErrorDetails")} tone="warning">{t("rtk.stats.unavailableHelp")}</PageNotice>
        ) : statistics.summary.totalCommands === 0 && statistics.byCommand.length === 0 ? (
          <PageEmptyState title={t("rtk.stats.empty")} />
        ) : (
          <RtkStatisticsBody locale={locale} onPeriodChange={setPeriod} period={period} statistics={statistics} />
        )}
      </PageSection>
      {statistics?.available ? (
        <PageSection actions={<Button data-rtk-clear-history="" disabled={loading || clearing} onClick={() => setConfirmClear(true)} size="sm" type="button" variant="destructive">
          {t("rtk.stats.clearHistory")}</Button>} description={t("rtk.stats.clearHistoryHelp")} title={t("rtk.stats.historyTitle")} />
      ) : null}
      <Dialog onOpenChange={(open) => { if (!clearing) setConfirmClear(open); }} open={confirmClear}>
        <DialogContent showCloseButton={!clearing} size="sm">
          <DialogHeader>
            <DialogTitle>{t("rtk.stats.clearHistory")}</DialogTitle>
            <DialogDescription>{t("rtk.stats.clearHistoryHelp")}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button disabled={clearing} onClick={() => setConfirmClear(false)} type="button" variant="outline">{t("conversation.action.cancel")}</Button>
            <Button data-rtk-clear-confirm="" disabled={clearing} onClick={() => void clear()} type="button" variant="destructive">{t("rtk.stats.clearHistory")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

// RtkStatisticsBody renders available statistics.
// RtkStatisticsBody 渲染可用的统计数据。
function RtkStatisticsBody(props: { statistics: RtkStatisticsSnapshot; period: RtkPeriodKind; locale: string; onPeriodChange: (period: RtkPeriodKind) => void }) {
  const { t } = useI18n();
  const { summary, byCommand } = props.statistics;
  const format = (value: number, kind: "number" | "compact" | "percent" | "ratio" | "duration_ms") => formatDataValue(value, kind, props.locale);
  const periods: RtkStatisticsPeriod[] = props.statistics[props.period];
  const dateFormat = props.period === "monthly" ? "month" : "date";
  const periodOptions: Array<{ value: RtkPeriodKind; label: string }> = [
    { value: "daily", label: t("rtk.stats.daily") },
    { value: "weekly", label: t("rtk.stats.weekly") },
    { value: "monthly", label: t("rtk.stats.monthly") },
  ];
  return (
    <>
      <MetricGrid>
        <MetricCard icon="tokens" label={t("rtk.stats.savedTokens")} value={format(summary.totalSaved, "compact")} />
        <MetricCard icon="success" label={t("rtk.stats.averageSavings")} value={format(summary.avgSavingsPct, "percent")} />
        <MetricCard icon="activity" label={t("rtk.stats.commands")} value={format(summary.totalCommands, "number")} />
        <MetricCard icon="requests" label={t("rtk.stats.capacityRatio")} value={format(summary.totalInput / Math.max(1, summary.totalOutput), "ratio")} />
        <MetricCard icon="internal" label={t("rtk.stats.excludedOutputs")} value={format(summary.oversizedExcluded, "number")} />
      </MetricGrid>
      <PageRow label={t("rtk.stats.savedTokens")} stacked>
        <Select onValueChange={(value) => props.onPeriodChange(value as RtkPeriodKind)} value={props.period}>
          <SelectTrigger aria-label={t("rtk.stats.savedTokens")} data-rtk-period="" size="sm"><SelectValue /></SelectTrigger>
          <SelectContent>
            {periodOptions.map((option) => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}
          </SelectContent>
        </Select>
        <BarChart ariaLabel={t("rtk.stats.savedTokens")} emptyLabel={t("rtk.stats.empty")} points={periods.map((entry) => {
          const label = formatDataValue(entry.start, dateFormat, props.locale);
          return {
            key: entry.start,
            label,
            // BarChart takes non-negative heights; a period whose compression grew the output shows as zero.
            // BarChart 只接受非负柱高；压缩使输出变大的周期显示为零。
            value: Math.max(0, entry.savedTokens),
            title: `${label}: ${format(entry.savedTokens, "number")} · ${format(entry.savingsPct, "percent")}`,
          };
        })} />
      </PageRow>
      <PageTable
        columns={[
          { key: "command", label: t("rtk.stats.command") },
          { key: "count", label: t("rtk.stats.count"), numeric: true },
          { key: "saved", label: t("rtk.stats.saved"), numeric: true },
          { key: "rate", label: t("rtk.stats.savingsRate"), numeric: true },
          { key: "time", label: t("rtk.stats.time"), numeric: true },
        ]}
        empty={t("rtk.stats.noCommandBreakdown")}
        label={t("rtk.stats.byCommand")}
        rows={byCommand.map((row, index) => ({
          key: `${index}:${row.command}`,
          cells: {
            command: <code>{row.command}</code>,
            count: format(row.count, "number"),
            saved: format(row.savedTokens, "compact"),
            rate: format(row.savingsPct, "percent"),
            time: format(row.avgTimeMs, "duration_ms"),
          },
        }))}
      />
    </>
  );
}

// checkedRoutes projects the stored lists onto the offered catalog as a checked set.
// checkedRoutes 将已存列表投影为可选目录上的勾选集合。
export function checkedRoutes(settings: RtkSettings, universe: readonly string[]): Set<string> {
  if (settings.routeMode === "all") {
    const disabled = new Set(settings.disabledRoutes);
    return new Set(universe.filter((route) => !disabled.has(route)));
  }
  return new Set(settings.enabledRoutes.filter((route) => universe.includes(route)));
}

// storedRoutes writes a checked set back into the list owned by the active mode; the other list is untouched.
// storedRoutes 将勾选集合写回当前模式拥有的列表；另一个列表保持不变。
export function storedRoutes(settings: RtkSettings, universe: readonly string[], checked: ReadonlySet<string>): RtkSettings {
  const outside = (routes: readonly string[]) => routes.filter((route) => !universe.includes(route));
  if (settings.routeMode === "all") {
    const disabledRoutes = [...outside(settings.disabledRoutes), ...universe.filter((route) => !checked.has(route))].sort();
    return { ...settings, disabledRoutes };
  }
  const enabledRoutes = [...outside(settings.enabledRoutes), ...universe.filter((route) => checked.has(route))].sort();
  return { ...settings, enabledRoutes };
}

// sameRtkSettings compares two settings field by field; route lists are sorted on both sides.
// sameRtkSettings 逐字段比较两份设置；两侧的路由列表均已排序。
function sameRtkSettings(left: RtkSettings, right: RtkSettings): boolean {
  const sameList = (a: readonly string[], b: readonly string[]) => a.length === b.length && a.every((value, index) => value === b[index]);
  return left.enabled === right.enabled
    && left.routeMode === right.routeMode
    && left.telemetryDisabled === right.telemetryDisabled
    && left.gitCommitContextEnabled === right.gitCommitContextEnabled
    && sameList(left.enabledRoutes, right.enabledRoutes)
    && sameList(left.disabledRoutes, right.disabledRoutes);
}

// The page reuses the plugin checklist and chart vocabulary.
// 页面复用插件清单与图表词汇。
import "@/ui-kit/styles/features/plugin-host-components.css";
