import React from "react";
import "@/ui-kit/styles/features/knowledge.css";
import { BookMarked, LoaderCircle } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { useI18n } from "@/i18n";
import { formatKnowledgeIndexStatus } from "@/lib/knowledge-index-status";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { Switch } from "@/ui-kit/primitives/switch";
import {
  listKnowledgeLibraries,
  listProjectKnowledgeBindings,
  notifyKnowledgeProjectBindingsChanged,
  setKnowledgeProjectBinding,
} from "@/service/knowledge-base-service";
import type { KnowledgeLibrarySummary, KnowledgeProjectBinding } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

// ProjectKnowledgeBasesSection renders durable library authorization and inheritance defaults for one project.
// ProjectKnowledgeBasesSection 渲染单个项目的持久化知识库授权与继承默认值。
// Parameters: props.projectId is the exact binding owner; returns authoritative authorization/default controls or explicit loading and read-error recovery.
// 参数：props.projectId 是精确绑定归属；返回权威授权和默认值控件，或明确加载与读取错误恢复界面。
export function ProjectKnowledgeBasesSection(props: {
  // Exact project identifier whose bindings are edited.
  // 当前编辑绑定的精确项目标识符。
  projectId: string;
}) {
  // t resolves user-visible project authorization copy.
  // t 解析用户可见的项目授权文案。
  const { t } = useI18n();
  // libraries stores every active library available for authorization.
  // libraries 保存可供授权的全部活动知识库。
  const [libraries, setLibraries] = React.useState<KnowledgeLibrarySummary[]>([]);
  // bindings maps authorized library IDs to this project's durable binding rows.
  // bindings 将已授权知识库标识映射到当前项目的持久化绑定记录。
  const [bindings, setBindings] = React.useState<Record<string, KnowledgeProjectBinding>>({});
  // loading tracks authoritative list refreshes.
  // loading 跟踪权威列表刷新。
  const [loading, setLoading] = React.useState(true);
  // loadedProjectId proves that both lists came from the current project before mutation.
  // loadedProjectId 证明两份列表均来自当前项目后才允许变更。
  const [loadedProjectId, setLoadedProjectId] = React.useState<string | null>(null);
  // loadError keeps failed reads distinct from authoritative empty lists.
  // loadError 将读取失败与权威空列表明确区分。
  const [loadError, setLoadError] = React.useState<string | null>(null);
  // savingLibraryId identifies the exact row whose binding mutation is active.
  // savingLibraryId 标识绑定变更正在执行的精确知识库行。
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
  const saveInFlightRef = React.useRef({ projectId: props.projectId, busy: false, active: true });
  if (saveInFlightRef.current.projectId !== props.projectId) {
    saveInFlightRef.current = { projectId: props.projectId, busy: false, active: true };
  }
  // authoritative permits mutation only after both current-project reads succeed.
  // authoritative 仅在当前项目两项读取均成功后允许变更。
  const authoritative = loadedProjectId === props.projectId && !loading && loadError === null;

  // loadBindings reloads authoritative library and project-binding records from the host.
  // loadBindings 从宿主重新加载权威知识库与项目绑定记录。
  const loadBindings = React.useCallback(async () => {
    // sequence marks the exact refresh allowed to publish UI state.
    // sequence 标记允许发布界面状态的精确刷新请求。
    const sequence = loadSequenceRef.current + 1;
    // projectId binds this request to the settings target that created the callback.
    // projectId 将当前请求绑定到创建该回调的设置目标。
    const projectId = props.projectId;
    loadSequenceRef.current = sequence;
    setLoading(true);
    setLoadedProjectId(null);
    setLoadError(null);
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
        setLoadedProjectId(projectId);
        return true;
      }
    } catch (error) {
      if (loadSequenceRef.current === sequence && activeProjectIdRef.current === projectId) {
        setLoadError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (loadSequenceRef.current === sequence && activeProjectIdRef.current === projectId) {
        setLoading(false);
      }
    }
    return false;
  }, [props.projectId]);

  React.useEffect(() => {
    // guard belongs to this mounted project visit, including A to B to A navigation.
    // guard 属于本次挂载项目访问，并区分从 A 到 B 再回 A 的导航。
    const guard = saveInFlightRef.current;
    guard.active = true;
    setLibraries([]);
    setBindings({});
    setSavingLibraryId(null);
    void loadBindings();
    return () => {
      guard.active = false;
      loadSequenceRef.current += 1;
    };
  }, [loadBindings]);

  // saveBinding persists one exact authorization/default pair and then reloads the authoritative state.
  // saveBinding 持久化一组精确授权与默认值并重新加载权威状态。
  // Parameters: libraryId identifies the library and authorized/defaultEnabled are the requested binding flags; returns after write and refresh, reporting a committed-but-unread state separately.
  // 参数：libraryId 标识资料库，authorized/defaultEnabled 是请求的绑定标记；在写入及刷新后返回，并单独报告已提交但未读回状态。
  async function saveBinding(libraryId: string, authorized: boolean, defaultEnabled: boolean): Promise<void> {
    // guard synchronously owns one mounted project visit and its in-flight write.
    // guard 同步拥有一次已挂载项目访问及其在途写入。
    const guard = saveInFlightRef.current;
    if (!guard.active || guard.busy || !authoritative || !libraries.some((library) => library.id === libraryId)) {
      return;
    }
    // projectId prevents a completed old-project mutation from repainting a newly selected project.
    // projectId 防止已完成的旧项目变更重新绘制新选择的项目。
    const projectId = props.projectId;
    guard.busy = true;
    setSavingLibraryId(libraryId);
    try {
      await setKnowledgeProjectBinding({ libraryId, projectId, authorized, defaultEnabled: authorized && defaultEnabled });
      notifyKnowledgeProjectBindingsChanged(projectId);
      if (guard.active && saveInFlightRef.current === guard && activeProjectIdRef.current === projectId) {
        // refreshed proves the successful write is also reflected by an authoritative read.
        // refreshed 证明成功写入同时已由权威读取反映。
        const refreshed = await loadBindings();
        if (guard.active && saveInFlightRef.current === guard && activeProjectIdRef.current === projectId) {
          if (refreshed) toast.success(t("projectSettings.knowledgeBasesSaved"));
          else setLoadError((current) => `${t("projectSettings.knowledgeBasesSavedRefreshFailed")} ${current ?? ""}`);
        }
      }
    } catch (error) {
      if (guard.active && saveInFlightRef.current === guard && activeProjectIdRef.current === projectId) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
    } finally {
      guard.busy = false;
      if (guard.active && saveInFlightRef.current === guard) setSavingLibraryId(null);
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
      {loading || loadedProjectId !== props.projectId && !loadError ? (
        <StreamPlaceholder label={t("projectSettings.knowledgeBasesLoading")} shape="rows" count={3} />
      ) : loadError ? (
        <div role="alert" className="vk-form-stack">
          <p className="vk-field-error vk-form-break">{loadError}</p>
          <Button type="button" variant="outline" disabled={Boolean(savingLibraryId)} onClick={() => void loadBindings()}>{t("composer.capability.retry")}</Button>
        </div>
      ) : libraries.length === 0 ? (
        <div className="knowledge-empty">{t("knowledge.emptyLibraries")}</div>
      ) : (
        <div className="knowledge-binding-grid">
          {libraries.map((library) => {
            // binding is the exact durable authorization row for this library, when present.
            // binding 是当前知识库对应的精确持久化授权记录（若存在）。
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
                  <Checkbox
                    checked={authorized}
                    disabled={Boolean(savingLibraryId)}
                    onCheckedChange={(checked) => void saveBinding(library.id, checked === true, binding?.defaultEnabled ?? false)}
                  />
                  <span>
                    <strong>{library.name}</strong>
                    <small title={library.indexStatus}>{t("knowledge.libraryCounts")
                      .replace("{documents}", String(library.documentCount))
                      .replace("{chunks}", String(library.chunkCount))
                      .replace("{status}", formatKnowledgeIndexStatus(library.indexStatus, t))}</small>
                  </span>
                </label>
                <label className="knowledge-binding-default">
                  <Switch
                    checked={binding?.defaultEnabled ?? false}
                    disabled={!authorized || Boolean(savingLibraryId)}
                    onCheckedChange={(checked) => void saveBinding(library.id, true, checked)}
                  />
                  <span>{t("knowledge.sessionDefaultEnabled")}</span>
                </label>
                {saving ? <LoaderCircle className="vk-spin" size={14} /> : null}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
