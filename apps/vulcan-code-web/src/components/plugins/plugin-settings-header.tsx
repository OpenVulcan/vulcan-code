import * as React from "react";
import { StatusPill } from "@/components/shared/status";
import { useI18n } from "@/i18n";
import { PluginMetadataLink, TextDocumentDialog } from "./plugin-host-components";
import type { PluginSettingsHost } from "./plugin-settings-host";
import type { PluginSettingsHeader } from "./plugin-settings-layout";

// ConfigState is the configuration read outcome the availability pill reflects.
// ConfigState 是可用状态标签所反映的配置读取结果。
export type ConfigState = "loading" | "ready" | "unavailable";

// PluginSettingsHeaderActions renders the declared status pill, metadata links and package documents.
// PluginSettingsHeaderActions 渲染已声明的状态标签、元数据链接与包内文档。
export function PluginSettingsHeaderActions(props: { header: PluginSettingsHeader; host: PluginSettingsHost; config: ConfigState }) {
  const { t } = useI18n();
  const { header, host } = props;
  const [openId, setOpenId] = React.useState<string | null>(null);
  // documents caches each read text by id; errors stay per document so one failure never hides another.
  // documents 按 id 缓存已读文本；错误按文档分开，一个失败不会掩盖另一个。
  const [documents, setDocuments] = React.useState<Record<string, { content?: string; error?: string }>>({});
  const mounted = React.useRef(true);
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  // openDocument shows the dialog at once and reads the package file the first time only.
  // openDocument 立即显示对话框，并只在第一次读取包内文件。
  async function openDocument(id: string, asset: string) {
    setOpenId(id);
    if (documents[id]?.content !== undefined) return;
    setDocuments((current) => ({ ...current, [id]: {} }));
    try {
      const content = await host.readTextAsset(asset);
      if (mounted.current) setDocuments((current) => ({ ...current, [id]: { content } }));
    } catch (reason) {
      if (mounted.current) setDocuments((current) => ({ ...current, [id]: { error: reason instanceof Error ? reason.message : String(reason) } }));
    }
  }

  const open = header.documents?.find((document) => document.id === openId);
  const openState = open ? documents[open.id] : undefined;
  return <div className="plugin-settings-header">
    {header.status ? <StatusPill label={props.config === "loading" ? t("plugins.settings.statusLoading")
      : host.message(props.config === "ready" ? header.status.ready_message_key : header.status.unavailable_message_key)}
      status={props.config === "ready" ? "ready" : props.config === "loading" ? "pending" : "warning"} /> : null}
    {(header.links ?? []).map((link) => <PluginMetadataLink ariaLabel={host.message(link.aria_label_message_key ?? link.label_message_key)} href={link.url}
      icon={link.icon === "github" ? "github" : "link"} key={link.url} label={host.message(link.label_message_key)}
      onActivate={() => void host.openExternalUrl(link.url)} />)}
    {(header.documents ?? []).map((document) => <PluginMetadataLink ariaLabel={host.message(document.aria_label_message_key ?? document.label_message_key)}
      icon={document.icon === "license" ? "license" : "document"} key={document.id} label={host.message(document.label_message_key)}
      onActivate={() => void openDocument(document.id, document.asset)} />)}
    {open ? <TextDocumentDialog content={openState?.content ?? ""} description={open.description_message_key ? host.message(open.description_message_key) : undefined}
      error={openState?.error ?? ""} loading={openState !== undefined && openState.content === undefined && openState.error === undefined}
      loadingLabel={t("plugins.settings.documentLoading")} onOpenChange={(next) => { if (!next) setOpenId(null); }} open title={host.message(open.title_message_key)} /> : null}
  </div>;
}
