import React from "react";

/** One project the draft may be created in.
 * 草稿可归属的单个项目。 */
export interface DraftComposerProjectOption {
  id: string;
  name: string;
}

/** Workbench-owned facts about the selected unsent draft, read by the composer without prop threading.
 * 工作台持有的当前未发送草稿事实，输入区无需逐层传参即可读取。 */
export interface DraftComposerContextValue {
  /** Local-only draft identity; stays stable when the draft changes project.
   * 仅本地的草稿身份；草稿更换项目时保持不变。 */
  draftId: string;
  /** Project currently owning the draft.
   * 当前持有草稿的项目。 */
  projectId: string;
  /** Projects the draft may move to before its first submission.
   * 首次提交前草稿可以移动到的项目。 */
  projects: DraftComposerProjectOption[];
  /** Whether the first submission is already in flight; every draft choice is then frozen.
   * 首次提交是否已在进行中；此时所有草稿选择都被冻结。 */
  submissionPending: boolean;
  /** Moves the draft to another project; ignored once a submission is pending.
   * 将草稿移动到另一个项目；提交挂起后忽略。 */
  onChangeProject: (projectId: string) => void;
}

/** Present only while an unsent draft is selected.
 * 仅在选中未发送草稿时存在。 */
export const DraftComposerContext = React.createContext<DraftComposerContextValue | null>(null);
