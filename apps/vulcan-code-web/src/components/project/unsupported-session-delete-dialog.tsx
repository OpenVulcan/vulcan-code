import { Button } from "@/ui-kit/primitives/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { Spinner } from "@/ui-kit/primitives/spinner";
import type { UnsupportedDeleteRequest } from "@/components/workspace/workbench-layout/use-conversation-menu-actions";
import { formatMessage, useI18n } from "@/i18n";

/** Explains that an old-format session cannot be archived and offers to delete it outright.
 * 说明旧格式会话无法归档，并提供直接删除。 */
export function UnsupportedSessionDeleteDialog(props: UnsupportedDeleteRequest) {
  const { t } = useI18n();
  return (
    <Dialog open={props.conversation !== null} onOpenChange={(open) => !open && !props.deleting && props.onCancel()}>
      <DialogContent showCloseButton={!props.deleting} size="sm">
        <DialogHeader>
          <DialogTitle>{t("workbenchConversation.unsupportedDelete.title")}</DialogTitle>
          <DialogDescription>
            {formatMessage(t("workbenchConversation.unsupportedDelete.description"), {
              conversation: props.conversation?.title ?? "",
            })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button disabled={props.deleting} onClick={props.onCancel} type="button" variant="outline">
            {t("conversation.action.cancel")}
          </Button>
          <Button disabled={props.deleting} onClick={props.onConfirm} type="button" variant="destructive">
            {props.deleting ? <Spinner className="size-4" /> : null}
            {t("workbenchConversation.unsupportedDelete.confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
