import * as React from "react";
import { PageEmptyState } from "@/components/page";
import { RefreshCw, Settings2, Trash2 } from "lucide-react";
import { Badge } from "@/ui-kit/primitives/badge";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { Switch } from "@/ui-kit/primitives/switch";
import { Textarea } from "@/ui-kit/primitives/textarea";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { useI18n } from "@/i18n";
import type {
  LuaSkillConfigDescriptor,
  LuaSkillConfigItem,
  LuaSkillConfigValidation,
  LuaSkillSummary,
} from "@/types";
import type { SkillActionProps } from "./skill-area-types";

// LuaSkillConfigDraftValue is one editable scalar retained only inside the active dialog.
// LuaSkillConfigDraftValue 是仅保留在活动弹窗内的单个可编辑标量。
type LuaSkillConfigDraftValue = string | boolean;

// LuaSkillConfigDraft stores editor values keyed by exact declaration key.
// LuaSkillConfigDraft 按精确声明键保存编辑器值。
type LuaSkillConfigDraft = Record<string, LuaSkillConfigDraftValue>;

// LuaSkillConfigItemGroup contains one visible declaration group in author-defined order.
// LuaSkillConfigItemGroup 包含一个按作者顺序展示的配置声明分组。
interface LuaSkillConfigItemGroup {
  // Stable group label; an empty label represents ungrouped declarations.
  // 稳定分组名称；空名称表示未分组声明。
  label: string;
  // Ordered declarations in this group.
  // 当前分组内已排序的声明。
  items: LuaSkillConfigItem[];
}

// draftFromDescriptor creates a fresh editor draft without retaining sensitive effective values.
// draftFromDescriptor 创建一份不保留敏感有效值的新编辑草稿。
//
// Parameters: descriptor is the latest host-safe package configuration snapshot.
// 参数：descriptor 是最新宿主安全技能包配置快照。
// Returns: editable scalar values keyed by declaration key.
// 返回值：按声明键索引的可编辑标量值。
export function draftFromDescriptor(descriptor: LuaSkillConfigDescriptor): LuaSkillConfigDraft {
  // draft accumulates exact initial field values for the current snapshot.
  // draft 累积当前快照的精确初始字段值。
  const draft: LuaSkillConfigDraft = {};
  for (const item of descriptor.items) {
    if (item.sensitive) {
      draft[item.key] = "";
    } else if (item.type === "boolean") {
      draft[item.key] = item.value === "true";
    } else {
      draft[item.key] = item.value ?? "";
    }
  }
  return draft;
}

// parseDraftValue converts one edited field into the exact typed scalar accepted by LuaSkills.
// parseDraftValue 把单个已编辑字段转换为 LuaSkills 接受的精确类型化标量。
//
// Parameters: item is the authoritative declaration and value is the current editor scalar.
// 参数：item 是权威声明，value 是当前编辑器标量。
// Returns: one typed scalar or throws a field-qualified validation error.
// 返回值：一个类型化标量，或抛出带字段信息的校验错误。
export function parseDraftValue(item: LuaSkillConfigItem, value: LuaSkillConfigDraftValue): string | number | boolean {
  if (item.type === "boolean") {
    if (typeof value === "boolean") {
      return value;
    }
    if (value === "true" || value === "false") {
      return value === "true";
    }
    throw new Error(`${item.key}: boolean value required`);
  }
  if (typeof value !== "string") {
    throw new Error(`${item.key}: text value required`);
  }
  if (item.type === "integer") {
    if (value.trim().length === 0) {
      throw new Error(`${item.key}: integer value required`);
    }
    // parsedInteger is the exact safe integer represented by the input text.
    // parsedInteger 是输入文本表示的精确安全整数。
    const parsedInteger = Number(value);
    if (!Number.isSafeInteger(parsedInteger)) {
      throw new Error(`${item.key}: safe integer required`);
    }
    return parsedInteger;
  }
  if (item.type === "float") {
    if (value.trim().length === 0) {
      throw new Error(`${item.key}: finite number required`);
    }
    // parsedFloat is the exact finite float represented by the input text.
    // parsedFloat 是输入文本表示的精确有限浮点数。
    const parsedFloat = Number(value);
    if (!Number.isFinite(parsedFloat)) {
      throw new Error(`${item.key}: finite number required`);
    }
    return parsedFloat;
  }
  return value;
}

// orderedConfigItems sorts declarations by group, author order, and stable key.
// orderedConfigItems 按分组、作者顺序与稳定键排序声明。
//
// Parameters: items is the package-authored declaration list.
// 参数：items 是技能包作者声明列表。
// Returns: a stable sorted copy.
// 返回值：稳定排序后的副本。
export function orderedConfigItems(items: LuaSkillConfigItem[]): LuaSkillConfigItem[] {
  return [...items].sort((left, right) => (
    (left.group ?? "").localeCompare(right.group ?? "")
    || (left.order ?? Number.MAX_SAFE_INTEGER) - (right.order ?? Number.MAX_SAFE_INTEGER)
    || left.key.localeCompare(right.key)
  ));
}

// groupConfigItems partitions an already ordered declaration list by its exact group label.
// groupConfigItems 按精确分组名称划分已经排序的声明列表。
//
// Parameters: items is the stable author-ordered declaration list.
// 参数：items 是按作者规则稳定排序的声明列表。
// Returns: visible groups in the same stable order.
// 返回值：保持相同稳定顺序的可见分组。
export function groupConfigItems(items: LuaSkillConfigItem[]): LuaSkillConfigItemGroup[] {
  // groups accumulates contiguous declarations with the same exact label.
  // groups 累积具有相同精确名称的连续声明。
  const groups: LuaSkillConfigItemGroup[] = [];
  for (const item of items) {
    // label is the exact authored group or the ungrouped sentinel.
    // label 是作者声明的精确分组名称或未分组哨兵。
    const label = item.group ?? "";
    // currentGroup is the last stable group accumulated so far.
    // currentGroup 是当前已累积的最后一个稳定分组。
    const currentGroup = groups[groups.length - 1];
    if (!currentGroup || currentGroup.label !== label) {
      groups.push({ label, items: [item] });
    } else {
      currentGroup.items.push(item);
    }
  }
  return groups;
}

// LuaSkillConfigDialog renders and synchronizes one dynamic package configuration editor.
// LuaSkillConfigDialog 渲染并同步单个动态技能包配置编辑器。
export function LuaSkillConfigDialog(props: {
  // Service-backed LuaSkills actions.
  // 服务端支持的 LuaSkills 动作。
  actions: SkillActionProps;
  // Whether the dialog is open.
  // 弹窗当前是否打开。
  open: boolean;
  // Selected Lua Skill package.
  // 当前选中的 Lua Skill 包。
  skill: LuaSkillSummary | null;
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useI18n();
  // descriptor is the latest host-safe configuration snapshot.
  // descriptor 是最新宿主安全配置快照。
  const [descriptor, setDescriptor] = React.useState<LuaSkillConfigDescriptor | null>(null);
  // validation is the latest authoritative completeness result.
  // validation 是最新权威完整性结果。
  const [validation, setValidation] = React.useState<LuaSkillConfigValidation | null>(null);
  // draft stores current editor scalars, including user-entered sensitive values.
  // draft 保存当前编辑器标量，包括用户输入的敏感值。
  const [draft, setDraft] = React.useState<LuaSkillConfigDraft>({});
  // changedKeys identifies fields explicitly changed by the user.
  // changedKeys 标识用户明确修改的字段。
  const [changedKeys, setChangedKeys] = React.useState<string[]>([]);
  // changedKeysRef exposes the latest edited keys to stable async callbacks.
  // changedKeysRef 向稳定异步回调暴露最新已编辑键。
  const changedKeysRef = React.useRef<string[]>([]);
  // busy reports an active load, write, validation, or delete operation.
  // busy 表示正在执行加载、写入、校验或删除操作。
  const [busy, setBusy] = React.useState(false);
  // mutationRef prevents duplicate manual operations and dismissal while a CAS write is unresolved.
  // mutationRef 防止重复手动操作及 CAS 写入未完成期间的关闭。
  const mutationRef = React.useRef(false);
  // errorMessage contains the current value-free service or editor error.
  // errorMessage 保存当前不含配置值的服务或编辑器错误。
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);
  // configWatchIssues retains unresolved watcher failures for this exact store and package.
  // configWatchIssues 保留当前精确存储域与技能包尚未恢复的监听故障。
  const [configWatchIssues, setConfigWatchIssues] = React.useState<{
    storage: string | null;
    skill: string | null;
  }>({ storage: null, skill: null });
  // conflictMessage tells the user that the latest revision was reloaded without discarding drafts.
  // conflictMessage 告知用户已重新加载最新修订且未丢弃草稿。
  const [conflictMessage, setConflictMessage] = React.useState<string | null>(null);
  // eventCursorRef retains the engine-local event cursor without triggering renders.
  // eventCursorRef 在不触发渲染的情况下保留引擎内事件游标。
  const eventCursorRef = React.useRef<string | null>(null);
  // loadGenerationRef prevents stale asynchronous loads from replacing a newer snapshot.
  // loadGenerationRef 防止过期异步加载覆盖较新的快照。
  const loadGenerationRef = React.useRef(0);
  // skillId is the stable selected package identity across catalog object refreshes.
  // skillId 是跨目录对象刷新的稳定选中技能包身份。
  const skillId = props.skill?.skillId ?? null;

  React.useEffect(() => {
    changedKeysRef.current = changedKeys;
  }, [changedKeys]);

  // loadDescriptor loads one snapshot and optionally preserves explicitly edited draft fields.
  // loadDescriptor 加载一个快照，并可选择保留明确编辑过的草稿字段。
  //
  // Parameters: preserveDraft keeps changed fields across CAS conflicts or external reloads.
  // 参数：preserveDraft 在 CAS 冲突或外部重载时保留已变更字段。
  // Returns: whether the requested snapshot remained current and was applied.
  // 返回值：请求的快照是否保持最新并已应用。
  const loadDescriptor = React.useCallback(async (preserveDraft: boolean): Promise<boolean> => {
    if (!skillId) {
      return false;
    }
    // loadGeneration identifies this request among overlapping refreshes.
    // loadGeneration 在重叠刷新请求中标识当前请求。
    const loadGeneration = loadGenerationRef.current + 1;
    loadGenerationRef.current = loadGeneration;
    // loadedDescriptor is the exact host-safe snapshot returned by the service.
    // loadedDescriptor 是服务返回的精确宿主安全快照。
    const loadedDescriptor = await props.actions.onDescribeLuaSkillConfig(skillId);
    if (loadGeneration !== loadGenerationRef.current) {
      return false;
    }
    if (!loadedDescriptor) {
      setDescriptor(null);
      setValidation(null);
      setDraft({});
      changedKeysRef.current = [];
      setChangedKeys([]);
      return true;
    }
    // loadedValidation is computed against the same package after descriptor discovery.
    // loadedValidation 是在描述发现后针对同一技能包计算的校验结果。
    const loadedValidation = await props.actions.onValidateLuaSkillConfig(skillId);
    if (loadGeneration !== loadGenerationRef.current) {
      return false;
    }
    setDescriptor(loadedDescriptor);
    setValidation(loadedValidation);
    setDraft((currentDraft) => {
      // nextDraft begins from the latest non-sensitive server snapshot.
      // nextDraft 从最新非敏感服务端快照开始。
      const nextDraft = draftFromDescriptor(loadedDescriptor);
      if (preserveDraft) {
        for (const key of changedKeysRef.current) {
          if (Object.prototype.hasOwnProperty.call(currentDraft, key)) {
            nextDraft[key] = currentDraft[key];
          }
        }
      }
      return nextDraft;
    });
    if (!preserveDraft) {
      changedKeysRef.current = [];
      setChangedKeys([]);
    }
    return true;
  }, [props.actions, skillId]);

  React.useEffect(() => {
    if (!props.open || !skillId) {
      loadGenerationRef.current += 1;
      setDescriptor(null);
      setValidation(null);
      setDraft({});
      changedKeysRef.current = [];
      setChangedKeys([]);
      setErrorMessage(null);
      setConfigWatchIssues({ storage: null, skill: null });
      setConflictMessage(null);
      eventCursorRef.current = null;
      return;
    }
    // cancelled prevents a completed initial request from updating a closed dialog.
    // cancelled 防止已完成的初始请求更新已关闭弹窗。
    let cancelled = false;
    setBusy(true);
    setErrorMessage(null);
    setConfigWatchIssues({ storage: null, skill: null });
    void loadDescriptor(false)
      .catch((error: unknown) => {
        if (!cancelled) {
          setErrorMessage(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setBusy(false);
        }
      });
    return () => {
      cancelled = true;
      loadGenerationRef.current += 1;
    };
  }, [loadDescriptor, props.open, skillId]);

  React.useEffect(() => {
    if (!props.open || !skillId || !descriptor || busy) {
      return;
    }
    // cancelled stops the recursive poll after the dialog closes.
    // cancelled 在弹窗关闭后停止递归轮询。
    let cancelled = false;
    // timeoutId owns the next scheduled event poll.
    // timeoutId 持有下一次计划事件轮询。
    let timeoutId: number | null = null;
    // poll requests one event batch and reloads only when the selected package changed.
    // poll 请求一个事件批次，并仅在选中技能包变更时重新加载。
    const poll = async (): Promise<void> => {
      try {
        // batch is the ordered value-free event result from the retained engine.
        // batch 是保留引擎返回的有序且不含值的事件结果。
        const batch = await props.actions.onPollLuaSkillConfigEvents(eventCursorRef.current);
        if (cancelled) {
          return;
        }
        eventCursorRef.current = batch.nextSequence;
        // Apply exact-store watcher transitions in event order. A local write proves
        // storage access only; an external reload proves that this watcher recovered.
        // 按事件顺序应用精确存储域的监听转换。本地写入只证明存储可访问；外部重载才证明
        // 当前监听器已经恢复。
        setConfigWatchIssues((current) => {
          const next = { ...current };
          for (const event of batch.events) {
            if (event.storeScope !== descriptor.storeScope) {
              continue;
            }
            if (event.error) {
              const message = `${event.error.code}: ${event.error.message}`;
              if (event.skillId === null) {
                next.storage = message;
              } else if (event.skillId === skillId) {
                next.skill = message;
              }
              continue;
            }
            if (event.source === "external_reload") {
              next.storage = null;
              if (event.skillId === skillId) {
                next.skill = null;
              }
            }
          }
          return next.storage === current.storage && next.skill === current.skill ? current : next;
        });
        // latestSelectedEvent is the newest retained event for the open package.
        // latestSelectedEvent 是当前打开技能包的最新保留事件。
        const latestSelectedEvent = [...batch.events]
          .reverse()
          .find((event) => (
            event.storeScope === descriptor.storeScope && event.skillId === skillId
          ));
        if (latestSelectedEvent && !latestSelectedEvent.error
          && latestSelectedEvent.revision !== descriptor.revision) {
          if (await loadDescriptor(true)) {
            setConflictMessage(t("skills.luaConfig.externalChange"));
          }
        }
      } catch (error: unknown) {
        if (!cancelled) {
          setErrorMessage(error instanceof Error ? error.message : String(error));
        }
      } finally {
        if (!cancelled) {
          timeoutId = window.setTimeout(() => void poll(), 1500);
        }
      }
    };
    timeoutId = window.setTimeout(() => void poll(), 1500);
    return () => {
      cancelled = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [busy, descriptor, loadDescriptor, props.actions, props.open, skillId, t]);

  // updateDraft records one explicit user edit without mutating unrelated fields.
  // updateDraft 记录单次明确用户编辑且不修改无关字段。
  // Parameters: key identifies the item, value is the edited text, and sensitive preserves an existing secret when the text is blank.
  // 参数：key 标识配置项，value 是编辑后的文本，sensitive 表示文本为空时保留现有敏感值。
  // Returns: no value; the draft and changed-key state are updated together.
  // 返回值：无；草稿与已变更键状态会被同步更新。
  const updateDraft = React.useCallback((
    key: string,
    value: LuaSkillConfigDraftValue,
    sensitive = false,
  ) => {
    setDraft((current) => ({ ...current, [key]: value }));
    setChangedKeys((current) => {
      // nextChangedKeys is the exact edited-key set after this input event.
      // nextChangedKeys 是当前输入事件后的精确已编辑键集合。
      const nextChangedKeys = sensitive && value === ""
        ? current.filter((candidate) => candidate !== key)
        : current.includes(key) ? current : [...current, key];
      changedKeysRef.current = nextChangedKeys;
      return nextChangedKeys;
    });
    setConflictMessage(null);
  }, []);

  // handleRevisionConflict reloads the latest snapshot while retaining explicit draft edits.
  // handleRevisionConflict 在保留明确草稿编辑的同时重新加载最新快照。
  const handleRevisionConflict = React.useCallback(async (error: unknown): Promise<boolean> => {
    // message is inspected only for the stable upstream CAS error code.
    // message 仅检查稳定上游 CAS 错误码。
    const message = error instanceof Error ? error.message : String(error);
    if (!message.includes("CONFIG_REVISION_CONFLICT")) {
      return false;
    }
    try {
      if (await loadDescriptor(true)) {
        setConflictMessage(t("skills.luaConfig.revisionConflict"));
      }
    } catch (reloadError: unknown) {
      setErrorMessage(reloadError instanceof Error ? reloadError.message : String(reloadError));
    }
    return true;
  }, [loadDescriptor, t]);

  // runExclusive runs one manual operation under the shared duplicate guard, busy state and error report.
  // runExclusive 在共享的防重复守卫、忙碌状态与错误报告下运行一个手动操作。
  //
  // Parameters: operation is the work to run; recoverConflict first offers failures to revision-conflict recovery.
  // 参数：operation 是要运行的工作；recoverConflict 为真时先把失败交给修订冲突恢复处理。
  // Returns: a promise that settles after the operation and its cleanup; a concurrent call does nothing.
  // 返回值：操作及清理完成后结束的 Promise；并发调用不执行任何操作。
  const runExclusive = React.useCallback(async (
    operation: () => Promise<void>,
    recoverConflict: boolean,
  ): Promise<void> => {
    if (mutationRef.current) return;
    mutationRef.current = true;
    setBusy(true);
    setErrorMessage(null);
    try {
      await operation();
    } catch (error: unknown) {
      if (!recoverConflict || !(await handleRevisionConflict(error))) {
        setErrorMessage(error instanceof Error ? error.message : String(error));
      }
    } finally {
      mutationRef.current = false;
      setBusy(false);
    }
  }, [handleRevisionConflict]);

  // reloadDescriptor reloads the package's store from disk, then refreshes the snapshot with
  // consistent busy and error handling, so a change the file watcher missed still becomes visible.
  // reloadDescriptor 先从磁盘重新载入技能包所在存储，再以一致的忙碌状态与错误处理刷新快照，
  // 使文件监听遗漏的变更也能显示。
  //
  // Returns: a promise that settles after the manual refresh completes.
  // 返回值：手动刷新完成后结束的 Promise。
  const reloadDescriptor = React.useCallback(() => runExclusive(async () => {
    await props.actions.onRefreshLuaSkillConfig(descriptor?.storeScope ?? null);
    await loadDescriptor(true);
  }, false), [descriptor?.storeScope, loadDescriptor, props.actions, runExclusive]);

  // submitChangedValues sends one non-empty typed CAS batch and clears sensitive inputs afterward.
  // submitChangedValues 发送一组非空类型化 CAS 批次，并在完成后清空敏感输入。
  const submitChangedValues = React.useCallback(async () => {
    if (!descriptor || changedKeys.length === 0) {
      return;
    }
    await runExclusive(async () => {
      // values contains only fields explicitly changed by the user.
      // values 仅包含用户明确修改的字段。
      const values: Record<string, string | number | boolean> = {};
      for (const key of changedKeys) {
        // item is the authoritative declaration for the changed key.
        // item 是当前变更键的权威声明。
        const item = descriptor.items.find((candidate) => candidate.key === key);
        if (!item) {
          throw new Error(`${key}: declaration is no longer available`);
        }
        values[key] = parseDraftValue(item, draft[key]);
      }
      // mutation is the host-safe result of the completed CAS transaction.
      // mutation 是已完成 CAS 事务的宿主安全结果。
      const mutation = await props.actions.onSetLuaSkillConfig({
        skillId: descriptor.skillId,
        expectedRevision: descriptor.revision,
        values,
      });
      // updated is the post-commit host-safe snapshot.
      // updated 是提交后的宿主安全快照。
      const updated = mutation.descriptor;
      setDescriptor(updated);
      // Clear committed sensitive drafts before the independent validation read can fail.
      // 在独立校验读取可能失败之前清除已提交的敏感草稿。
      setDraft(draftFromDescriptor(updated));
      changedKeysRef.current = [];
      setChangedKeys([]);
      setConflictMessage(null);
      setValidation(null);
      setValidation(await props.actions.onValidateLuaSkillConfig(updated.skillId));
    }, true);
  }, [changedKeys, descriptor, draft, props.actions, runExclusive]);

  // deleteValue removes one explicit or orphaned key with the currently observed revision.
  // deleteValue 使用当前观察到的修订号删除单个显式或遗留键。
  const deleteValue = React.useCallback(async (key: string) => {
    if (!descriptor) {
      return;
    }
    await runExclusive(async () => {
      // mutation is the host-safe result of the completed CAS deletion.
      // mutation 是已完成 CAS 删除的宿主安全结果。
      const mutation = await props.actions.onDeleteLuaSkillConfig({
        skillId: descriptor.skillId,
        key,
        expectedRevision: descriptor.revision,
      });
      // updated is the post-delete host-safe snapshot.
      // updated 是删除后的宿主安全快照。
      const updated = mutation.descriptor;
      setDescriptor(updated);
      // Deleting one key must preserve edits to every other unsaved field.
      // 删除一个键必须保留所有其他尚未保存字段的编辑。
      changedKeysRef.current = changedKeysRef.current.filter((changedKey) => changedKey !== key);
      setChangedKeys(changedKeysRef.current);
      setDraft((current) => {
        const nextDraft = draftFromDescriptor(updated);
        for (const changedKey of changedKeysRef.current) nextDraft[changedKey] = current[changedKey];
        return nextDraft;
      });
      setConflictMessage(null);
      setValidation(null);
      setValidation(await props.actions.onValidateLuaSkillConfig(updated.skillId));
    }, true);
  }, [descriptor, props.actions, runExclusive]);

  // orderedItems is the stable author-directed declaration order.
  // orderedItems 是稳定的作者指定声明顺序。
  const orderedItems = React.useMemo(
    () => orderedConfigItems(descriptor?.items ?? []),
    [descriptor?.items],
  );
  // standardItems contains declarations shown by default.
  // standardItems 包含默认展示的声明。
  const standardItems = orderedItems.filter((item) => !item.advanced);
  // advancedItems contains declarations collapsed behind the advanced section.
  // advancedItems 包含折叠在高级区域后的声明。
  const advancedItems = orderedItems.filter((item) => item.advanced);
  // standardGroups contains visible default declarations partitioned by author group.
  // standardGroups 包含按作者分组划分的默认可见声明。
  const standardGroups = groupConfigItems(standardItems);
  // advancedGroups contains advanced declarations partitioned by author group.
  // advancedGroups 包含按作者分组划分的高级声明。
  const advancedGroups = groupConfigItems(advancedItems);

  // renderItem renders one declaration-driven input and explicit delete action.
  // renderItem 渲染单个声明驱动输入与显式删除动作。
  const renderItem = (item: LuaSkillConfigItem): React.ReactNode => {
    // fieldId provides a stable label association for accessibility.
    // fieldId 为无障碍访问提供稳定标签关联。
    const fieldId = `lua-skill-config-${item.key}`;
    // currentValue is the editor-owned scalar for this declaration.
    // currentValue 是当前声明对应的编辑器自有标量。
    const currentValue = draft[item.key] ?? (item.type === "boolean" ? false : "");
    // inputType selects the semantic browser input for numeric and secret values.
    // inputType 为数值与秘密值选择语义化浏览器输入类型。
    const inputType = item.sensitive || item.format === "password"
      ? "password"
      : item.type === "integer" || item.type === "float"
        ? "number"
        : "text";
    return (
      <div className="lua-skill-config-field" key={item.key}>
        <div className="lua-skill-config-field-heading">
          <label htmlFor={fieldId}>{item.title ?? item.key}</label>
          <div className="lua-skill-config-field-badges">
            <Badge variant="outline">{item.type}</Badge>
            {item.required ? <Badge variant="outline">{t("skills.luaConfig.required")}</Badge> : null}
            {item.sensitive ? <Badge variant="outline">{t("skills.luaConfig.sensitive")}</Badge> : null}
            {item.restartRequired ? <Badge variant="outline">{t("skills.luaConfig.restartRequired")}</Badge> : null}
            {item.deprecated ? <Badge variant="outline">{t("skills.luaConfig.deprecated")}</Badge> : null}
          </div>
        </div>
        <p>{item.description}</p>
        {item.deprecationMessage ? <p className="lua-skill-config-warning">{item.deprecationMessage}</p> : null}
        {item.sensitive && item.state === "configured" ? (
          <span className="lua-skill-config-secret-status">{t("skills.luaConfig.secretConfigured")}</span>
        ) : null}
        {item.type === "boolean" && item.sensitive ? (
          <Select
            disabled={busy}
            onValueChange={(value) => updateDraft(item.key, value === "preserve-secret" ? "" : value, true)}
            value={String(currentValue) || "preserve-secret"}
          >
            <SelectTrigger className="w-full" id={fieldId}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="preserve-secret">{t("skills.luaConfig.preserveSecret")}</SelectItem>
              <SelectItem value="true">true</SelectItem>
              <SelectItem value="false">false</SelectItem>
            </SelectContent>
          </Select>
        ) : item.type === "boolean" ? (
          <Switch
            checked={Boolean(currentValue)}
            disabled={busy}
            id={fieldId}
            onCheckedChange={(checked) => updateDraft(item.key, checked)}
          />
        ) : item.type === "enum" ? (
          <Select
            disabled={busy}
            onValueChange={(value) => updateDraft(item.key, value === "unset-enum" ? "" : value, item.sensitive)}
            value={String(currentValue) || "unset-enum"}
          >
            <SelectTrigger className="w-full" id={fieldId}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="unset-enum">{item.placeholder ?? t("skills.luaConfig.selectValue")}</SelectItem>
              {item.options.map((option) => (
                <SelectItem key={option.value} title={option.description} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : item.format === "multiline" ? (
          <Textarea
            density="large"
            disabled={busy}
            id={fieldId}
            maxLength={item.constraints.max_length}
            minLength={item.constraints.min_length}
            onChange={(event) => updateDraft(item.key, event.target.value, item.sensitive)}
            placeholder={item.placeholder ?? undefined}
            value={String(currentValue)}
          />
        ) : (
          <Input
            disabled={busy}
            id={fieldId}
            max={item.constraints.maximum}
            maxLength={item.constraints.max_length}
            min={item.constraints.minimum}
            minLength={item.constraints.min_length}
            onChange={(event) => updateDraft(item.key, event.target.value, item.sensitive)}
            placeholder={item.placeholder ?? undefined}
            step={item.type === "integer" ? 1 : item.type === "float" ? "any" : undefined}
            type={inputType}
            value={String(currentValue)}
          />
        )}
        <div className="lua-skill-config-field-footer">
          <span>{item.key} · {item.state}</span>
          {item.state === "configured" ? (
            <IconButton
              aria-label={t("skills.luaConfig.deleteValue")}
              disabled={busy}
              onClick={() => void deleteValue(item.key)}
              title={t("skills.luaConfig.deleteValue")}
              type="button"
            >
              <Trash2 size={14} />
            </IconButton>
          ) : null}
        </div>
        {item.validationError ? (
          <p className="lua-skill-config-error">{item.validationError.code}: {item.validationError.message}</p>
        ) : null}
      </div>
    );
  };

  return (
    <Dialog open={props.open && props.skill !== null} onOpenChange={(open) => { if (!mutationRef.current) props.onOpenChange(open); }}>
      <DialogContent className="lua-skill-config-dialog grid-rows-[auto_minmax(0,1fr)_auto]" size="lg" scrollMode="fixed-footer">
        <DialogHeader>
          <DialogTitle>{t("skills.luaConfig.title")}</DialogTitle>
          <DialogDescription>
            {props.skill ? `${props.skill.displayName} · ${props.skill.skillId}` : t("skills.luaConfig.description")}
          </DialogDescription>
        </DialogHeader>
        <div className="lua-skill-config-body min-h-0">
        {busy && !descriptor ? <StreamPlaceholder label={t("skills.luaConfig.loading")} shape="fields" count={4} /> : null}
        {!busy && !descriptor && !errorMessage ? <PageEmptyState title={t("skills.luaConfig.empty")} /> : null}
        {descriptor ? (
          <div className="grid gap-3">
            <div className="lua-skill-config-summary">
              <Badge variant={descriptor.complete ? "secondary" : "outline"}>
                {descriptor.complete ? t("skills.luaConfig.complete") : t("skills.luaConfig.incomplete")}
              </Badge>
              <span>{descriptor.storeScope} · rev {descriptor.revision}</span>
              <IconButton
                aria-label={t("skills.luaConfig.reload")}
                disabled={busy}
                onClick={() => void reloadDescriptor()}
                title={t("skills.luaConfig.reload")}
                type="button"
              >
                <RefreshCw size={14} />
              </IconButton>
            </div>
            {conflictMessage ? <p className="lua-skill-config-warning">{conflictMessage}</p> : null}
            {errorMessage ? <p className="lua-skill-config-error">{errorMessage}</p> : null}
            {configWatchIssues.storage ? <p className="lua-skill-config-error">
              {configWatchIssues.storage}
            </p> : null}
            {configWatchIssues.skill ? <p className="lua-skill-config-error">
              {configWatchIssues.skill}
            </p> : null}
            {validation && !validation.complete ? (
              <div className="lua-skill-config-issues">
                {[...validation.missing, ...validation.invalid, ...validation.businessIssues].map((issue) => (
                  <p key={`${issue.key ?? "package"}:${issue.code}`}>{issue.code}: {issue.message}</p>
                ))}
              </div>
            ) : null}
            {standardGroups.map((group) => (
              <section className="lua-skill-config-group" key={group.label || "__ungrouped_standard"}>
                {group.label ? <h3>{group.label}</h3> : null}
                <div className="lua-skill-config-fields">{group.items.map(renderItem)}</div>
              </section>
            ))}
            {advancedItems.length > 0 ? (
              <details className="lua-skill-config-advanced">
                <summary><Settings2 size={14} /> {t("skills.luaConfig.advanced")}</summary>
                {advancedGroups.map((group) => (
                  <section className="lua-skill-config-group" key={group.label || "__ungrouped_advanced"}>
                    {group.label ? <h3>{group.label}</h3> : null}
                    <div className="lua-skill-config-fields">{group.items.map(renderItem)}</div>
                  </section>
                ))}
              </details>
            ) : null}
            {descriptor.orphaned.length > 0 ? (
              <div className="lua-skill-config-orphaned">
                <strong>{t("skills.luaConfig.orphaned")}</strong>
                {descriptor.orphaned.map((key) => (
                  <div key={key}>
                    <code>{key}</code>
                    <IconButton
                      aria-label={t("skills.luaConfig.deleteValue")}
                      disabled={busy}
                      onClick={() => void deleteValue(key)}
                      title={t("skills.luaConfig.deleteValue")}
                      type="button"
                    >
                      <Trash2 size={14} />
                    </IconButton>
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        ) : errorMessage ? <div role="alert"><p className="lua-skill-config-error">{errorMessage}</p>
          <Button size="sm" variant="outline" disabled={busy} onClick={() => void reloadDescriptor()}>{t("skills.luaConfig.reload")}</Button>
        </div> : null}
        </div>
        <DialogFooter>
          <Button
            disabled={mutationRef.current}
            onClick={() => props.onOpenChange(false)}
            type="button"
            variant="secondary"
          >
            {t("skills.action.cancel")}
          </Button>
          <Button
            disabled={busy || changedKeys.length === 0}
            onClick={() => void submitChangedValues()}
            type="button"
          >
            {busy ? t("skills.luaConfig.saving") : t("skills.luaConfig.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
