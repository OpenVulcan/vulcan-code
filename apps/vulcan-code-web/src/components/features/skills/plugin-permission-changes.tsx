import * as React from "react";
import { PageNotice } from "@/components/page";
import { formatMessage, useI18n } from "@/i18n";
import { loadInstalledPluginPermissionPreview } from "@/service/skills-actions-service";
import type { PluginSummary, VulcanPluginPermissionPreview } from "@/types";

// AddedAuthorization lists what an update candidate requests beyond the installed version, per category.
// AddedAuthorization 按类别列出更新候选相对已安装版本新增请求的内容。
export interface AddedAuthorization {
  // Stable category identifiers mapped to the added entries of that category.
  // 稳定类别标识，映射到该类别的新增条目。
  permissions: string[]; hookPoints: string[]; uiMountPoints: string[]; binaryIds: string[]
  nodeEntryIds: string[]; pythonEntryIds: string[]; mcpServers: string[]; luaRuntime: boolean
}

// addedAuthorization compares two host previews; an MCP server counts as added when it is new or changes transport or Secret keys.
// addedAuthorization 比较两份宿主预览；MCP 服务器为新增、或传输类型与 Secret 键变化时视为新增。
export function addedAuthorization(installed: VulcanPluginPermissionPreview, candidate: VulcanPluginPermissionPreview): AddedAuthorization {
  const added = (before: string[], after: string[]) => after.filter((entry) => !before.includes(entry));
  const mcpServers = candidate.mcpServers.filter((server) => {
    const previous = installed.mcpServers.find((entry) => entry.serverId === server.serverId);
    return previous === undefined
      || previous.transport !== server.transport
      || added(previous.secretKeys, server.secretKeys).length > 0;
  }).map((server) => `${server.name} (${server.serverId}, ${server.transport})`);
  return {
    permissions: added(installed.permissions, candidate.permissions),
    hookPoints: added(installed.hookPoints, candidate.hookPoints),
    uiMountPoints: added(installed.uiMountPoints, candidate.uiMountPoints),
    binaryIds: added(installed.binaryIds, candidate.binaryIds),
    nodeEntryIds: added(installed.nodeEntryIds, candidate.nodeEntryIds),
    pythonEntryIds: added(installed.pythonEntryIds, candidate.pythonEntryIds),
    mcpServers,
    luaRuntime: candidate.hasLuaRuntime && !installed.hasLuaRuntime,
  };
}

// PluginPermissionChanges tells the user which access an update adds before they authorize it.
// PluginPermissionChanges 在用户授权前告知更新新增了哪些访问权限。
export function PluginPermissionChanges(props: {
  // Host preview of the update candidate the user is about to authorize.
  // 用户即将授权的更新候选的宿主预览。
  candidate: VulcanPluginPermissionPreview
}) {
  const { t } = useI18n();
  // installed and error belong to the inspected identity and reset when it changes.
  // installed 与 error 属于当前检查的身份，身份变化时重置。
  const [installed, setInstalled] = React.useState<VulcanPluginPermissionPreview | null>(null);
  const [error, setError] = React.useState("");
  const pluginId = props.candidate.pluginId;
  React.useEffect(() => {
    let active = true;
    setInstalled(null);
    setError("");
    loadInstalledPluginPermissionPreview(pluginId)
      .then((preview) => { if (active) setInstalled(preview); })
      .catch((reason: unknown) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)); });
    return () => { active = false; };
  }, [pluginId]);
  if (error) {
    return <PageNotice tone="warning">{formatMessage(t("skills.plugins.update.installedUnavailable"), { error })}</PageNotice>;
  }
  if (installed === null) {
    return null;
  }
  const changes = addedAuthorization(installed, props.candidate);
  // rows keeps category order identical to the full preview list above it.
  // rows 的类别顺序与上方完整预览列表一致。
  const rows: Array<[string, string[]]> = [
    [t("skills.plugins.permissions"), changes.permissions],
    ["Hooks", changes.hookPoints],
    [t("skills.plugins.uiMounts"), changes.uiMountPoints],
    [t("skills.plugins.nativeProcesses"), changes.binaryIds],
    ["Node.js", changes.nodeEntryIds],
    ["Python", changes.pythonEntryIds],
    [t("skills.plugins.pluginType"), changes.luaRuntime ? [t("skills.plugins.runtimePlugin")] : []],
    ["MCP", changes.mcpServers],
  ];
  const addedRows = rows.filter(([, entries]) => entries.length > 0);
  if (addedRows.length === 0) {
    return <PageNotice tone="info">{t("skills.plugins.update.noNewAuthorization")}</PageNotice>;
  }
  return (
    <PageNotice tone="warning" title={t("skills.plugins.update.newAuthorization")}>
      <dl>
        {addedRows.map(([label, entries]) => <div key={label}><dt>{label}</dt><dd>{entries.join("、")}</dd></div>)}
      </dl>
    </PageNotice>
  );
}

// InspectedPluginNotices explains how an inspected package relates to an installed plugin with the same identity.
// InspectedPluginNotices 说明已检查插件包与同身份已安装插件的关系。
export function InspectedPluginNotices(props: {
  // Host preview of the inspected package.
  // 已检查插件包的宿主预览。
  candidate: VulcanPluginPermissionPreview
  // Installed plugin with the same identity, if any.
  // 同身份的已安装插件（如有）。
  installed: PluginSummary | undefined
}) {
  const { t } = useI18n();
  const installed = props.installed;
  if (installed === undefined) {
    return null;
  }
  // Only a disabled user-installed plugin can be updated; other owners block the package.
  // 只有已停用的用户安装插件可以更新；其他归属会阻止该插件包。
  const userInstalled = installed.extension?.origin === "userInstalled";
  return (
    <>
      <PageNotice tone={userInstalled && installed.runtimeState === "disabled" ? "info" : "warning"}>
        {!userInstalled
          ? t("skills.plugins.thisIdIsOwned")
          : installed.runtimeState === "disabled"
            ? t("skills.plugins.thisWillUpdateThe")
            : t("skills.plugins.disableAndFullyDrain")}
      </PageNotice>
      {userInstalled ? <PluginPermissionChanges candidate={props.candidate} /> : null}
    </>
  );
}
