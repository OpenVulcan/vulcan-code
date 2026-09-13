import React from "react";
import { I18nProvider } from "../../src/i18n";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { GeneratedMediaPlayerWindow } from "../../src/components/media/generated-media-player";
import { Button } from "../../src/components/ui/button";
import type { GeneratedMediaAttachment } from "../../src/components/media/generated-media-types";

// panorama contains only labeled vector geometry and never reads or uploads a user image.
// panorama 仅包含有标注的矢量几何，不读取或上传用户图片。
const panorama = "data:image/svg+xml," + encodeURIComponent(
  '<svg xmlns="http://www.w3.org/2000/svg" width="10000" height="2000" viewBox="0 0 10000 2000">' +
  '<rect width="10000" height="2000" fill="#e8eef5"/><rect x="40" y="40" width="9920" height="1920" fill="none" stroke="#246c53" stroke-width="80"/>' +
  '<path d="M0 0L10000 2000M10000 0L0 2000" stroke="#6f8091" stroke-width="25"/>' +
  '<text x="200" y="350" font-size="240" fill="#132b44">LEFT</text><text x="8900" y="1850" font-size="240" fill="#132b44">RIGHT</text></svg>',
);

// cases supplies explicit local fixtures; invalid media bytes exercise the browser's real decoder failure.
// cases 提供明确的本地样本；无效媒体字节用于验证浏览器实际解码失败。
const cases: Record<string, GeneratedMediaAttachment> = {
  image: { kind: "image", fileName: "合成全景图 · 10000 × 2000", localPath: panorama, mimeType: "image/svg+xml" },
  "image-failure": { kind: "image", fileName: "合成图片解码失败", localPath: "data:image/png;base64,AAAA", mimeType: "image/png" },
  "video-failure": { kind: "video", fileName: "合成视频解码失败", localPath: "data:video/mp4;base64,AAAA", mimeType: "video/mp4" },
  "audio-failure": { kind: "audio", fileName: "合成音频解码失败", localPath: "data:audio/wav;base64,AAAA", mimeType: "audio/wav" },
};

/** Mounts the production media viewer with a deterministic large panorama and a reopen control.
 * 使用确定的超宽全景图和重开按钮挂载生产媒体查看器。
 * Takes no parameters; returns an isolated view with no RPC, permission, or native-window actions.
 * 无参数；返回不调用 RPC、权限或原生窗口动作的隔离界面。 */
export function AuditMediaSurface() {
  // caseName selects one declared fixture, defaulting only when the query parameter is absent.
  // caseName 选择已声明样本，仅在缺少查询参数时使用默认样本。
  const caseName = new URLSearchParams(window.location.search).get("mediaCase") ?? "image";
  // attachment resolves this exact fixture or fails explicitly on an invalid audit URL.
  // attachment 解析准确样本，验收网址无效时明确失败。
  const attachment = cases[caseName];
  if (!attachment) throw new Error(`Unknown media audit case: ${caseName}`);
  // open preserves the ordinary close and reopen lifecycle without modifying any project.
  // open 保留普通关闭与重开生命周期，不修改任何项目。
  const [open, setOpen] = React.useState(true);
  return <I18nProvider initialLocale="zh-CN"><TooltipProvider>
    <Button onClick={() => setOpen(true)}>打开合成全景图</Button>
    {open ? <GeneratedMediaPlayerWindow attachment={attachment} onClose={() => setOpen(false)} /> : null}
  </TooltipProvider></I18nProvider>;
}
