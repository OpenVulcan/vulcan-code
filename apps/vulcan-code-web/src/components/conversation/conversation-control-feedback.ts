import { toast } from "@/ui-kit/primitives/toast";

/** Runs one UI action and reports its original rejection without claiming success.
 * 执行一个界面动作并报告原始拒绝，绝不将失败当作成功。
 * @param action exact caller-owned operation; it may return synchronously.
 *               调用方持有的精确操作，可以同步返回。
 * @param failureLabel localized user-facing failure title.
 *                     面向用户的本地化失败标题。
 * @returns true only after the operation completes successfully.
 *          仅在操作成功结束后返回 true。 */
export async function runConversationControlAction(
  action: () => Promise<unknown> | unknown,
  failureLabel: string,
): Promise<boolean> {
  try {
    await action();
    return true;
  } catch (error) {
    toast.error(failureLabel, {
      description: error instanceof Error ? error.message : String(error),
    });
    return false;
  }
}
