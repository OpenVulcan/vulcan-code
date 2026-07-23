import React from "react";
import { BookMarked, LoaderCircle } from "lucide-react";
import { toast } from "sonner";
import { useI18n } from "@/i18n";
import {
  listKnowledgeLibraries,
  listProjectKnowledgeBindings,
  notifyKnowledgeProjectBindingsChanged,
  setKnowledgeProjectBinding,
} from "@/service/knowledge-base-service";
import type { KnowledgeLibrarySummary, KnowledgeProjectBinding } from "@/types";

// ProjectKnowledgeBasesSection renders durable library authorization and inheritance defaults for one project.
// ProjectKnowledgeBasesSection 渲染单个项目的持久化资料库授权与继承默认值。
export function ProjectKnowledgeBasesSection(props: {
  // Exact project identifier whose bindings are edited.
  // 当前编辑绑定的精确项目标识符。
  projectId: string;
}) {
  // t resolves user-visible project authorization copy.
  // t 解析用户可见的项目授权文案。
  const { t } = useI18n();
  // libraries stores every active library available for authorization.
  // libraries 保存可供授权的全部活动资料库。
  const [libraries, setLibraries] = React.useState<KnowledgeLibrarySummary[]>([]);
  // bindings maps authorized library IDs to this project's durable binding rows.
  // bindings 将已授权资料库标识映射到当前项目的持久化绑定记录。
  const [bindings, setBindings] = React.useState<Record<string, KnowledgeProjectBinding>>({});
  // loading tracks authoritative list refreshes.
  // loading 跟踪权威列表刷新。
  const [loading, setLoading] = React.useState(true);
  // savingLibraryId identifies the exact row whose binding mutation is active.
  // savingLibraryId 标识绑定变更正在执行的精确资料库行。
  const [savingLibraryId, setSavingLibraryId] = React.useState<string | null>(null);
  // loadSequenceRef rejects obsolete responses when the settings target changes.
  // loadSequenceRef 在设置目标变化时丢弃过期响应。
  const loadSequenceRef = React.useRef(0);
  // activeProjectIdRef lets old async callbacks prove that their project still owns the panel.
  // activeProjectIdRef 让旧异步回调确认其项目仍然拥有当前面板。
  const activeProjectIdRef = React.useRef(props.projectId);
  activeProjectIdRef.current = props.projectId;
  // saveInFlightRef synchronously prevents duplicate binding mutations before React rerenders.
  // saveInFlightRef 在 React 重新渲染前同步阻止重复绑定变更。
  const saveInFlightRef = React.useRef(false);

  // loadBindings reloads authoritative library and project-binding records from the host.
  // loadBindings 从宿主重新加载权威资料库与项目绑定记录。
  const loadBindings = React.useCallback(async () => {
    // sequence marks the exact refresh allowed to publish UI state.
    // sequence 标记允许发布界面状态的精确刷新请求。
    const sequence = loadSequenceRef.current + 1;
    // projectId binds this request to the settings target that created the callback.
    // projectId 将当前请求绑定到创建该回调的设置目标。
    const projectId = props.projectId;
    loadSequenceRef.current = sequence;
    setLoading(true);
    try {
      // libraryRows and bindingRows are parallel authoritative reads for the selected project.
      // libraryRows 与 bindingRows 是针对选中项目并行执行的权威读取。
      const [libraryRows, bindingRows] = await Promise.all([
        listKnowledgeLibraries(),
        listProjectKnowledgeBindings(projectId),
      ]);
      // nextLibraries excludes transient deleting or error rows from new authorization decisions.
      // nextLibraries 从新的授权决策中排除临时删除中或错误状态记录。
      const nextLibraries = libraryRows.filter((library) => library.lifecycleState === "active");
      // nextBindings provides constant-time row lookup while rendering checkboxes.
      // nextBindings 为复选框渲染提供常量时间的记录查找。
      const nextBindings: Record<string, KnowledgeProjectBinding> = {};
      bindingRows.forEach((binding) => {
        nextBindings[binding.libraryId] = binding;
      });
      if (loadSequenceRef.current === sequence && activeProjectIdRef.current === projectId) {
        setLibraries(nextLibraries);
        setBindings(nextBindings);
      }
    } catch (error) {
      if (loadSequenceRef.current === sequence && activeProjectIdRef.current === projectId) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (loadSequenceRef.current === sequence && activeProjectIdRef.current === projectId) {
        setLoading(false);
      }
    }
  }, [props.projectId]);

  React.useEffect(() => {
    void loadBindings();
    return () => {
      loadSequenceRef.current += 1;
    };
  }, [loadBindings]);

  // saveBinding persists one exact authorization/default pair and then reloads the authoritative state.
  // saveBinding 持久化一组精确授权与默认值并重新加载权威状态。
  async function saveBinding(libraryId: string, authorized: boolean, defaultEnabled: boolean): Promise<void> {
    if (saveInFlightRef.current) {
      return;
    }
    // projectId prevents a completed old-project mutation from repainting a newly selected project.
    // projectId 防止已完成的旧项目变更重新绘制新选择的项目。
    const projectId = props.projectId;
    saveInFlightRef.current = true;
    setSavingLibraryId(libraryId);
    try {
      await setKnowledgeProjectBinding(
        libraryId,
        projectId,
        authorized,
        authorized && defaultEnabled,
      );
      notifyKnowledgeProjectBindingsChanged(projectId);
      if (activeProjectIdRef.current === projectId) {
        await loadBindings();
        if (activeProjectIdRef.current === projectId) {
          toast.success(t("projectSettings.knowledgeBasesSaved"));
        }
      }
    } catch (error) {
      if (activeProjectIdRef.current === projectId) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
    } finally {
      saveInFlightRef.current = false;
      setSavingLibraryId(null);
    }
  }

  return (
    <div className="project-settings-section">
      <div className="project-settings-summary">
        <BookMarked size={22} />
        <div>
          <strong>{t("projectSettings.knowledgeBases")}</strong>
          <span>{t("projectSettings.knowledgeBasesDescription")}</span>
        </div>
      </div>
      {loading ? (
        <div className="knowledge-empty"><LoaderCircle className="spin" size={18} /> {t("projectSettings.knowledgeBasesLoading")}</div>
      ) : libraries.length === 0 ? (
        <div className="knowledge-empty">{t("knowledge.emptyLibraries")}</div>
      ) : (
        <div className="knowledge-binding-grid">
          {libraries.map((library) => {
            // binding is the exact durable authorization row for this library, when present.
            // binding 是当前资料库对应的精确持久化授权记录（若存在）。
            const binding = bindings[library.id];
            // authorized derives solely from an existing backend binding row.
            // authorized 仅由后端绑定记录是否存在派生。
            const authorized = Boolean(binding);
            // saving controls compact progress feedback for the exact mutated row.
            // saving 控制精确变更行的紧凑进度反馈。
            const saving = savingLibraryId === library.id;
            return (
              <div className="knowledge-binding-row" key={library.id}>
                <label className="knowledge-binding-access">
                  <input
                    checked={authorized}
                    disabled={Boolean(savingLibraryId)}
                    onChange={(event) => void saveBinding(library.id, event.target.checked, binding?.defaultEnabled ?? false)}
                    type="checkbox"
                  />
                  <span>
                    <strong>{library.name}</strong>
                    <small>{t("knowledge.libraryCounts")
                      .replace("{documents}", String(library.documentCount))
                      .replace("{chunks}", String(library.chunkCount))
                      .replace("{status}", library.indexStatus)}</small>
                  </span>
                </label>
                <label className="knowledge-binding-default">
                  <input
                    checked={binding?.defaultEnabled ?? false}
                    disabled={!authorized || Boolean(savingLibraryId)}
                    onChange={(event) => void saveBinding(library.id, true, event.target.checked)}
                    type="checkbox"
                  />
                  <span>{t("knowledge.sessionDefaultEnabled")}</span>
                </label>
                {saving ? <LoaderCircle className="spin" size={14} /> : null}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
