import { formatMessage, useI18n } from "@/i18n";
import { InlineDisclosure } from "@/ui-kit/primitives/disclosure";

/** Renders the prompt's file paths behind a collapsed attachment count.
 * 将提示词中的文件路径放在默认收起的附件数量后方。
 * `references` are ordered original paths or URIs. Returns no UI for an empty list.
 * `references` 为有序原始路径或 URI。列表为空时无界面输出。 */
export function ConversationPromptFileReferences({ references }: { references: readonly string[] }) {
  if (!references.length) return null;
  return <FileReferenceDisclosure references={references} />;
}

/** Binds a nonempty list of original paths to its localized disclosure control.
 * 将非空原始路径列表绑定到本地化折叠控件。
 * `references` are the paths to reveal. Returns a collapsed attachment disclosure.
 * `references` 为待展开的路径。返回默认收起的附件控件。 */
function FileReferenceDisclosure({ references }: { references: readonly string[] }) {
  // Reuse the attachment count label already used for queued prompts.
  // 复用现有排队提示词的附件数量文案。
  const { t } = useI18n();
  return (
    <InlineDisclosure label={formatMessage(t("pendingPrompt.attachmentCount"), { count: references.length })}>
      <ul className="vk-prompt-files">
        {references.map(reference => <li key={reference}>{reference}</li>)}
      </ul>
    </InlineDisclosure>
  );
}
