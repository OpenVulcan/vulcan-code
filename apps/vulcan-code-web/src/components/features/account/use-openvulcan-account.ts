import * as React from "react";
import { errorMessage } from "@/lib/display-format";
import { hasServiceRuntime } from "@/service/webui-service-runtime";
import { cancelOpenVulcanLogin, claimOpenVulcanPrompt, commitOpenVulcanLogin, loadOpenVulcanAccount, logoutOpenVulcanAccount, openVulcanLoginStatus, startOpenVulcanLogin, type OpenVulcanAccountSnapshot, type OpenVulcanRegion } from "@/service/openvulcan-account-service";
import type { PublicAiOAuthChallenge } from "@/types";
import type { OpenVulcanAccountController, OpenVulcanAccountError } from "./openvulcan-account-context";

// BootstrapClaim survives StrictMode's effect restart without losing the host's one-time prompt claim.
// BootstrapClaim 在 StrictMode 重启副作用时保留宿主的一次性提示认领。
interface BootstrapClaim {
  // One account read followed by the atomic installation-wide claim.
  // 一次账户读取，随后执行整个安装的原子认领。
  promise: Promise<{ account: OpenVulcanAccountSnapshot; first: boolean }>;
  // A claimed prompt is consumed only by a still-mounted workbench.
  // 已认领提示仅由仍挂载的工作台消费。
  consumed: boolean;
}

// bootstraps retains one claim per service generation in this browser window.
// bootstraps 在此浏览器窗口中为每个服务代际保留一次认领。
const bootstraps = new Map<number, BootstrapClaim>();

// useOpenVulcanAccount owns global onboarding and serial login polling for a ready service epoch.
// useOpenVulcanAccount 为就绪服务代际持有全局引导及串行登录轮询。
// Parameters: ready gates host access; serviceEpoch invalidates old callbacks. Returns the shell controller.
// 参数：ready 门控宿主访问；serviceEpoch 使旧回调失效。返回外壳控制器。
export function useOpenVulcanAccount(ready: boolean, serviceEpoch: number): OpenVulcanAccountController {
  // snapshot is safe public state; credentials remain exclusively on the host.
  // snapshot 是安全公开状态；凭据仅保留于宿主。
  const [snapshot, setSnapshot] = React.useState<OpenVulcanAccountSnapshot | null>(null);
  // initialized orders this prompt before subsequent setup guides.
  // initialized 将本提示排列在后续初始化引导之前。
  const [initialized, setInitialized] = React.useState(false);
  // open, busy, error and challenge describe the current account interaction.
  // open、busy、error 与 challenge 描述当前账户交互。
  const [open, setDialogOpen] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<OpenVulcanAccountError | null>(null);
  const [challenge, setChallenge] = React.useState<PublicAiOAuthChallenge | null>(null);
  // target is the deployment of the latest sign-in, shown on its card while it starts or waits.
  // target 是最近一次登录的部署，在其启动或等待期间显示在对应卡片上。
  const [target, setTarget] = React.useState<OpenVulcanRegion | null>(null);
  // generation rejects observations from actions that have been superseded or unmounted.
  // generation 拒绝来自已被替代或卸载操作的观测。
  const generation = React.useRef(0);
  // mutation prevents same-frame double-clicks before React commits the busy state.
  // mutation 防止 React 提交忙碌状态前的同帧双击。
  const mutation = React.useRef(false);
  // pending keeps the exact transaction available to unmount cleanup.
  // pending 为卸载清理保留准确事务。
  const pending = React.useRef<string | null>(null);
  // loginRegion preserves the user's explicit deployment for a fresh authorization after failure.
  // loginRegion 保留用户明确选择的部署，用于失败后发起新的授权。
  const loginRegion = React.useRef<OpenVulcanRegion | null>(null);

  React.useEffect(() => {
    if (!ready) return;
    if (!hasServiceRuntime()) { setInitialized(true); return; }
    // live and epoch prevent an old service generation from updating this workbench.
    // live 与 epoch 防止旧服务代际更新此工作台。
    let live = true;
    const epoch = ++generation.current;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setInitialized(false);
    setChallenge(null);
    setBusy(false);
    mutation.current = false;
    // refresh serializes background reads and lets the host renew before credential expiration.
    // refresh 串行执行后台读取，使宿主在凭据过期前续期。
    const refresh = async () => {
      if (mutation.current || pending.current) { if (live) timer = setTimeout(() => void refresh(), 60_000); return; }
      const revision = generation.current;
      try {
        const next = await loadOpenVulcanAccount();
        if (live && revision === generation.current && !mutation.current) {
          setSnapshot(next);
          setError((current) => current?.stage === "load" ? null : current);
        }
      } catch (cause) {
        if (live && revision === generation.current && !mutation.current) setError({ stage: "load", detail: errorMessage(cause), region: null });
      }
      if (live) timer = setTimeout(() => void refresh(), 60_000);
    };
    // claim is reused across StrictMode remounts, but consumed once by the live owner.
    // claim 在 StrictMode 重挂载间复用，但仅由存活所有者消费一次。
    let claim = bootstraps.get(serviceEpoch);
    if (!claim) {
      claim = { promise: loadOpenVulcanAccount().then(async (account) => ({ account, first: await claimOpenVulcanPrompt() })), consumed: false };
      bootstraps.set(serviceEpoch, claim);
    }
    const ownedClaim = claim;
    // Later workbench mounts re-read the account without reclaiming or replaying onboarding.
    // 后续工作台挂载重读账户，不重新认领或重放引导。
    const observation = ownedClaim.consumed ? loadOpenVulcanAccount().then((account) => ({ account, first: false })) : ownedClaim.promise;
    void observation.then(({ account, first }) => {
      if (!live || generation.current !== epoch) return;
      setSnapshot(account);
      if (!ownedClaim.consumed) {
        ownedClaim.consumed = true;
        if (first && !account.connected) setDialogOpen(true);
      }
    }).catch((cause: unknown) => {
      // A failed bootstrap never claims onboarding; a later mount may retry the host read.
      // 失败的初始化不会认领引导；后续挂载可重试宿主读取。
      if (!ownedClaim.consumed && bootstraps.get(serviceEpoch) === ownedClaim) bootstraps.delete(serviceEpoch);
      if (live && generation.current === epoch) setError({ stage: "load", detail: errorMessage(cause), region: null });
    }).finally(() => {
      if (live) { setInitialized(true); timer = setTimeout(() => void refresh(), 60_000); }
    });
    return () => {
      live = false;
      generation.current += 1;
      clearTimeout(timer);
      if (pending.current) void cancelOpenVulcanLogin(pending.current).catch(() => undefined);
      pending.current = null;
    };
  }, [ready, serviceEpoch]);

  React.useEffect(() => {
    if (!challenge) return;
    // alive and epoch bind polling and commit to the displayed transaction.
    // alive 与 epoch 将轮询和提交绑定到展示中的事务。
    let alive = true;
    const epoch = generation.current;
    const transactionId = challenge.transaction.transactionId;
    let timer: ReturnType<typeof setTimeout> | undefined;
    // poll waits for each read before scheduling another, then commits success exactly once.
    // poll 等待每次读取完成后再安排下一次，并仅提交一次成功结果。
    const poll = async () => {
      // stage records the exact failing boundary; an exchanged code alone does not prove a saved login.
      // stage 记录准确失败边界；仅交换授权码不能证明登录已保存。
      let stage: OpenVulcanAccountError["stage"] = "authorize";
      try {
        const status = await openVulcanLoginStatus(transactionId);
        if (!alive || epoch !== generation.current) return;
        if (status.phase === "succeeded") {
          mutation.current = true;
          setBusy(true);
          stage = "connect";
          await commitOpenVulcanLogin(transactionId);
          stage = "load";
          const next = await loadOpenVulcanAccount();
          if (!alive || epoch !== generation.current) return;
          pending.current = null;
          setSnapshot(next);
          setChallenge(null);
          setDialogOpen(false);
          mutation.current = false;
          setBusy(false);
          return;
        }
        if (status.phase === "failed" || status.phase === "cancelled") {
          throw new Error(status.message ?? `Authorization ${status.phase}.`);
        }
        timer = setTimeout(() => void poll(), 1_000);
      } catch (cause) {
        if (!alive || epoch !== generation.current) return;
        void cancelOpenVulcanLogin(transactionId).catch(() => undefined);
        pending.current = null;
        setChallenge(null);
        mutation.current = false;
        setBusy(false);
        setError({ stage, detail: errorMessage(cause), region: loginRegion.current });
        // A failed observation after commit still reconciles any already-persisted account.
        // 提交后的观测失败仍会协调已经持久化的账户。
        void loadOpenVulcanAccount().then((next) => {
          if (epoch !== generation.current) return;
          setSnapshot(next);
          if (stage === "load") setError(null);
        }).catch(() => undefined);
      }
    };
    void poll();
    return () => { alive = false; clearTimeout(timer); };
  }, [challenge]);

  // setOpen closes without signing out, cancelling only this dialog's pending authorization.
  // setOpen 关闭时不退出账户，仅取消本弹窗的待处理授权。
  const setOpen = React.useCallback((next: boolean) => {
    if (mutation.current) return;
    if (!next) {
      generation.current += 1;
      if (pending.current) void cancelOpenVulcanLogin(pending.current).catch((cause: unknown) => setError({ stage: "authorize", detail: errorMessage(cause), region: loginRegion.current }));
      pending.current = null;
      setChallenge(null);
    }
    setDialogOpen(next);
  }, []);

  // cancel abandons the waiting browser authorization, for example after its page was closed, so a
  // new sign-in can start at once instead of waiting for the transaction to expire.
  // cancel 放弃正在等待的浏览器授权（例如授权页已被关闭），使新的登录可以立即开始，而不必等待事务过期。
  const cancel = React.useCallback(() => {
    if (mutation.current || !pending.current) return;
    generation.current += 1;
    const transactionId = pending.current;
    pending.current = null;
    setChallenge(null);
    void cancelOpenVulcanLogin(transactionId).catch((cause: unknown) => setError({ stage: "authorize", detail: errorMessage(cause), region: loginRegion.current }));
  }, []);

  // login starts the chosen region and rejects a late result after unmount or service restart.
  // login 启动所选区域，并拒绝卸载或服务重启后的迟到结果。
  const login = React.useCallback(async (region: OpenVulcanRegion) => {
    if (mutation.current || pending.current) return;
    mutation.current = true;
    const epoch = ++generation.current;
    loginRegion.current = region;
    setTarget(region);
    setBusy(true);
    setError(null);
    try {
      const next = await startOpenVulcanLogin(region);
      if (epoch !== generation.current) { await cancelOpenVulcanLogin(next.transaction.transactionId); return; }
      pending.current = next.transaction.transactionId;
      setChallenge(next);
    } catch (cause) {
      if (epoch === generation.current) setError({ stage: "start", detail: errorMessage(cause), region });
    } finally {
      if (epoch === generation.current) { mutation.current = false; setBusy(false); }
    }
  }, []);

  // logout always re-reads host state because remote revocation may fail after local removal succeeds.
  // logout 始终重读宿主状态，因为远程撤销可能在本地删除成功后失败。
  const logout = React.useCallback(async () => {
    if (mutation.current) return;
    mutation.current = true;
    const epoch = ++generation.current;
    setBusy(true);
    setError(null);
    try { await logoutOpenVulcanAccount(); }
    catch (cause) { if (epoch === generation.current) setError({ stage: "logout", detail: errorMessage(cause), region: null }); }
    finally {
      try {
        const next = await loadOpenVulcanAccount();
        if (epoch === generation.current) setSnapshot(next);
      } catch (cause) { if (epoch === generation.current) setError({ stage: "load", detail: errorMessage(cause), region: null }); }
      if (epoch === generation.current) { mutation.current = false; setBusy(false); }
    }
  }, []);

  // refresh reconciles an uncertain account observation without replaying a committed authorization.
  // refresh 协调不确定的账户观测，不重放已经提交的授权。
  // Returns after the current host state or its explicit read failure has been applied.
  // 应用当前宿主状态或明确的读取失败后返回。
  const refresh = React.useCallback(async () => {
    if (mutation.current || pending.current) return;
    mutation.current = true;
    const epoch = ++generation.current;
    setBusy(true);
    setError(null);
    try {
      const next = await loadOpenVulcanAccount();
      if (epoch === generation.current) setSnapshot(next);
    } catch (cause) {
      if (epoch === generation.current) setError({ stage: "load", detail: errorMessage(cause), region: null });
    } finally {
      if (epoch === generation.current) { mutation.current = false; setBusy(false); }
    }
  }, []);

  return { snapshot, initialized, open, busy, waiting: challenge !== null, loginRegion: target, error, setOpen, cancel, login, logout, refresh };
}
