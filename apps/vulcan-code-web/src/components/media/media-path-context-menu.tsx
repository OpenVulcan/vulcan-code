import React from "react";
import { Copy, ImageIcon } from "lucide-react";
import { toast } from "sonner";
import { runConversationControlAction } from "@/components/conversation/conversation-control-feedback";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { copyText } from "@/components/workspace/workbench-layout/types";
import { useI18n } from "@/i18n";
import { mediaFileLocator, resolveMediaFilePath, type MediaFileLocator } from "./media-file-path";
import { copyImageToClipboard } from "./media-image-clipboard";

/** Adds a right-click menu to one displayed image, video, or audio file: "copy file path", plus
 * "copy image" when the click lands on an image.
 * 为一个展示中的图片、视频或音频文件添加右键菜单：“复制文件路径”，点在图片上时另有“复制图片”。
 *
 * Remote or inline sources have no file, so their element keeps the browser's own menu.
 * 远程或内联来源没有文件，因此其元素保留浏览器自身的菜单。 */
export function MediaPathContextMenu(props: {
  // locator is the host path or media URL the element displays.
  // locator 是元素展示的宿主路径或媒体 URL。
  locator: string | null | undefined;
  // children is the single element that receives the right click.
  // children 是接收右键点击的单个元素。
  children: React.ReactElement;
}) {
  const locator = React.useMemo(() => mediaFileLocator(props.locator), [props.locator]);
  // imageSource is the image under the latest right click; null for video, audio, or no image.
  // imageSource 是最近一次右键所在的图片；视频、音频或无图片时为 null。
  const [imageSource, setImageSource] = React.useState<string | null>(null);
  if (!locator) return props.children;
  return (
    <ContextMenu>
      <ContextMenuTrigger
        asChild
        onContextMenu={(event) => setImageSource(displayedImageSource(event.target, event.currentTarget))}
      >
        {props.children}
      </ContextMenuTrigger>
      <ContextMenuContent>
        {imageSource ? <MediaImageCopyItem source={imageSource} /> : null}
        <MediaPathCopyItem locator={locator} />
      </ContextMenuContent>
    </ContextMenu>
  );
}

/** Returns the displayed source of the image a right click targets.
 * 返回右键所指图片的展示来源。
 *
 * @param target the element under the pointer; container the menu trigger element.
 *               指针下的元素；container 为菜单触发元素。
 * @returns the image URL, or null when the click is not on an image or the trigger holds video or audio.
 *          图片 URL；点击不在图片上或触发元素包含视频、音频时返回 null。 */
export function displayedImageSource(target: EventTarget | null, container: Element): string | null {
  const targeted = target instanceof Element ? target.closest("img") : null;
  const image = targeted && container.contains(targeted)
    ? targeted
    : container.querySelector("video, audio") ? null : container.querySelector("img");
  return image ? image.currentSrc || image.getAttribute("src") || null : null;
}

/** The copy-image item; copies the displayed image itself as PNG.
 * 复制图片菜单项；以 PNG 复制所展示的图片本身。 */
function MediaImageCopyItem(props: { source: string }) {
  const { t } = useI18n();
  const copy = async () => {
    const copied = await runConversationControlAction(
      () => copyImageToClipboard(props.source),
      t("media.copyImageFailed"),
    );
    if (copied) toast.success(t("media.copyImageDone"));
  };
  return (
    <ContextMenuItem onSelect={() => void copy()}>
      <ImageIcon size={15} />
      <span>{t("media.copyImage")}</span>
    </ContextMenuItem>
  );
}

/** The copy item; mounted only while the menu is open, so closed media never reads the catalog.
 * 复制菜单项；仅在菜单打开时挂载，因此未打开菜单的媒体不读取文案目录。 */
function MediaPathCopyItem(props: { locator: MediaFileLocator }) {
  const { t } = useI18n();
  const copy = async () => {
    const copied = await runConversationControlAction(
      async () => copyText(await resolveMediaFilePath(props.locator)),
      t("media.copyPathFailed"),
    );
    if (copied) toast.success(t("media.copyPathDone"));
  };
  return (
    <ContextMenuItem onSelect={() => void copy()}>
      <Copy size={15} />
      <span>{t("media.copyPath")}</span>
    </ContextMenuItem>
  );
}
