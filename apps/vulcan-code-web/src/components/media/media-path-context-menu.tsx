import React from "react";
import { Copy } from "lucide-react";
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

/** Adds a right-click "copy file path" menu to one displayed image, video, or audio file.
 * 为一个展示中的图片、视频或音频文件添加右键“复制文件路径”菜单。
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
  if (!locator) return props.children;
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{props.children}</ContextMenuTrigger>
      <ContextMenuContent>
        <MediaPathCopyItem locator={locator} />
      </ContextMenuContent>
    </ContextMenu>
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
