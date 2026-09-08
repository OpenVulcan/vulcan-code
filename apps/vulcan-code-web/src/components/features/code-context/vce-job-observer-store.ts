import type { VceJobSummary } from "@/types";
import type { ServiceRequestOptions } from "@/service/protocol";
import {
  VCE_OBSERVER_READ_DEADLINE_MS,
  VCE_OBSERVER_READ_TIMEOUT_MESSAGE,
} from "./vce-observer-timeouts";

// Expose immutable job observations shared by all mounted views of the same project.
// 暴露同一项目所有已挂载视图共享的不可变作业观察。
export interface VceObservedJob {
  // job is the latest durable receipt, or null before one has been observed.
  // job 为最近持久回执，尚未观察到时为空。
  readonly job: VceJobSummary | null;
  // error preserves an explicit transport or command failure without erasing the receipt.
  // error 保留明确的传输或命令失败，不擦除回执。
  readonly error: string | null;
  // cancelPending belongs only to the exact currently displayed job.
  // cancelPending 仅属于当前展示的精确作业。
  readonly cancelPending: boolean;
}

// Inject existing host transport without giving the observer permission to create indexing jobs.
// 注入既有宿主传输，不赋予观察器创建索引作业的权限。
export interface VceJobObserverTransport {
  // Read projectId's latest receipt; returns null only when the host has no receipt.
  // 读取 projectId 的最近回执；仅在宿主没有回执时返回空值。
  load(projectId: string, options?: ServiceRequestOptions): Promise<VceJobSummary | null>;
  // Deliver durable receipts to listener; returns the exact stream disposal function.
  // 向 listener 投递持久回执；返回精确的流释放函数。
  subscribe(listener: (receipt: VceJobSummary) => void): Promise<() => void>;
  // Cancel the explicit projectId/jobId pair; returns that job's updated receipt.
  // 取消明确的 projectId/jobId 组合；返回该作业的更新回执。
  cancel(projectId: string, jobId: string): Promise<VceJobSummary>;
}

// Keep a stable allocation-free snapshot for absent projects and server rendering.
// 为无项目和服务端渲染保留稳定且不重复分配的快照。
export const EMPTY_VCE_OBSERVED_JOB: VceObservedJob = Object.freeze({ job: null, error: null, cancelPending: false });

// Accept only newer receipts for projectId, including events that overtake a read.
// 仅接收 projectId 的较新回执，包括先于读取到达的事件。
// current/incoming are observed/durable receipts. Returns the newer receipt; null never erases one.
// current/incoming 为已观察／持久回执。返回较新回执；空值绝不擦除已有回执。
export function mergeVceJobReceipt(current: VceJobSummary | null, incoming: VceJobSummary | null, projectId: string): VceJobSummary | null {
  if (incoming === null) return current;
  if (incoming.projectId !== projectId) throw new Error("语义索引作业返回了其他项目的数据。");
  if (current === null || current.projectId !== projectId) return incoming;
  if (current.jobId === incoming.jobId) return incoming.sequence > current.sequence ? incoming : current;
  if (incoming.createdAtUnixMs !== current.createdAtUnixMs) {
    return incoming.createdAtUnixMs > current.createdAtUnixMs ? incoming : current;
  }
  return incoming.jobId > current.jobId ? incoming : current;
}

// Determine execution independently from query eligibility; job is the latest optional receipt.
// 独立于查询资格判断执行状态；job 为可选的最近回执。
// Returns whether the job is queued, executing, or cancelling.
// 返回作业是否排队、执行或取消中。
export function isVceJobActive(job: VceJobSummary | null): boolean {
  return job !== null && (job.state === "queued" || job.state === "running" || job.state === "cancelling");
}

// Retain one observation lifetime per mounted project; removal invalidates every pending callback.
// 为每个已挂载项目保留一个观察生命周期；移除时使全部待完成回调失效。
interface ProjectJobObservation {
  // projectId identifies the exact host project, never the active terminal or session.
  // projectId 标识精确宿主项目，绝非活动终端或会话。
  projectId: string;
  // snapshot is replaced only when the public projection changes.
  // snapshot 仅在公共投影变化时替换。
  snapshot: VceObservedJob;
  // listeners contains only consumers of this project.
  // listeners 仅包含此项目的消费者。
  listeners: Set<() => void>;
  // timer bounds read recovery to one cadence per project.
  // timer 将读取恢复限制为每项目一个节拍。
  timer: ReturnType<typeof setInterval>;
  // reading coalesces overlapping polls without queuing a busy follow-up loop.
  // reading 合并重叠轮询，不排入忙碌的后续循环。
  reading: boolean;
  // readAbortController owns the current read and is aborted when the observation retires.
  // readAbortController 持有当前读取，并在观察生命周期退役时中止读取。
  readAbortController: AbortController | null;
  // readDeadlineTimer is cleared when an observer retires before its read settles.
  // readDeadlineTimer 在观察者于读取完成前退役时被清理。
  readDeadlineTimer: ReturnType<typeof setTimeout> | null;
  // readError survives unrelated events until an authoritative read succeeds.
  // readError 保留至权威读取成功，不被无关事件清除。
  readError: string | null;
  // actionFailure belongs to a precise still-active job, not a later replacement.
  // actionFailure 属于精确的仍活动作业，而非后续替代作业。
  actionFailure: { jobId: string; message: string } | null;
  // cancellation deduplicates a precise command across simultaneous views.
  // cancellation 在同时显示的视图之间去重精确命令。
  cancellation: { jobId: string; request: Promise<void> } | null;
}

// Fence asynchronous registration and callbacks by exact stream lifetime.
// 以精确流生命周期隔离异步注册和回调。
interface JobEventRegistration {
  // stop is absent until asynchronous subscription registration succeeds.
  // stop 在异步订阅注册成功之前为空。
  stop: (() => void) | null;
}

// Own one stream and one read chain per visible project, independently from React render frequency.
// 持有一个事件流及每可见项目一条读取链，独立于 React 渲染频率。
export class VceJobObserverStore {
  // projects contains only currently observed project lifetimes.
  // projects 仅包含当前被观察的项目生命周期。
  private readonly projects = new Map<string, ProjectJobObservation>();
  // registration prevents late callbacks from an obsolete event stream being applied.
  // registration 防止应用过期事件流的迟到回调。
  private registration: JobEventRegistration | null = null;
  // subscriptionError is independent of per-project read success.
  // subscriptionError 独立于各项目读取成功状态。
  private subscriptionError: string | null = null;

  // Construct an inert store from transport; construction performs no IO and returns the instance.
  // 从 transport 构造静止状态仓库；构造不执行输入输出，返回实例。
  constructor(private readonly transport: VceJobObserverTransport) {}

  // Read projectId's immutable projection without allocating or scheduling IO.
  // 读取 projectId 的不可变投影，不分配对象或调度输入输出。
  // Returns the empty projection for an absent or unobserved project.
  // 对无项目或未被观察项目返回空投影。
  snapshot(projectId: string | null): VceObservedJob {
    return projectId === null ? EMPTY_VCE_OBSERVED_JOB : this.projects.get(projectId)?.snapshot ?? EMPTY_VCE_OBSERVED_JOB;
  }

  // Observe projectId through listener; returns an idempotent release that never cancels host work.
  // 通过 listener 观察 projectId；返回幂等释放函数，绝不取消宿主工作。
  subscribe(projectId: string | null, listener: () => void): () => void {
    if (projectId === null) return () => undefined;
    // existing shares current state and IO with another view of this exact project.
    // existing 与此精确项目的另一视图共享当前状态和输入输出。
    const existing = this.projects.get(projectId);
    // entry identity is the lifecycle fence, including an A-to-B-to-A navigation.
    // entry 身份为生命周期隔离标识，包含甲到乙再到甲的导航。
    const entry: ProjectJobObservation = existing ?? {
      projectId, snapshot: EMPTY_VCE_OBSERVED_JOB, listeners: new Set(), reading: false,
      readError: null, actionFailure: null, cancellation: null,
      readAbortController: null, readDeadlineTimer: null,
      timer: setInterval(() => {
        if (!this.owns(entry)) return;
        this.ensureRegistration();
        void this.reload(entry);
      }, 5000),
    };
    this.projects.set(projectId, entry);
    entry.listeners.add(listener);
    this.publish(entry);
    this.ensureRegistration();
    if (!existing) void this.reload(entry);
    // released makes duplicate React cleanup harmless to later subscribers.
    // released 使重复 React 清理不影响后续订阅者。
    let released = false;
    return () => {
      if (released) return;
      released = true;
      entry.listeners.delete(listener);
      if (!this.owns(entry) || entry.listeners.size > 0) return;
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
      // retired must be detached before external unsubscribe can dispatch a final callback.
      // retired 必须在外部退订可能投递最后回调之前脱离归属。
      const retired = this.registration;
      this.registration = null;
      this.subscriptionError = null;
      retired?.stop?.();
    };
  }

  // Apply an explicit command receipt for projectId; returns nothing and never resurrects a view.
  // 应用 projectId 的明确命令回执；无返回值且绝不复活视图。
  receive(projectId: string, receipt: VceJobSummary): void {
    if (receipt.projectId !== projectId) throw new Error("索引回执项目身份不匹配。");
    // entry is absent after final unmount, which must discard the command's delayed UI receipt.
    // entry 在最后一次卸载后不存在，此时必须丢弃命令迟到的界面回执。
    const entry = this.projects.get(projectId);
    if (entry) this.accept(entry, receipt);
  }

  // Cancel projectId/jobId as actually displayed; returns the shared command or a no-op if retired.
  // 取消实际展示的 projectId/jobId；返回共享命令，已退役则为空操作。
  cancel(projectId: string, jobId: string): Promise<void> {
    // entry and job capture authority before asynchronous transport begins.
    // entry 与 job 在异步传输开始前捕获权威归属。
    const entry = this.projects.get(projectId);
    const job = entry?.snapshot.job;
    if (!entry || !job || job.jobId !== jobId || !isVceJobActive(job)) return Promise.resolve();
    if (entry.cancellation?.jobId === job.jobId) return entry.cancellation.request;
    entry.actionFailure = null;
    // request starts in a microtask so its identity is installed before even synchronous failures.
    // request 在微任务中启动，使其身份在同步失败发生之前也已安装。
    const request = Promise.resolve().then(async () => {
      try {
        // receipt must belong to the explicitly requested job, even if a new job is now visible.
        // receipt 必须属于明确请求的作业，即使此时已显示新作业。
        const receipt = await this.transport.cancel(projectId, job.jobId);
        if (receipt.projectId !== projectId || receipt.jobId !== job.jobId) throw new Error("取消索引作业返回了不匹配的身份。");
        if (this.owns(entry)) this.accept(entry, receipt);
      } catch (error) {
        if (this.owns(entry) && entry.snapshot.job?.jobId === job.jobId) {
          entry.actionFailure = { jobId: job.jobId, message: error instanceof Error ? error.message : String(error) };
        }
      } finally {
        if (this.owns(entry) && entry.cancellation?.request === request) {
          entry.cancellation = null;
          this.publish(entry);
        }
      }
    });
    entry.cancellation = { jobId: job.jobId, request };
    this.publish(entry);
    return request;
  }

  // Test entry's exact ownership. Returns false after unmount or replacement, even for the same ID.
  // 检查 entry 的精确归属。卸载或替换后返回否，即使项目编号相同。
  private owns(entry: ProjectJobObservation): boolean { return this.projects.get(entry.projectId) === entry; }

  // Merge receipt into the owned entry with sequence checks; returns nothing.
  // 使用序号检查将 receipt 合并到所属 entry；无返回值。
  private accept(entry: ProjectJobObservation, receipt: VceJobSummary | null): void {
    if (!this.owns(entry)) return;
    this.publish(entry, mergeVceJobReceipt(entry.snapshot.job, receipt, entry.projectId));
  }

  // Reload entry once without overlapping requests or implicitly admitting work; resolves after IO.
  // 不重叠请求或隐式受理工作地重新读取 entry 一次；输入输出结束后完成。
  private async reload(entry: ProjectJobObservation): Promise<void> {
    if (!this.owns(entry) || entry.reading) return;
    entry.reading = true;
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
      // receipt is ignored if its exact observation lifetime has already ended.
      // receipt 在其精确观察生命周期已经结束时被忽略。
      const receipt = await this.transport.load(entry.projectId, { signal: controller.signal });
      if (controller.signal.aborted) {
        if (timedOut && this.owns(entry)) {
          entry.readError = VCE_OBSERVER_READ_TIMEOUT_MESSAGE;
          this.publish(entry);
        }
        return;
      }
      if (!this.owns(entry)) return;
      if (receipt !== null && receipt.projectId !== entry.projectId) throw new Error("语义索引作业返回了其他项目的数据。");
      entry.readError = null;
      this.accept(entry, receipt);
    } catch (error) {
      // An unmount is silent transport control; a deadline publishes a retryable diagnostic.
      // 卸载属于静默传输控制；达到期限时发布可重试诊断。
      if (controller.signal.aborted) {
        if (timedOut && this.owns(entry)) {
          entry.readError = VCE_OBSERVER_READ_TIMEOUT_MESSAGE;
          this.publish(entry);
        }
      } else if (this.owns(entry)) {
        entry.readError = error instanceof Error ? error.message : String(error);
        this.publish(entry);
      }
    } finally {
      clearTimeout(deadline);
      if (entry.readDeadlineTimer === deadline) entry.readDeadlineTimer = null;
      if (entry.readAbortController === controller) entry.readAbortController = null;
      entry.reading = false;
    }
  }

  // Register one shared event stream if needed; retries only at a subsequent observation/poll.
  // 按需注册一个共享事件流；仅在后续观察或轮询时重试。
  // Takes no arguments and returns nothing; obsolete subscription handles are always released.
  // 无参数和返回值；过期订阅句柄始终被释放。
  private ensureRegistration(): void {
    if (this.registration || this.projects.size === 0) return;
    // registration is installed synchronously to coalesce simultaneous subscribing views.
    // registration 同步安装，用于合并同时订阅的视图。
    const registration: JobEventRegistration = { stop: null };
    this.registration = registration;
    void Promise.resolve().then(async () => {
      if (this.registration !== registration) return;
      // stop belongs to this stream only; a retired async registration must not leak its handle.
      // stop 仅属于此流；已退役的异步注册不得泄漏句柄。
      const stop = await this.transport.subscribe((receipt) => {
        if (this.registration !== registration) return;
        // entry is selected from the event's declared project identity, never a UI default.
        // entry 从事件声明的项目身份选择，绝不使用界面默认值。
        const entry = this.projects.get(receipt.projectId);
        if (entry) this.accept(entry, receipt);
      });
      if (this.registration !== registration) { stop(); return; }
      registration.stop = stop;
      this.subscriptionError = null;
      for (const entry of this.projects.values()) this.publish(entry);
    }).catch((error) => {
      if (this.registration !== registration) return;
      this.registration = null;
      this.subscriptionError = error instanceof Error ? error.message : String(error);
      for (const entry of this.projects.values()) this.publish(entry);
    });
  }

  // Publish a changed entry projection only to its project listeners; job defaults to its receipt.
  // 仅向项目自己的监听者发布变化后的 entry 投影；job 默认为其已有回执。
  // Returns nothing; command, read, and stream errors retain their explicitly separate ownership.
  // 无返回值；命令、读取和流错误保留其明确分离的归属。
  private publish(entry: ProjectJobObservation, job: VceJobSummary | null = entry.snapshot.job): void {
    if (entry.actionFailure && (entry.actionFailure.jobId !== job?.jobId || !isVceJobActive(job))) entry.actionFailure = null;
    // error prioritizes an exact command failure, then project reads, then the shared connection.
    // error 优先展示精确命令失败，其次是项目读取，最后是共享连接。
    const error = entry.actionFailure?.message ?? entry.readError ?? this.subscriptionError;
    // cancelPending never carries an old job's pending command into a new job's controls.
    // cancelPending 绝不将旧作业的待完成命令带入新作业控件。
    const cancelPending = entry.cancellation !== null && entry.cancellation.jobId === job?.jobId;
    if (entry.snapshot.job === job && entry.snapshot.error === error && entry.snapshot.cancelPending === cancelPending) return;
    entry.snapshot = { job, error, cancelPending };
    for (const listener of entry.listeners) listener();
  }
}
