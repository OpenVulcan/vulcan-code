import * as React from "react";
import { LoaderCircle, RefreshCw } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n } from "@/i18n";
import { PageEmptyState } from "@/components/page";
import { errorMessage } from "@/lib/display-format";
import { loadOpenVulcanEdgeUsage, type OpenVulcanEdgeUsage } from "@/service/openvulcan-account-service";
import { formatCount, formatUsageHour } from "./openvulcan-usage-format";

// OpenVulcanJudgeModels lists the cloud judge models of the account's deployment as tool
// judging models, each with the account's calls over the server's window. It reads when it mounts and on
// refresh, dropping stale responses; a model the deployment stopped serving stays listed while it has calls.
// OpenVulcanJudgeModels 以工具裁定模型列出账户所属部署的云端判定模型，并附该账户在服务端时间窗内的调用。
// 挂载与刷新时读取并丢弃过期响应；部署已停止提供但仍有调用的模型继续列出。
// The parent keys this component by connection ID; returns the model table.
// 父组件按连接 ID 设置本组件 key；返回模型表。
export function OpenVulcanJudgeModels() {
  const { locale, t } = useI18n();
  const [usage, setUsage] = React.useState<OpenVulcanEdgeUsage | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // version invalidates observations on account replacement, unmount or a newer read.
  // version 在账户替换、卸载或更新的读取时使观测失效。
  const version = React.useRef(0);
  const read = React.useCallback(async () => {
    const generation = ++version.current;
    setBusy(true);
    setError(null);
    try {
      const next = await loadOpenVulcanEdgeUsage();
      if (generation === version.current) setUsage(next);
    } catch (cause) { if (generation === version.current) setError(errorMessage(cause)); }
    finally { if (generation === version.current) setBusy(false); }
  }, []);
  React.useEffect(() => {
    void read();
    return () => { version.current += 1; };
  }, [read]);
  // span names the server's window, in whole days when it is a number of days.
  // span 表示服务端时间窗；为整天时以天数表示。
  const span = usage ? (usage.hours % 24 === 0
    ? formatMessage(t("openvulcan.judgeModels.windowDays"), { days: String(usage.hours / 24) })
    : formatMessage(t("openvulcan.judgeModels.windowHours"), { hours: String(usage.hours) })) : null;
  return <section className="openvulcan-usage openvulcan-judge-models" aria-busy={busy} data-openvulcan-judge-models="">
    <header className="openvulcan-usage-header">
      <h3>{t("openvulcan.judgeModels.title")}</h3>
      {span ? <span>{span}</span> : null}
      <Button variant="ghost" size="sm" disabled={busy} onClick={() => void read()}>
        {busy ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : <RefreshCw aria-hidden="true" />}
        {t("openvulcan.usage.refresh")}
      </Button>
    </header>
    {error ? <SettingsErrorNotice message={t("openvulcan.judgeModels.failed")} detail={error} /> : null}
    {usage && usage.models.length > 0 ? <div className="openvulcan-usage-table">
      <table>
        <thead><tr>
          <th scope="col">{t("openvulcan.usage.model")}</th>
          <th scope="col">{t("openvulcan.judgeModels.type")}</th>
          <th scope="col">{t("openvulcan.judgeModels.state")}</th>
          <th scope="col">{t("openvulcan.usage.requests")}</th>
          <th scope="col">{t("openvulcan.judgeModels.items")}</th>
          <th scope="col">{t("openvulcan.judgeModels.lastCall")}</th>
        </tr></thead>
        <tbody>{usage.models.map((model) => <tr key={model.model_id} data-judge-model-enabled={model.enabled ? "true" : "false"}>
          <td className="openvulcan-usage-model">{model.model_id}</td>
          <td>{t("openvulcan.judgeModels.kind")}</td>
          <td>{t(model.enabled ? "openvulcan.judgeModels.enabled" : "openvulcan.judgeModels.retired")}</td>
          <td>
            {formatCount(String(model.requests), locale)}
            {model.errors > 0 ? <small>{formatMessage(t("openvulcan.usage.failed"), { count: formatCount(String(model.errors), locale) })}</small> : null}
          </td>
          <td>{formatCount(String(model.items), locale)}</td>
          <td>{model.last_hour ? formatUsageHour(model.last_hour, locale) : "—"}</td>
        </tr>)}</tbody>
      </table>
    </div> : usage ? <PageEmptyState title={t("openvulcan.judgeModels.empty")} /> : null}
  </section>;
}
