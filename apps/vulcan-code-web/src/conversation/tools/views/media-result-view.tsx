// Generated-resource result for media tools: each resource the provider returned, with its size and the local
// path the host staged it to; text and other non-resource results are shown as they came.
// 媒体工具的生成资源结果：供应商返回的每个资源及其大小与宿主暂存的本地路径；文本等非资源结果原样展示。
import type * as React from "react";
import { useI18n } from "@/i18n";
import { formatBytes } from "@/lib/display-format";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { domainObject, resolveV1ToolDomainPresentation, v1ModelResult } from "@/components/conversation/tool-presentations/v1-tool-domain-presentation";
import { JsonView } from "./json-view";
import { MarkdownResult } from "./result-views";

/** Renders the persisted public result and the host's generated attachments of one media call.
 * 渲染一次媒体调用的持久化公开结果与宿主生成附件。 */
export function MediaResultView(props: { event: ToolCallEvent }) {
  const { t } = useI18n();
  // result and composite follow the producer's two explicit branches, never alternate candidate paths.
  // result 与 composite 遵循产出方的两个显式分支，绝不轮询候选路径。
  const result = v1ModelResult(props.event.output);
  const object = domainObject(result);
  const composite = object?.kind === "composite" ? domainObject(object.composite) : null;
  const resources = composite === null ? object?.resources : composite.resources;
  // Only the generated_attachments host contract may supply local paths.
  // 只有 generated_attachments 宿主契约可以提供本地路径。
  const host = domainObject(props.event.output?.host_result);
  const payload = host?.kind === "generated_attachments" ? domainObject(host.payload) : null;
  const outputs = Array.isArray(payload?.outputs) ? payload.outputs : [];
  // diagnostic keeps an execution failure visible even when the response also carries resources.
  // diagnostic 即使响应还带有资源，也保持执行失败可见。
  const diagnostic = resolveV1ToolDomainPresentation(props.event, t).diagnostic;
  // supplemental keeps every non-resource composite field without repeating the resource list.
  // supplemental 保留复合结果的所有非资源字段，不重复资源列表。
  const supplemental = composite === null ? null : Object.fromEntries(Object.entries(composite)
    .filter(([key, value]) => key !== "resources" && value !== null && (!Array.isArray(value) || value.length > 0)));
  let body: React.ReactNode;
  if (Array.isArray(resources)) {
    body = resources.length === 0 ? <p className="vk-tool-note">{t("toolPresentation.media.empty")}</p> : (
      <ul className="vk-media-list">
        {resources.map((value, index) => {
          const resource = domainObject(value);
          if (!resource) return <li className="vk-tool-error" key={index} role="alert">{t("toolPresentation.media.invalidResource")}</li>;
          // Match by the producer's asset id, never by output position or file name.
          // 按产出方的资产标识匹配，绝不按输出位置或文件名关联。
          const attachment = typeof resource.asset_id === "string"
            ? outputs.map(domainObject).find((output) => output?.asset_id === resource.asset_id) : undefined;
          const localPath = typeof attachment?.local_path === "string" && attachment.local_path.trim() ? attachment.local_path : null;
          const facts = [typeof resource.mime_type === "string" ? resource.mime_type : "", typeof resource.byte_len === "number" ? formatBytes(resource.byte_len) : ""]
            .filter(Boolean).join(" · ");
          return (
            <li className="vk-media-item" data-media-resource key={index}>
              <div className="vk-media-head">
                <strong>{typeof resource.file_name === "string" ? resource.file_name : t("toolPresentation.media.resourceFallback")}</strong>
                {facts ? <span>{facts}</span> : null}
              </div>
              <div className="vk-media-path">
                <span>{t("toolPresentation.media.localPath")}</span>
                {localPath ? <code tabIndex={0}>{localPath}</code> : <em>{t("toolPresentation.media.localPathMissing")}</em>}
              </div>
              {typeof resource.resource_path === "string" ? (
                <details className="vk-model-more">
                  <summary>{t("toolPresentation.media.resourceReference")}</summary>
                  <code>{resource.resource_path}</code>
                </details>
              ) : null}
            </li>
          );
        })}
      </ul>
    );
  } else if (result === undefined) {
    body = <p className="vk-tool-note">{t("toolPresentation.media.pending")}</p>;
  } else if (object?.kind === "text" && typeof object.text === "string") {
    body = <MarkdownResult text={object.text} />;
  } else {
    body = typeof result === "string" ? <MarkdownResult text={result} /> : <JsonView value={result} />;
  }
  return (
    <div className="vk-media-result" data-media-generation-result>
      {diagnostic ? <pre className="vk-tool-error" role="alert">{diagnostic}</pre> : null}
      {body}
      {supplemental && Object.keys(supplemental).length > 0 ? <JsonView value={supplemental} /> : null}
    </div>
  );
}
