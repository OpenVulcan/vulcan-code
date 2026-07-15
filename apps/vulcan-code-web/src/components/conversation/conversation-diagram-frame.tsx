import * as React from "react";
import { Check, Copy } from "lucide-react";

import {
  FileTypeIcon,
  resolveFileTypeIcon,
} from "@/components/conversation/conversation-file-type-icon";
import { useI18n } from "@/i18n";

// MarkdownDiagramFrameProps describes one shared fenced-diagram presentation.
// MarkdownDiagramFrameProps 描述一个共享的围栏图表展示框。
export interface MarkdownDiagramFrameProps {
  // Accessible alternative text for the isolated diagram image.
  // 隔离图表图片使用的无障碍替代文本。
  alt: string;
  // Copies the complete fenced source through the shared Markdown clipboard path.
  // 通过共享 Markdown 剪贴板路径复制完整围栏源码。
  copySource: () => Promise<void>;
  // Pseudo file path resolved by the shared file-type icon registry.
  // 由共享文件类型图标注册表解析的伪文件路径。
  iconPath: string;
  // Blob URL containing the isolated SVG image, or null while loading.
  // 包含隔离 SVG 图片的 Blob URL，加载期间为 null。
  imageUrl: string | null;
  // Whether the diagram image is still being prepared.
  // 图表图片是否仍在准备中。
  loading: boolean;
  // Optional image-load failure callback used to restore the code fallback.
  // 可选的图片加载失败回调，用于恢复代码回退展示。
  onImageError?: () => void;
  // Human-readable fenced format label.
  // 人类可读的围栏格式标签。
  title: string;
}

// MarkdownDiagramFrame renders the shared title bar, copy action, and isolated SVG image stage.
// MarkdownDiagramFrame 渲染共享标题栏、复制操作与隔离 SVG 图片舞台。
//
// Parameters:
// 参数：
// - props: Diagram label, icon, loading state, image URL, and copy callback.
// - props：图表标签、图标、加载状态、图片 URL 与复制回调。
//
// Returns:
// 返回值：
// A code-block-style diagram frame.
// 一个代码块风格的图表展示框。
export function MarkdownDiagramFrame(props: MarkdownDiagramFrameProps) {
  // t resolves the same localized copy labels used by ordinary Markdown code blocks.
  // t 解析普通 Markdown 代码块共用的本地化复制文案。
  const { t } = useI18n();
  // icon resolves the requested format from the shared Material Icon Theme collection.
  // icon 从共享 Material Icon Theme 图标集中解析请求的格式。
  const icon = React.useMemo(() => resolveFileTypeIcon(props.iconPath), [props.iconPath]);
  // copied controls the temporary success icon after the source reaches the clipboard.
  // copied 控制源码写入剪贴板后的临时成功图标。
  const [copied, setCopied] = React.useState(false);

  React.useEffect(() => {
    if (!copied) {
      return undefined;
    }
    // timer restores the normal copy action after the same delay used by code blocks.
    // timer 按照代码块使用的相同延迟恢复普通复制操作。
    const timer = window.setTimeout(() => setCopied(false), 1200);
    return () => window.clearTimeout(timer);
  }, [copied]);

  return (
    <figure
      aria-busy={props.loading ? "true" : undefined}
      className="markdown-diagram-block"
    >
      <figcaption className="markdown-code-header">
        <span className="markdown-code-language">
          <FileTypeIcon icon={icon} />
          <span>{props.title}</span>
        </span>
        <span className="markdown-code-actions">
          <button
            aria-label={copied ? t("markdown.copied") : t("markdown.copyCode")}
            className="markdown-code-action"
            onClick={() => {
              void props.copySource().then(() => setCopied(true));
            }}
            title={copied ? t("markdown.copied") : t("markdown.copyCode")}
            type="button"
          >
            {copied ? <Check size={14} /> : <Copy size={14} />}
          </button>
        </span>
      </figcaption>
      <div
        className={props.loading
          ? "markdown-diagram-stage markdown-diagram-loading"
          : "markdown-diagram-stage"}
      >
        {props.imageUrl ? (
          <img
            alt={props.alt}
            className="markdown-diagram-image"
            draggable={false}
            onError={props.onImageError}
            src={props.imageUrl}
          />
        ) : (
          <div className="markdown-diagram-loading-bar" />
        )}
      </div>
    </figure>
  );
}
