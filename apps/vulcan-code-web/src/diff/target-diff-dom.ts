// TargetDiffCodeResizeItem stores ResizeManager state for one rendered code column.
// TargetDiffCodeResizeItem 保存单个已渲染代码列的 ResizeManager 状态。
interface TargetDiffCodeResizeItem {
  // Item kind used to match the target ResizeManager branch.
  // 用于匹配目标 ResizeManager 分支的条目类型。
  type: "code";
  // Code element that owns gutter and content grids.
  // 持有 gutter 与 content 网格的 code 元素。
  codeElement: HTMLElement;
  // First child measured as the line-number gutter element.
  // 作为行号 gutter 测量的第一个子元素。
  numberElement: HTMLElement | null;
  // Last measured code column width.
  // 最近一次测量到的代码列宽度。
  codeWidth: number | "auto";
  // Last measured gutter width.
  // 最近一次测量到的 gutter 宽度。
  numberWidth: number;
}

// TargetDiffAnnotationColumn stores one side of a paired annotation row.
// TargetDiffAnnotationColumn 保存成对注解行中的一侧。
interface TargetDiffAnnotationColumn {
  // Annotation container element.
  // 注解容器元素。
  container: HTMLElement;
  // Direct child whose height is measured.
  // 被测量高度的直接子元素。
  child: HTMLElement;
  // Last measured child height.
  // 最近一次测量到的子元素高度。
  childHeight: number;
}

// TargetDiffAnnotationResizeItem stores height synchronization state for paired annotations.
// TargetDiffAnnotationResizeItem 保存成对注解的高度同步状态。
interface TargetDiffAnnotationResizeItem {
  // Item kind used to match the target ResizeManager branch.
  // 用于匹配目标 ResizeManager 分支的条目类型。
  type: "annotations";
  // First annotation column.
  // 第一侧注解列。
  column1: TargetDiffAnnotationColumn;
  // Second annotation column.
  // 第二侧注解列。
  column2: TargetDiffAnnotationColumn;
  // Last applied shared annotation height.
  // 最近一次应用的共享注解高度。
  currentHeight: number | "auto";
}

// TargetDiffResizeItem is one observed ResizeManager item.
// TargetDiffResizeItem 表示一个被 ResizeManager 观察的条目。
type TargetDiffResizeItem = TargetDiffCodeResizeItem | TargetDiffAnnotationResizeItem;

// TargetDiffResizeUpdate stores one queued resize update for a code item.
// TargetDiffResizeUpdate 保存代码条目的一次排队 resize 更新。
type TargetDiffResizeUpdate = [HTMLElement, number];

// setupTargetDiffDom binds target DOM managers to rendered diff blocks.
// setupTargetDiffDom 将目标 DOM 管理器绑定到已渲染 diff 块。
export function setupTargetDiffDom(
  // Host element that owns rendered diff HTML.
  // 持有已渲染 diff HTML 的宿主元素。
  host: HTMLElement | null,
): (() => void) | undefined {
  if (!host) {
    return undefined;
  }
  // preNodes mirrors the target renderer's per-file diff root scan.
  // preNodes 对齐目标渲染器的单文件 diff 根节点扫描。
  const preNodes = Array.from(host.querySelectorAll<HTMLElement>("pre[data-diff]"));
  if (!preNodes.length) {
    return undefined;
  }
  // managers stores all per-pre manager pairs for deterministic cleanup.
  // managers 保存所有按 pre 创建的管理器对，便于确定性清理。
  const managers: Array<{ resizeManager: TargetDiffResizeManager; scrollSyncManager: TargetDiffSplitScrollSyncManager }> = [];
  for (const preNode of preNodes) {
    // resizeManager mirrors the target ResizeManager instance owned by one rendered file.
    // resizeManager 对齐目标中单个已渲染文件持有的 ResizeManager 实例。
    const resizeManager = new TargetDiffResizeManager();
    // scrollSyncManager mirrors the target split-scroll manager instance.
    // scrollSyncManager 对齐目标中的 split 横向滚动同步管理器实例。
    const scrollSyncManager = new TargetDiffSplitScrollSyncManager();
    resizeManager.setup(preNode, preNode.dataset.overflow === "wrap");
    if (preNode.dataset.diffType === "split" && preNode.dataset.overflow === "scroll") {
      scrollSyncManager.setup(preNode);
    } else {
      scrollSyncManager.cleanUp();
    }
    managers.push({ resizeManager, scrollSyncManager });
  }
  return () => {
    for (const manager of managers) {
      manager.resizeManager.cleanUp();
      manager.scrollSyncManager.cleanUp();
    }
  };
}

// TargetDiffSplitScrollSyncManager mirrors the target De split scroll synchronizer.
// TargetDiffSplitScrollSyncManager 复刻目标 De 分栏滚动同步器。
class TargetDiffSplitScrollSyncManager {
  // Whether the deletion side is currently driving synchronization.
  // 删除侧当前是否正在驱动同步。
  private isDeletionsScrolling = false;
  // Whether the addition side is currently driving synchronization.
  // 新增侧当前是否正在驱动同步。
  private isAdditionsScrolling = false;
  // Debounce timeout used to release the current driving side.
  // 用于释放当前驱动侧的防抖计时器。
  private timeoutId = -1;
  // Deletion code column element.
  // 删除侧 code 列元素。
  private codeDeletions: HTMLElement | undefined;
  // Addition code column element.
  // 新增侧 code 列元素。
  private codeAdditions: HTMLElement | undefined;
  // Whether listeners are currently installed.
  // 当前是否已经安装监听器。
  private enabled = false;

  // cleanUp removes target scroll listeners and resets manager state.
  // cleanUp 移除目标滚动监听并重置管理器状态。
  cleanUp(): void {
    if (!this.enabled) {
      return;
    }
    this.codeDeletions?.removeEventListener("scroll", this.handleDeletionsScroll);
    this.codeAdditions?.removeEventListener("scroll", this.handleAdditionsScroll);
    window.clearTimeout(this.timeoutId);
    this.codeDeletions = undefined;
    this.codeAdditions = undefined;
    this.enabled = false;
  }

  // setup attaches split scroll synchronization to target code columns.
  // setup 将分栏滚动同步绑定到目标 code 列。
  setup(
    // Diff pre root that may contain deletion and addition columns.
    // 可能包含删除列与新增列的 diff pre 根节点。
    preNode: HTMLElement,
    // Optional deletion code column supplied by the caller.
    // 调用方可选传入的删除侧 code 列。
    codeDeletions?: HTMLElement | null,
    // Optional addition code column supplied by the caller.
    // 调用方可选传入的新增侧 code 列。
    codeAdditions?: HTMLElement | null,
  ): void {
    if (codeDeletions == null || codeAdditions == null) {
      for (const childNode of preNode.children ?? []) {
        if (childNode instanceof HTMLElement) {
          if ("deletions" in childNode.dataset) {
            codeDeletions = childNode;
          } else if ("additions" in childNode.dataset) {
            codeAdditions = childNode;
          }
        }
      }
    }
    if (codeAdditions == null || codeDeletions == null) {
      this.cleanUp();
      return;
    }
    if (this.codeDeletions !== codeDeletions) {
      this.codeDeletions?.removeEventListener("scroll", this.handleDeletionsScroll);
      this.codeDeletions = codeDeletions;
      codeDeletions.addEventListener("scroll", this.handleDeletionsScroll, { passive: true });
    }
    if (this.codeAdditions !== codeAdditions) {
      this.codeAdditions?.removeEventListener("scroll", this.handleAdditionsScroll);
      this.codeAdditions = codeAdditions;
      codeAdditions.addEventListener("scroll", this.handleAdditionsScroll, { passive: true });
    }
    this.enabled = true;
  }

  // handleDeletionsScroll mirrors deletion-side horizontal scroll into the addition side.
  // handleDeletionsScroll 将删除侧横向滚动镜像到新增侧。
  private readonly handleDeletionsScroll = () => {
    if (this.isAdditionsScrolling) {
      return;
    }
    this.isDeletionsScrolling = true;
    window.clearTimeout(this.timeoutId);
    this.timeoutId = window.setTimeout(() => {
      this.isDeletionsScrolling = false;
    }, 300);
    this.codeAdditions?.scrollTo({ left: this.codeDeletions?.scrollLeft });
  };

  // handleAdditionsScroll mirrors addition-side horizontal scroll into the deletion side.
  // handleAdditionsScroll 将新增侧横向滚动镜像到删除侧。
  private readonly handleAdditionsScroll = () => {
    if (this.isDeletionsScrolling) {
      return;
    }
    this.isAdditionsScrolling = true;
    window.clearTimeout(this.timeoutId);
    this.timeoutId = window.setTimeout(() => {
      this.isAdditionsScrolling = false;
    }, 300);
    this.codeDeletions?.scrollTo({ left: this.codeAdditions?.scrollLeft });
  };
}

// TargetDiffResizeManager mirrors the target Dn ResizeManager.
// TargetDiffResizeManager 复刻目标 Dn 尺寸管理器。
class TargetDiffResizeManager {
  // observedNodes maps observed DOM nodes back to their resize item.
  // observedNodes 将被观察 DOM 节点映射回对应 resize 条目。
  private readonly observedNodes = new Map<HTMLElement, TargetDiffResizeItem>();
  // queuedUpdates buffers code width updates before applying CSS variables.
  // queuedUpdates 在应用 CSS 变量前缓存代码宽度更新。
  private readonly queuedUpdates = new Map<TargetDiffCodeResizeItem, TargetDiffResizeUpdate[]>();
  // resizeObserver is lazily created just like the target implementation.
  // resizeObserver 与目标实现一致按需延迟创建。
  private resizeObserver: ResizeObserver | undefined;

  // cleanUp disconnects observation and clears pending state.
  // cleanUp 断开观察并清理待处理状态。
  cleanUp(): void {
    this.resizeObserver?.disconnect();
    this.observedNodes.clear();
    this.queuedUpdates.clear();
  }

  // setup scans rendered code columns and annotation pairs under one diff root.
  // setup 扫描单个 diff 根节点下的代码列与成对注解。
  setup(
    // Diff pre root to observe.
    // 要观察的 diff pre 根节点。
    preNode: HTMLElement,
    // Whether the diff is rendered in wrapping mode.
    // diff 是否以自动换行模式渲染。
    wrap: boolean,
  ): void {
    if (typeof ResizeObserver === "undefined") {
      return;
    }
    this.resizeObserver ??= new ResizeObserver(this.handleResizeObserver);
    // codeNodes matches the target querySelectorAll("code") scan.
    // codeNodes 对齐目标 querySelectorAll("code") 扫描。
    const codeNodes = preNode.querySelectorAll<HTMLElement>("code");
    // staleObservedNodes tracks nodes no longer present after the rescan.
    // staleObservedNodes 跟踪重新扫描后不再存在的节点。
    const staleObservedNodes = new Map(this.observedNodes);
    this.observedNodes.clear();
    for (const codeElement of codeNodes) {
      // existingItem is reused when a code element was already observed.
      // existingItem 在 code 元素已被观察时复用。
      let existingItem = staleObservedNodes.get(codeElement);
      if (existingItem != null && existingItem.type !== "code") {
        throw Error("ResizeManager.setup: somehow a code node is being used for an annotation, should be impossible");
      }
      // numberElement mirrors target's firstElementChild gutter measurement.
      // numberElement 对齐目标使用 firstElementChild 测量 gutter 的做法。
      let numberElement: Element | null = codeElement.firstElementChild;
      if (!(numberElement instanceof HTMLElement)) {
        numberElement = null;
      }
      if (existingItem == null) {
        existingItem = { type: "code", codeElement, numberElement, codeWidth: "auto", numberWidth: 0 };
        this.observedNodes.set(codeElement, existingItem);
        this.resizeObserver.observe(codeElement);
        if (numberElement != null) {
          this.observedNodes.set(numberElement, existingItem);
          this.resizeObserver.observe(numberElement);
        }
      } else {
        this.observedNodes.set(codeElement, existingItem);
        staleObservedNodes.delete(codeElement);
        if (existingItem.numberElement === numberElement) {
          if (existingItem.numberElement != null) {
            staleObservedNodes.delete(existingItem.numberElement);
            this.observedNodes.set(existingItem.numberElement, existingItem);
          }
        } else {
          if (existingItem.numberElement != null) {
            this.resizeObserver.unobserve(existingItem.numberElement);
          }
          if (numberElement != null) {
            this.resizeObserver.observe(numberElement);
            staleObservedNodes.delete(numberElement);
            this.observedNodes.set(numberElement, existingItem);
          }
          existingItem.numberElement = numberElement;
        }
      }
    }
    if (codeNodes.length > 1 && !wrap) {
      this.setupAnnotationPairs(preNode, staleObservedNodes);
    }
    for (const staleNode of staleObservedNodes.keys()) {
      if (staleNode.isConnected) {
        staleNode.style.removeProperty("--diffs-column-content-width");
        staleNode.style.removeProperty("--diffs-column-number-width");
        staleNode.style.removeProperty("--diffs-column-width");
        if (staleNode.parentElement instanceof HTMLElement) {
          staleNode.parentElement.style.removeProperty("--diffs-annotation-min-height");
        }
      }
      this.resizeObserver.unobserve(staleNode);
    }
    staleObservedNodes.clear();
  }

  // setupAnnotationPairs mirrors target paired annotation height synchronization.
  // setupAnnotationPairs 复刻目标成对注解高度同步。
  private setupAnnotationPairs(
    // Diff pre root containing annotation containers.
    // 包含注解容器的 diff pre 根节点。
    preNode: HTMLElement,
    // Remaining stale nodes from the current setup scan.
    // 当前 setup 扫描后剩余的过期节点。
    staleObservedNodes: Map<HTMLElement, TargetDiffResizeItem>,
  ): void {
    if (!this.resizeObserver) {
      return;
    }
    // annotationNodes matches target's data-line-annotation pair scan.
    // annotationNodes 对齐目标 data-line-annotation 成对扫描。
    const annotationNodes = preNode.querySelectorAll<HTMLElement>("[data-line-annotation*=',']");
    // annotationPairs groups annotation containers by their pair id.
    // annotationPairs 按成对标识分组注解容器。
    const annotationPairs = new Map<string, HTMLElement[]>();
    for (const annotationNode of annotationNodes) {
      // lineAnnotation is the target pair id stored on dataset.
      // lineAnnotation 是目标存储在 dataset 上的成对标识。
      const { lineAnnotation = "" } = annotationNode.dataset;
      if (!/^\d+,\d+$/.test(lineAnnotation)) {
        console.error("DiffFileRenderer.setupResizeObserver: Invalid element or annotation", {
          lineAnnotation,
          element: annotationNode,
        });
        continue;
      }
      // pairNodes holds the two annotation containers for one pair id.
      // pairNodes 保存同一个成对标识下的两个注解容器。
      let pairNodes = annotationPairs.get(lineAnnotation);
      if (pairNodes == null) {
        pairNodes = [];
        annotationPairs.set(lineAnnotation, pairNodes);
      }
      pairNodes.push(annotationNode);
    }
    for (const [lineAnnotation, pairNodes] of annotationPairs) {
      if (pairNodes.length !== 2) {
        console.error("DiffFileRenderer.setupResizeObserver: Bad Pair", lineAnnotation, pairNodes);
        continue;
      }
      // column1Container and column2Container are the two paired annotation containers.
      // column1Container 与 column2Container 是两个成对注解容器。
      const [column1Container, column2Container] = pairNodes;
      // column1Child is the measured child on the first annotation side.
      // column1Child 是第一侧注解被测量的子元素。
      const column1Child = column1Container.firstElementChild;
      // column2Child is the measured child on the second annotation side.
      // column2Child 是第二侧注解被测量的子元素。
      const column2Child = column2Container.firstElementChild;
      if (!(column1Child instanceof HTMLElement) || !(column2Child instanceof HTMLElement)) {
        continue;
      }
      // existingItem reuses a previously observed annotation item.
      // existingItem 复用此前已观察的注解条目。
      let existingItem = staleObservedNodes.get(column1Child);
      if (existingItem != null) {
        this.observedNodes.set(column1Child, existingItem);
        this.observedNodes.set(column2Child, existingItem);
        staleObservedNodes.delete(column1Child);
        staleObservedNodes.delete(column2Child);
        continue;
      }
      existingItem = {
        type: "annotations",
        column1: { container: column1Container, child: column1Child, childHeight: column1Child.getBoundingClientRect().height },
        column2: { container: column2Container, child: column2Child, childHeight: column2Child.getBoundingClientRect().height },
        currentHeight: "auto",
      };
      // sharedHeight is the current maximum height across the pair.
      // sharedHeight 是当前成对两侧的最大高度。
      const sharedHeight = Math.max(existingItem.column1.childHeight, existingItem.column2.childHeight);
      this.applyNewHeight(existingItem, sharedHeight);
      this.observedNodes.set(column1Child, existingItem);
      this.observedNodes.set(column2Child, existingItem);
      this.resizeObserver.observe(column1Child);
      this.resizeObserver.observe(column2Child);
    }
  }

  // handleResizeObserver queues target resize entries and flushes column changes.
  // handleResizeObserver 排队目标 resize 条目并刷新列宽变化。
  private readonly handleResizeObserver = (entries: ResizeObserverEntry[]) => {
    for (const entry of entries) {
      // targetNode is the observed element reported by ResizeObserver.
      // targetNode 是 ResizeObserver 返回的被观察元素。
      const targetNode = entry.target;
      if (!(targetNode instanceof HTMLElement)) {
        console.error("FileDiff.handleResizeObserver: Invalid element for ResizeObserver", entry);
        continue;
      }
      // observedItem is the manager state associated with the target node.
      // observedItem 是目标节点关联的管理器状态。
      const observedItem = this.observedNodes.get(targetNode);
      if (observedItem == null) {
        console.error("FileDiff.handleResizeObserver: Not a valid observed node", entry);
        continue;
      }
      if (observedItem.type === "annotations") {
        // annotationColumn identifies which side of the pair resized.
        // annotationColumn 标识成对注解中哪一侧发生了尺寸变化。
        const annotationColumn = targetNode === observedItem.column1.child
          ? observedItem.column1
          : targetNode === observedItem.column2.child
            ? observedItem.column2
            : undefined;
        if (annotationColumn == null) {
          console.error("FileDiff.handleResizeObserver: Couldn't find a column for", { item: observedItem, target: targetNode });
          continue;
        }
        annotationColumn.childHeight = resizeObserverBlockSize(entry);
        // sharedHeight is the new maximum annotation height.
        // sharedHeight 是新的注解最大高度。
        const sharedHeight = Math.max(observedItem.column1.childHeight, observedItem.column2.childHeight);
        this.applyNewHeight(observedItem, sharedHeight);
      } else if (observedItem.type === "code") {
        // updates is the per-code queue of inline-size measurements.
        // updates 是每个 code 的 inline-size 测量队列。
        const updates = this.queuedUpdates.get(observedItem) ?? [];
        updates.push([targetNode, resizeObserverInlineSize(entry)]);
        this.queuedUpdates.set(observedItem, updates);
      }
    }
    this.handleColumnChange();
  };

  // handleColumnChange applies queued target CSS width variables.
  // handleColumnChange 应用排队的目标 CSS 宽度变量。
  private handleColumnChange(): void {
    for (const [resizeItem, updates] of this.queuedUpdates) {
      for (const [targetNode, inlineSize] of updates) {
        if (targetNode === resizeItem.codeElement) {
          // codeWidth is the floored content width of the code column.
          // codeWidth 是向下取整后的 code 列内容宽度。
          const codeWidth = Math.max(Math.floor(inlineSize), 0);
          if (codeWidth !== resizeItem.codeWidth) {
            // contentWidth subtracts the measured gutter width from the code width.
            // contentWidth 从 code 宽度中扣除已测量的 gutter 宽度。
            const contentWidth = Math.max(codeWidth - resizeItem.numberWidth, 0);
            resizeItem.codeWidth = codeWidth === 0 ? "auto" : codeWidth;
            resizeItem.codeElement.style.setProperty("--diffs-column-content-width", `${contentWidth > 0 ? `${contentWidth}px` : "auto"}`);
            resizeItem.codeElement.style.setProperty(
              "--diffs-column-width",
              `${typeof resizeItem.codeWidth === "number" ? `${resizeItem.codeWidth}px` : "auto"}`,
            );
          }
          if (resizeItem.numberElement != null && typeof resizeItem.codeWidth === "number" && resizeItem.numberWidth === 0) {
            updates.push([resizeItem.numberElement, resizeItem.numberElement.getBoundingClientRect().width]);
          }
        } else if (targetNode === resizeItem.numberElement) {
          // numberWidth is the ceiled gutter width.
          // numberWidth 是向上取整后的 gutter 宽度。
          const numberWidth = Math.max(Math.ceil(inlineSize), 0);
          if (numberWidth !== resizeItem.numberWidth) {
            resizeItem.numberWidth = numberWidth;
            resizeItem.codeElement.style.setProperty(
              "--diffs-column-number-width",
              `${resizeItem.numberWidth === 0 ? "auto" : `${resizeItem.numberWidth}px`}`,
            );
            if (resizeItem.codeWidth !== "auto") {
              // contentWidth is recalculated after the gutter width changes.
              // contentWidth 在 gutter 宽度变化后重新计算。
              const contentWidth = Math.max(resizeItem.codeWidth - resizeItem.numberWidth, 0);
              resizeItem.codeElement.style.setProperty("--diffs-column-content-width", `${contentWidth === 0 ? "auto" : `${contentWidth}px`}`);
            }
          }
        }
      }
    }
    this.queuedUpdates.clear();
  }

  // applyNewHeight applies a shared target annotation minimum height.
  // applyNewHeight 应用目标注解共享最小高度。
  private applyNewHeight(
    // Annotation resize item to update.
    // 要更新的注解 resize 条目。
    item: TargetDiffAnnotationResizeItem,
    // Shared height in pixels.
    // 共享高度，单位像素。
    height: number,
  ): void {
    if (height === item.currentHeight) {
      return;
    }
    item.currentHeight = Math.max(height, 0);
    item.column1.container.style.setProperty("--diffs-annotation-min-height", `${item.currentHeight}px`);
    item.column2.container.style.setProperty("--diffs-annotation-min-height", `${item.currentHeight}px`);
  }
}

// resizeObserverInlineSize reads the target inline-size from a ResizeObserver entry.
// resizeObserverInlineSize 从 ResizeObserver 条目读取目标 inline-size。
function resizeObserverInlineSize(
  // ResizeObserver entry to inspect.
  // 要检查的 ResizeObserver 条目。
  entry: ResizeObserverEntry,
): number {
  // contentBoxSize is the same source used by the target ResizeManager.
  // contentBoxSize 与目标 ResizeManager 使用的来源一致。
  const contentBoxSize = Array.isArray(entry.contentBoxSize) ? entry.contentBoxSize[0] : entry.contentBoxSize;
  return contentBoxSize?.inlineSize ?? entry.contentRect.width;
}

// resizeObserverBlockSize reads the target block-size from a ResizeObserver entry.
// resizeObserverBlockSize 从 ResizeObserver 条目读取目标 block-size。
function resizeObserverBlockSize(
  // ResizeObserver entry to inspect.
  // 要检查的 ResizeObserver 条目。
  entry: ResizeObserverEntry,
): number {
  // borderBoxSize is the same source used by the target ResizeManager for annotations.
  // borderBoxSize 与目标 ResizeManager 对注解使用的来源一致。
  const borderBoxSize = Array.isArray(entry.borderBoxSize) ? entry.borderBoxSize[0] : entry.borderBoxSize;
  return borderBoxSize?.blockSize ?? entry.target.getBoundingClientRect().height;
}
