import React from "react";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { WorkspaceTextPreview } from "@/components/workspace/workspace-files-preview";
import { loadOutsideTextFile } from "@/service/outside-file-access-service";
import type { WorkspaceFileReadResponse } from "@/types";
import "@/ui-kit/styles/features/workspace-files.css";

// OutsideTextViewer shows a text file by absolute path, read-only, the way the files panel shows project text. It
// serves files the user already chose to open outside the workspace.
// OutsideTextViewer 按绝对路径只读显示文本文件，方式与文件面板显示项目文本相同。用于用户已选择打开的工作区之外的文件。
export function OutsideTextViewer(props: { path: string }) {
  const { t } = useI18n();
  const [state, setState] = React.useState<{ preview: WorkspaceFileReadResponse } | { error: string } | null>(null);
  const [attempt, setAttempt] = React.useState(0);
  React.useEffect(() => {
    let cancelled = false;
    setState(null);
    loadOutsideTextFile(props.path).then(
      (preview) => { if (!cancelled) setState({ preview }); },
      (error: unknown) => { if (!cancelled) setState({ error: errorMessage(error) }); },
    );
    return () => { cancelled = true; };
  }, [attempt, props.path]);
  if (!state) {
    return <div className="workspace-file-preview"><StreamPlaceholder className="vk-stream-inset" label={t("workspaceFilePreview.loading")} count={8} /></div>;
  }
  if ("error" in state) {
    return (
      <div className="workspace-file-preview empty error" role="alert">
        <span>{state.error}</span>
        <Button size="sm" variant="outline" onClick={() => setAttempt((value) => value + 1)}>{t("workspaceAudit.retry")}</Button>
      </div>
    );
  }
  return <WorkspaceTextPreview markdown={/\.md$/i.test(props.path)} preview={state.preview} />;
}
