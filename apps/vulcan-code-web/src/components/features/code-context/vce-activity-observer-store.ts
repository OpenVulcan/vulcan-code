import type { VceProjectActivityResponse, VceProjectChangedEvent } from "@/types";
import type { ServiceRequestOptions } from "@/service/protocol";
import {
  VCE_OBSERVER_READ_DEADLINE_MS,
  VCE_OBSERVER_READ_TIMEOUT_MESSAGE,
} from "./vce-observer-timeouts";

// Immutable lightweight metadata shared by the visible surfaces of one project.
// 同一项目可见界面共享的不可变轻量元数据。
export interface VceObservedActivity {
  // Last validated host activity; absent until this observation lifetime obtains a response.
  // 最近校验的宿主活动；此观察生命周期取得响应前为空。
  readonly activity: VceProjectActivityResponse | null;
  // Read or subscription error, never interpreted as a saved project switch.
  // 读取或订阅错误，绝不解释为已保存的项目开关。
  readonly error: string | null;
}

// Read-only host transport; no indexing or model-execution capability is registered here.
// 只读宿主传输；此处不注册索引或模型执行能力。
export interface VceActivityObserverTransport {
  // Read projectId's lightweight activity; resolves to its authoritative metadata.
  // 读取 projectId 的轻量活动；完成时返回其权威元数据。
  load(projectId: string, options?: ServiceRequestOptions): Promise<VceProjectActivityResponse>;
  // Subscribe to invalidations through listener; resolves to the exact unsubscribe function.
  // 通过 listener 订阅失效通知；完成时返回精确退订函数。
  subscribe(listener: (event: VceProjectChangedEvent) => void): Promise<() => void>;
}

// Stable empty identity used by server rendering and unobserved projects.
// 服务端渲染及未观察项目使用的稳定空身份。
export const EMPTY_VCE_ACTIVITY: VceObservedActivity = Object.freeze({ activity: null, error: null });

// One reference-counted project observation, including its single bounded read chain.
// 一个引用计数项目观察，包括其唯一有界读取链。
interface ActivityObservation {
  // Exact workbench project owner.
  // 精确工作台项目所有者。
  projectId: string;
  // Last immutable projection delivered to React.
  // 最近交付 React 的不可变投影。
  snapshot: VceObservedActivity;
  // Mounted views interested in this project only.
  // 仅关心本项目的已挂载视图。
  listeners: Set<() => void>;
  // One recovery poll per observed project.
  // 每个被观察项目的唯一恢复轮询。
  timer: ReturnType<typeof setInterval>;
  // Active read chain, shared by simultaneous refresh requests.
  // 活动读取链，由同时刷新的请求共享。
  reading: Promise<void> | null;
  // readAbortController owns the current read and is aborted when the observation retires.
  // readAbortController 持有当前读取，并在观察生命周期退役时中止读取。
  readAbortController: AbortController | null;
  // readDeadlineTimer is cleared when an observer retires before its read settles.
  // readDeadlineTimer 在观察者于读取完成前退役时被清理。
  readDeadlineTimer: ReturnType<typeof setTimeout> | null;
  // A later invalidation requires one follow-up read before publishing a response.
  // 后续失效要求发布响应前再读取一次。
  dirty: boolean;
  // Project read error retained separately from stream availability.
  // 与事件流可用性分开保留的项目读取错误。
  readError: string | null;
}

// Exact stream lifetime; late registration completion must release only its own handle.
// 精确事件流生命周期；迟到的注册完成必须仅释放自己的句柄。
interface ActivityRegistration {
  // Absent while host subscription registration is pending.
  // 宿主订阅注册等待期间为空。
  stop: (() => void) | null;
}

// Workbench-owned activity cache; only mounted project observers initiate metadata reads.
// 工作台持有的活动缓存；只有已挂载项目观察者才发起元数据读取。
export class VceActivityObserverStore {
  // Active observation lifetimes keyed by stable project ID.
  // 按稳定项目编号索引的活动观察生命周期。
  private readonly projects = new Map<string, ActivityObservation>();
  // Shared project-invalidation stream, independent from the job-progress stream.
  // 共享项目失效事件流，独立于作业进度事件流。
  private registration: ActivityRegistration | null = null;
  // Stream errors remain visible even if a metadata read succeeds.
  // 即使元数据读取成功，事件流错误仍保持可见。
  private subscriptionError: string | null = null;

  // Construct an inert cache from transport; returns its instance without performing IO.
  // 从 transport 构造静止缓存；返回实例，不执行输入输出。
  constructor(private readonly transport: VceActivityObserverTransport) {}

  // Return projectId's immutable snapshot, or the stable empty snapshot without any IO.
  // 返回 projectId 的不可变快照或稳定空快照，不执行任何输入输出。
  snapshot(projectId: string | null): VceObservedActivity {
    return projectId === null ? EMPTY_VCE_ACTIVITY : this.projects.get(projectId)?.snapshot ?? EMPTY_VCE_ACTIVITY;
  }

  // Observe projectId through listener; returns an idempotent release, never a host-job cancellation.
  // 通过 listener 观察 projectId；返回幂等释放函数，绝不取消宿主作业。
  subscribe(projectId: string | null, listener: () => void): () => void {
    if (projectId === null) return () => undefined;
    // existing prevents each UI surface from starting its own polling chain.
    // existing 防止各界面启动自己的轮询链。
    const existing = this.projects.get(projectId);
    // entry is also the ownership fence for close/reopen and A-to-B-to-A navigation.
    // entry 同时隔离关闭重开及甲到乙再到甲的导航归属。
    const entry: ActivityObservation = existing ?? {
      projectId, snapshot: EMPTY_VCE_ACTIVITY, listeners: new Set(), reading: null,
      readAbortController: null, readDeadlineTimer: null, dirty: false, readError: null,
      timer: setInterval(() => {
        if (!this.owns(entry)) return;
        this.ensureRegistration();
        // Polling does not invalidate a slow in-flight read; actual events still request a follow-up.
        // 轮询不使慢速在途读取失效；真实事件仍请求后续读取。
        if (entry.reading === null) void this.refresh(projectId);
      }, 5000),
    };
    this.projects.set(projectId, entry);
    entry.listeners.add(listener);
    this.ensureRegistration();
    if (!existing) void this.refresh(projectId);
    // released makes duplicate cleanup harmless to a replacement observation.
    // released 使重复清理不影响替代观察。
    let released = false;
    return () => {
      if (released) return;
      released = true;
      entry.listeners.delete(listener);
      if (!this.owns(entry) || entry.listeners.size !== 0) return;
      clearInterval(entry.timer);
      // Retiring an observer releases the browser request without cancelling the host job.
      // 观察退役时释放浏览器请求，但不取消宿主作业。
      if (entry.readDeadlineTimer !== null) {
        clearTimeout(entry.readDeadlineTimer);
        entry.readDeadlineTimer = null;
      }
      entry.readAbortController?.abort();
      this.projects.delete(projectId);
      if (this.projects.size !== 0) return;
      // retired is detached before invoking user code in the unsubscribe callback.
      // retired 在退订回调调用外部代码前脱离归属。
      const retired = this.registration;
      this.registration = null;
      this.subscriptionError = null;
      retired?.stop?.();
    };
  }

  // Invalidate and reread projectId without creating an unobserved project; resolves after the shared chain.
  // 使 projectId 失效并重读，不创建未被观察项目；共享读取链结束后完成。
  refresh(projectId: string | null): Promise<void> {
    // entry belongs only to an already mounted observer.
    // entry 仅属于已挂载观察者。
    const entry = projectId === null ? undefined : this.projects.get(projectId);
    if (!entry) return Promise.resolve();
    entry.dirty = true;
    if (entry.reading) return entry.reading;
    // request begins in a microtask so synchronous failures cannot race installation of its identity.
    // request 在微任务中开始，使同步失败不会与其身份安装竞争。
    const request = Promise.resolve().then(async () => {
      try {
        while (this.owns(entry) && entry.dirty) {
          entry.dirty = false;
          // controller owns this read's abort signal and never controls host indexing.
          // controller 持有本次读取的中止信号，绝不控制宿主索引。
          const controller = new AbortController();
          // timedOut distinguishes a user-visible deadline from a silent unmount abort.
          // timedOut 区分用户可见的期限超时与静默卸载中止。
          let timedOut = false;
          entry.readAbortController = controller;
          // deadline bounds this read and is cleared after completion or retirement.
          // deadline 限制本次读取，并在完成或退役后清理。
          const deadline = setTimeout(() => {
            timedOut = true;
            controller.abort();
          }, VCE_OBSERVER_READ_DEADLINE_MS);
          entry.readDeadlineTimer = deadline;
          try {
            // activity is discarded if an event invalidated this read or its owner was released.
            // activity 在事件使此读取失效或所有者被释放时丢弃。
            const activity = await this.transport.load(entry.projectId, { signal: controller.signal });
            if (controller.signal.aborted) {
              if (timedOut && this.owns(entry) && !entry.dirty) {
                entry.readError = VCE_OBSERVER_READ_TIMEOUT_MESSAGE;
                this.publish(entry);
              }
              continue;
            }
            if (!this.owns(entry)) return;
            if (activity.status.projectId !== entry.projectId) throw new Error("语义状态返回了其他项目的数据。");
            if (!entry.dirty) {
              entry.readError = null;
              this.publish(entry, activity);
            }
          } catch (error) {
            // An unmount is silent transport control; a deadline publishes a retryable diagnostic.
            // 卸载属于静默传输控制；达到期限时发布可重试诊断。
            if (controller.signal.aborted) {
              if (timedOut && this.owns(entry) && !entry.dirty) {
                entry.readError = VCE_OBSERVER_READ_TIMEOUT_MESSAGE;
                this.publish(entry);
              }
              continue;
            }
            if (this.owns(entry) && !entry.dirty) {
              entry.readError = error instanceof Error ? error.message : String(error);
              this.publish(entry);
            }
          } finally {
            clearTimeout(deadline);
            if (entry.readDeadlineTimer === deadline) entry.readDeadlineTimer = null;
            if (entry.readAbortController === controller) entry.readAbortController = null;
          }
        }
      } finally { if (entry.reading === request) entry.reading = null; }
    });
    entry.reading = request;
    return request;
  }

  // Check entry's exact observation lifetime; returns false after release even when its ID is reused.
  // 检查 entry 的精确观察生命周期；释放后返回否，即使其编号被复用。
  private owns(entry: ActivityObservation): boolean { return this.projects.get(entry.projectId) === entry; }

  // Register one stream; retries only on a later mounted subscription or poll. Returns nothing.
  // 注册唯一事件流；仅在后续挂载订阅或轮询时重试。无返回值。
  private ensureRegistration(): void {
    if (this.registration || this.projects.size === 0) return;
    // registration freezes the identity checked by callbacks and asynchronous completion.
    // registration 固定回调及异步完成所检查的身份。
    const registration: ActivityRegistration = { stop: null };
    this.registration = registration;
    void Promise.resolve().then(async () => {
      if (this.registration !== registration) return;
      // stop must be released even when all views unmount before registration completes.
      // stop 即使在注册完成前全部视图已卸载，也必须释放。
      const stop = await this.transport.subscribe((event) => {
        if (this.registration === registration) void this.refresh(event.projectId);
      });
      if (this.registration !== registration) { stop(); return; }
      registration.stop = stop;
      this.subscriptionError = null;
      for (const entry of this.projects.values()) {
        this.publish(entry);
        // Reread after subscription acknowledgement to close the initial snapshot/registration gap.
        // 订阅确认后重读，补齐初始快照与注册之间的空窗。
        void this.refresh(entry.projectId);
      }
    }).catch((error) => {
      if (this.registration !== registration) return;
      this.registration = null;
      this.subscriptionError = error instanceof Error ? error.message : String(error);
      for (const entry of this.projects.values()) this.publish(entry);
    });
  }

  // Deliver activity and explicit errors to entry's listeners only; returns nothing.
  // 仅向 entry 的监听者交付 activity 和明确错误；无返回值。
  private publish(entry: ActivityObservation, activity = entry.snapshot.activity): void {
    // error preserves project-read precedence without erasing an unrelated stream failure.
    // error 保留项目读取优先级，不擦除无关事件流失败。
    const error = entry.readError ?? this.subscriptionError;
    if (entry.snapshot.activity === activity && entry.snapshot.error === error) return;
    entry.snapshot = { activity, error };
    for (const listener of entry.listeners) listener();
  }
}
