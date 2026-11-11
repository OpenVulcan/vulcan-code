import * as React from "react";
import { LoaderCircle, Mic } from "lucide-react";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { toast } from "@/ui-kit/primitives/toast";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/ui-kit/primitives/hover-card";
import type { VoiceInputSummary } from "@/types";
import { readVoiceInputDevice, voiceInputAudioConstraints } from "@/components/composer/composer-panel/voice-input-device";
import { hasBlockingComposerPopup, isComposerTextEntryElement } from "@/components/composer/composer-model";
import { getWorkspaceInputOwner } from "@/components/workspace/workspace-input-owner";
import { isVoiceInputModifierKey, readVoiceInputShortcut, voiceInputShortcutConflicts, voiceInputShortcutFromKey } from "./voice-input-shortcut";

// The settings section loads with the quick setup dialog, keeping settings styles out of the workbench chunk.
// 设置节随快速设置对话框加载，使设置样式不进入工作台分块。
const LazyVoiceInputSettingsSection = React.lazy(() => import("@/components/features/settings/voice-input-settings")
  .then((module) => ({ default: module.VoiceInputSettingsSection })));

// Load the microphone summary and device picker when the user opens the hover card.
// 用户打开悬停卡片时加载麦克风摘要与设备选择器。
const LazyVoiceInputCard = React.lazy(() => import("./voice-input-card")
  .then((module) => ({ default: module.VoiceInputCard })));

// Keep the browser's native recording formats and encode the completed recording as PCM WAV for ASR.
// 保留浏览器原生录音格式，录制完成后编码为 PCM WAV 供语音识别使用。
const RECORDING_MIME_TYPES = ["audio/webm;codecs=opus", "audio/webm", "audio/ogg;codecs=opus", "audio/mp4"];

// Recordings shorter than this are treated as a tap, not speech.
// 短于此时长的录音视为轻点，而不是说话。
const MIN_RECORDING_MS = 500;

// Give ordinary modifier chords time to arrive before opening the microphone.
// 打开麦克风前留出时间，等待普通修饰键组合的后续按键。
const MODIFIER_HOLD_MS = 150;

// The countdown turns into a warning for the last seconds of a recording.
// 录音最后几秒的倒计时变为提醒样式。
const WARNING_SECONDS = 10;

type VoiceInputPhase = "idle" | "starting" | "recording" | "transcribing";

// RecordingSession is one press of the button, from opening the microphone to the transcript.
// RecordingSession 是一次按压，从打开麦克风到得到转写文本。
interface RecordingSession {
  // The exact chord that started this recording, or null for a microphone button press.
  // 启动本次录音的精确组合键，麦克风按钮按压则为 null。
  shortcut: string | null;
  recorder: MediaRecorder | null;
  stream: MediaStream | null;
  chunks: Blob[];
  startedAt: number;
  // Set once the press ends, by release, cancel or the duration limit.
  // 按压结束时置位：松开、取消或达到时长上限。
  released: boolean;
  cancelled: boolean;
  timer: number | null;
}

// VoiceInputButtonProps wires push-to-talk into the composer.
// VoiceInputButtonProps 把按住说话接入输入区。
export interface VoiceInputButtonProps {
  // Published voice-input state; absent until the AI configuration is loaded.
  // 已发布的语音输入状态；AI 配置加载前缺失。
  voiceInput: VoiceInputSummary | null;
  // Whether the composer currently rejects new input, such as while editing a pending prompt.
  // 输入区当前是否拒绝新输入，例如正在编辑待发送提示词时。
  disabled: boolean;
  // Identifies the active composer field when another editor owns keyboard input.
  // 当其他编辑器持有键盘时，用于识别当前输入区文本框。
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  // Inserts one non-empty transcript into the draft.
  // 将一段非空转写文本插入草稿。
  onTranscript: (text: string) => void;
  // Reports whether a recording or transcription is in progress, so sending can wait.
  // 报告录音或转写是否进行中，以便发送等待。
  onBusyChange: (busy: boolean) => void;
}

// pickRecordingMimeType returns the first recorder format this browser supports.
// pickRecordingMimeType 返回当前浏览器支持的首个录音格式。
function pickRecordingMimeType(): string | null {
  if (typeof MediaRecorder === "undefined") return null;
  return RECORDING_MIME_TYPES.find((type) => MediaRecorder.isTypeSupported(type)) ?? null;
}

// VoiceInputButton records while held and inserts the transcript; it never sends the draft.
// VoiceInputButton 在按住期间录音并插入转写文本；它从不发送草稿。
export function VoiceInputButton(props: VoiceInputButtonProps) {
  const { t } = useI18n();
  const [phase, setPhase] = React.useState<VoiceInputPhase>("idle");
  const [remainingSeconds, setRemainingSeconds] = React.useState(0);
  // The hover card summarizes the setup; it stays closed while recording and while the setup dialog is open.
  // 悬停卡片概括设置；录音期间与设置对话框打开期间保持关闭。
  const [cardOpen, setCardOpen] = React.useState(false);
  const [setupOpen, setSetupOpen] = React.useState(false);
  // session holds the live recorder; it is replaced on every press so late callbacks are ignored.
  // session 持有当前录音器；每次按下都会替换，使迟到的回调被忽略。
  const session = React.useRef<RecordingSession | null>(null);
  // Only a catalog result for the currently published native account can enable the microphone.
  // 只有当前发布的原生账号所对应的目录结果才能启用麦克风。
  const cloudAccountId = props.voiceInput?.cloudAccountId;
  const [readyCloudAccount, setReadyCloudAccount] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!cloudAccountId) return;
    let cancelled = false;
    void import("@/service/voice-input-service").then(({ loadVoiceInputCloudCatalog }) => loadVoiceInputCloudCatalog()).then(
      (catalog) => {
        if (cancelled) return;
        setReadyCloudAccount(catalog.status === "available" && catalog.connection_id === cloudAccountId && catalog.selection ? cloudAccountId : null);
      },
      () => { if (!cancelled) setReadyCloudAccount(null); },
    );
    return () => { cancelled = true; };
  }, [cloudAccountId]);
  const configured = Boolean(props.voiceInput?.configured || (cloudAccountId && readyCloudAccount === cloudAccountId));
  const maxDurationSeconds = props.voiceInput?.maxDurationSeconds ?? 0;
  const { disabled, onBusyChange, onTranscript, textareaRef } = props;

  React.useEffect(() => { onBusyChange(phase !== "idle"); }, [onBusyChange, phase]);

  // release ends the current press: stop and transcribe, or discard when cancelled.
  // release 结束当前按压：停止并转写；已取消时丢弃。
  const release = React.useCallback((cancel: boolean) => {
    const current = session.current;
    if (!current || current.released) return;
    current.released = true;
    current.cancelled = cancel;
    if (current.timer !== null) window.clearInterval(current.timer);
    // While the microphone is still opening there is no recorder yet; start() finishes the release.
    // 麦克风仍在打开时尚无录音器；由 start() 完成这次释放。
    if (current.recorder && current.recorder.state !== "inactive") current.recorder.stop();
  }, []);

  // finish turns the recorded chunks into a transcript after the recorder stops.
  // finish 在录音器停止后把录音片段转为转写文本。
  const finish = React.useCallback(async (current: RecordingSession, mimeType: string) => {
    current.stream?.getTracks().forEach((track) => track.stop());
    if (session.current !== current) return;
    if (current.cancelled) {
      session.current = null;
      setPhase("idle");
      return;
    }
    if (Date.now() - current.startedAt < MIN_RECORDING_MS) {
      session.current = null;
      setPhase("idle");
      toast.info(t("composer.voice.tooShort"));
      return;
    }
    setPhase("transcribing");
    try {
      // Load conversion and transport on demand when a complete recording needs transcription.
      // 录音完成且需要转写时按需加载转换与传输代码。
      const [{ encodeVoiceInputWav }, { transcribeVoiceInput }] = await Promise.all([
        import("./voice-input-wav"),
        import("@/service/voice-input-service"),
      ]);
      if (session.current !== current) return;
      // Decode after stopping so cancelled recordings never incur conversion or a provider call.
      // 停止后再解码，取消的录音无需转换或调用供应商。
      const recording = await encodeVoiceInputWav(new Blob(current.chunks, { type: mimeType }));
      if (session.current !== current) return;
      const { text } = await transcribeVoiceInput(recording);
      if (session.current !== current) return;
      if (text) onTranscript(text);
      else toast.info(t("composer.voice.noSpeech"));
    } catch (cause) {
      if (session.current === current) toast.error(t("composer.voice.failed"), { description: errorMessage(cause) });
    } finally {
      if (session.current === current) {
        session.current = null;
        setPhase("idle");
      }
    }
  }, [onTranscript, t]);

  // start opens the microphone for the supplied chord or button press, returning when recording begins or fails.
  // start 为指定组合键或按钮按压打开麦克风，录音开始或失败后返回。
  const start = React.useCallback(async (shortcut: string | null = null) => {
    if (session.current || !configured || disabled) return;
    const mimeType = pickRecordingMimeType();
    if (!mimeType || !navigator.mediaDevices?.getUserMedia) {
      toast.error(t("composer.voice.unsupported"));
      return;
    }
    const current: RecordingSession = {
      shortcut, recorder: null, stream: null, chunks: [], startedAt: Date.now(), released: false, cancelled: false, timer: null,
    };
    session.current = current;
    setCardOpen(false);
    setPhase("starting");
    try {
      current.stream = await navigator.mediaDevices.getUserMedia(voiceInputAudioConstraints(readVoiceInputDevice()));
    } catch (cause) {
      if (session.current !== current) return;
      session.current = null;
      setPhase("idle");
      if (!current.cancelled) toast.error(t("composer.voice.micUnavailable"), { description: errorMessage(cause) });
      return;
    }
    if (current.released) {
      // Released while the microphone was opening: nothing was recorded.
      // 麦克风打开期间已释放：没有录到任何内容。
      current.stream.getTracks().forEach((track) => track.stop());
      if (session.current !== current) return;
      session.current = null;
      setPhase("idle");
      if (!current.cancelled) toast.info(t("composer.voice.tooShort"));
      return;
    }
    const recorder = new MediaRecorder(current.stream, { mimeType });
    current.recorder = recorder;
    recorder.ondataavailable = (event) => { if (event.data.size > 0) current.chunks.push(event.data); };
    recorder.onstop = () => { void finish(current, recorder.mimeType || mimeType); };
    current.startedAt = Date.now();
    recorder.start();
    setRemainingSeconds(maxDurationSeconds);
    setPhase("recording");
    current.timer = window.setInterval(() => {
      const left = maxDurationSeconds - Math.floor((Date.now() - current.startedAt) / 1000);
      setRemainingSeconds(Math.max(0, left));
      // At the limit the recording ends and is still transcribed.
      // 到达上限时结束录音，仍会转写。
      if (left <= 0) release(false);
    }, 250);
  }, [configured, disabled, finish, maxDurationSeconds, release, t]);

  React.useEffect(() => {
    // Track both physical sides so a shared logical modifier flag cannot hide a second key.
    // 记录左右物理键，避免共用的逻辑修饰键标志掩盖第二个按键。
    const heldModifiers = new Set<string>();
    let pending: { code: string; timer: number } | null = null;

    // Cancels a modifier hold that has not opened the microphone yet.
    // 取消尚未打开麦克风的修饰键长按。
    const clearPending = () => {
      if (pending) window.clearTimeout(pending.timer);
      pending = null;
    };

    // Allows recording only in a visible composer that still owns keyboard input.
    // 仅允许在仍持有键盘输入的可见输入区录音。
    const canStartShortcut = () => {
      if (session.current || hasBlockingComposerPopup()) return false;
      const textarea = textareaRef.current;
      return Boolean(textarea?.isConnected && textarea.checkVisibility?.() !== false
        && getWorkspaceInputOwner().kind !== "project_terminal"
        && (document.activeElement === textarea || !isComposerTextEntryElement(document.activeElement)));
    };

    // Cancels standalone recording before other key handlers see a modifier chord.
    // 在其他按键处理器收到组合键前，取消单独修饰键启动的录音。
    const onOtherKeyDown = (event: KeyboardEvent) => {
      if (isVoiceInputModifierKey(event.code)) heldModifiers.add(event.code);
      if (pending && pending.code !== event.code) clearPending();
      const active = session.current?.shortcut;
      if (active && isVoiceInputModifierKey(active) && active !== event.code) release(true);
    };

    // Keep keyboard push-to-talk inside the active composer and respect keys claimed by other controls.
    // 键盘按住说话只作用于当前输入区，并尊重其他控件已处理的按键。
    const onShortcutDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !configured || disabled) return;
      // Read the current binding so a newly saved shortcut applies without reopening the conversation.
      // 读取当前绑定，使新保存的快捷键无需重开会话即可生效。
      const shortcut = voiceInputShortcutFromKey(event);
      if (!shortcut || shortcut !== readVoiceInputShortcut() || voiceInputShortcutConflicts(shortcut, event)) return;
      if (isVoiceInputModifierKey(shortcut)) {
        if (event.repeat || heldModifiers.size !== 1 || pending || !canStartShortcut()) return;
        pending = { code: shortcut, timer: window.setTimeout(() => {
          pending = null;
          if (canStartShortcut() && readVoiceInputShortcut() === shortcut) void start(shortcut);
        }, MODIFIER_HOLD_MS) };
        return;
      }
      if (session.current?.shortcut === shortcut) {
        event.preventDefault();
        return;
      }
      if (event.repeat || !canStartShortcut()) return;
      event.preventDefault();
      void start(shortcut);
    };
    // End the recorded chord even if focus changed or its modifier was released before its main key.
    // 即使焦点变化或修饰键先于主键松开，也结束该组合键录音。
    const onShortcutUp = (event: KeyboardEvent) => {
      heldModifiers.delete(event.code);
      if (pending?.code === event.code) clearPending();
      const shortcut = session.current?.shortcut;
      if (!shortcut) return;
      // A modifier's logical key is shared by the left and right physical keys.
      // 左右修饰键使用相同的逻辑键名。
      const keys = shortcut.split("+");
      if (!keys.includes(event.code) && !keys.includes(event.key)) return;
      event.preventDefault();
      release(false);
    };
    // Discard a held shortcut when the window loses focus and can no longer observe key release.
    // 窗口失焦后无法观察松键，丢弃仍按住的快捷键录音。
    const cancelShortcut = () => {
      heldModifiers.clear();
      clearPending();
      if (session.current?.shortcut) release(true);
    };
    window.addEventListener("keydown", onOtherKeyDown, true);
    window.addEventListener("keydown", onShortcutDown);
    window.addEventListener("keyup", onShortcutUp, true);
    window.addEventListener("blur", cancelShortcut);
    return () => {
      clearPending();
      window.removeEventListener("keydown", onOtherKeyDown, true);
      window.removeEventListener("keydown", onShortcutDown);
      window.removeEventListener("keyup", onShortcutUp, true);
      window.removeEventListener("blur", cancelShortcut);
    };
  }, [configured, disabled, release, start, textareaRef]);

  // Escape cancels an active recording from anywhere in the window.
  // 在窗口任意位置按 Escape 都会取消正在进行的录音。
  React.useEffect(() => {
    if (phase !== "starting" && phase !== "recording") return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      release(true);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [phase, release]);

  // Unmounting mid-recording discards the recording and frees the microphone.
  // 录音中卸载会丢弃录音并释放麦克风。
  React.useEffect(() => () => {
    const current = session.current;
    if (!current) return;
    session.current = null;
    if (current.timer !== null) window.clearInterval(current.timer);
    current.cancelled = true;
    current.released = true;
    if (current.recorder && current.recorder.state !== "inactive") current.recorder.stop();
    current.stream?.getTracks().forEach((track) => track.stop());
  }, []);

  const recording = phase === "starting" || phase === "recording";
  const unavailable = !configured || props.disabled;
  const label = !configured ? t("composer.voice.notConfigured")
    : phase === "transcribing" ? t("composer.voice.transcribing")
      : recording ? t("composer.voice.recording")
        : formatMessage(t("composer.voice.hold"), { seconds: maxDurationSeconds });
  return (
    <>
    <HoverCard
      onOpenChange={(open) => { if (!open || (phase === "idle" && !setupOpen)) setCardOpen(open); }}
      open={cardOpen}
    >
      <HoverCardTrigger asChild>
        <button
          aria-disabled={unavailable || phase === "transcribing"}
          aria-label={label}
          aria-pressed={recording}
          className="vk-voice"
          data-state={recording ? "recording" : phase === "transcribing" ? "transcribing" : undefined}
          data-warning={phase === "recording" && remainingSeconds <= WARNING_SECONDS ? "" : undefined}
          onKeyDown={(event) => {
            if ((event.key === " " || event.key === "Enter") && !event.repeat
              && !(event.ctrlKey || event.altKey || event.shiftKey || event.metaKey)) {
              event.preventDefault();
              void start();
            }
          }}
          onKeyUp={(event) => {
            if (!session.current?.shortcut && (event.key === " " || event.key === "Enter")) {
              event.preventDefault();
              release(false);
            }
          }}
          onPointerCancel={() => { if (!session.current?.shortcut) release(true); }}
          onPointerDown={(event) => {
            if (event.button !== 0 || unavailable || session.current) return;
            event.currentTarget.setPointerCapture(event.pointerId);
            void start();
          }}
          onPointerUp={() => { if (!session.current?.shortcut) release(false); }}
          type="button"
        >
          {phase === "transcribing"
            ? <LoaderCircle aria-hidden="true" className="vk-spin" size={16} />
            : <Mic aria-hidden="true" size={16} />}
          {phase === "recording" ? <span className="vk-voice-time">{remainingSeconds}s</span> : null}
        </button>
      </HoverCardTrigger>
      <HoverCardContent align="end" className="vk-cap-sum-pop" side="top" sideOffset={8}>
        <React.Suspense fallback={<p className="vk-cap-sum-note">{t("composer.voice.loading")}</p>}>
          <LazyVoiceInputCard
            configured={configured}
            maxDurationSeconds={maxDurationSeconds}
            onConfigure={() => {
              setCardOpen(false);
              setSetupOpen(true);
            }}
          />
        </React.Suspense>
      </HoverCardContent>
    </HoverCard>
    <Dialog onOpenChange={setSetupOpen} open={setupOpen}>
      <DialogContent size="sm">
        <DialogHeader><DialogTitle>{t("composer.voice.title")}</DialogTitle></DialogHeader>
        {setupOpen ? (
          <React.Suspense fallback={<p className="vk-cap-sum-note">{t("composer.voice.loading")}</p>}>
            <LazyVoiceInputSettingsSection />
          </React.Suspense>
        ) : null}
      </DialogContent>
    </Dialog>
    </>
  );
}
