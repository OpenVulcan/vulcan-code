import * as React from "react";
import { LoaderCircle, Mic } from "lucide-react";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { transcribeVoiceInput } from "@/service/voice-input-service";
import { toast } from "@/ui-kit/primitives/toast";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/ui-kit/primitives/hover-card";
import type { VoiceInputSummary } from "@/types";
import { VoiceInputCard } from "@/components/composer/composer-panel/voice-input-card";
import { readVoiceInputDevice, voiceInputAudioConstraints } from "@/components/composer/composer-panel/voice-input-device";

// The settings section loads with the quick setup dialog, keeping settings styles out of the workbench chunk.
// 设置节随快速设置对话框加载，使设置样式不进入工作台分块。
const LazyVoiceInputSettingsSection = React.lazy(() => import("@/components/features/settings/voice-input-settings")
  .then((module) => ({ default: module.VoiceInputSettingsSection })));

// Recorder formats in preference order; every one maps to an extension the service accepts.
// 按偏好排列的录音格式；每一种都对应服务接受的扩展名。
const RECORDING_MIME_TYPES = ["audio/webm;codecs=opus", "audio/webm", "audio/ogg;codecs=opus", "audio/mp4"];

// Recordings shorter than this are treated as a tap, not speech.
// 短于此时长的录音视为轻点，而不是说话。
const MIN_RECORDING_MS = 500;

// The countdown turns into a warning for the last seconds of a recording.
// 录音最后几秒的倒计时变为提醒样式。
const WARNING_SECONDS = 10;

type VoiceInputPhase = "idle" | "starting" | "recording" | "transcribing";

// RecordingSession is one press of the button, from opening the microphone to the transcript.
// RecordingSession 是一次按压，从打开麦克风到得到转写文本。
interface RecordingSession {
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
  const configured = props.voiceInput?.configured ?? false;
  const maxDurationSeconds = props.voiceInput?.maxDurationSeconds ?? 0;
  const { onBusyChange, onTranscript } = props;

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
      const { text } = await transcribeVoiceInput(new Blob(current.chunks, { type: mimeType }));
      if (text) onTranscript(text);
      else toast.info(t("composer.voice.noSpeech"));
    } catch (cause) {
      toast.error(t("composer.voice.failed"), { description: errorMessage(cause) });
    } finally {
      if (session.current === current) session.current = null;
      setPhase("idle");
    }
  }, [onTranscript, t]);

  // start opens the microphone and records until released or the duration limit is reached.
  // start 打开麦克风并录音，直到释放或达到时长上限。
  const start = React.useCallback(async () => {
    if (session.current || !configured || props.disabled) return;
    const mimeType = pickRecordingMimeType();
    if (!mimeType || !navigator.mediaDevices?.getUserMedia) {
      toast.error(t("composer.voice.unsupported"));
      return;
    }
    const current: RecordingSession = {
      recorder: null, stream: null, chunks: [], startedAt: Date.now(), released: false, cancelled: false, timer: null,
    };
    session.current = current;
    setCardOpen(false);
    setPhase("starting");
    try {
      current.stream = await navigator.mediaDevices.getUserMedia(voiceInputAudioConstraints(readVoiceInputDevice()));
    } catch (cause) {
      session.current = null;
      setPhase("idle");
      toast.error(t("composer.voice.micUnavailable"), { description: errorMessage(cause) });
      return;
    }
    if (current.released) {
      // Released while the microphone was opening: nothing was recorded.
      // 麦克风打开期间已释放：没有录到任何内容。
      current.stream.getTracks().forEach((track) => track.stop());
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
  }, [configured, finish, maxDurationSeconds, props.disabled, release, t]);

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
            if ((event.key === " " || event.key === "Enter") && !event.repeat) {
              event.preventDefault();
              void start();
            }
          }}
          onKeyUp={(event) => {
            if (event.key === " " || event.key === "Enter") {
              event.preventDefault();
              release(false);
            }
          }}
          onPointerCancel={() => release(true)}
          onPointerDown={(event) => {
            if (event.button !== 0 || unavailable) return;
            event.currentTarget.setPointerCapture(event.pointerId);
            void start();
          }}
          onPointerUp={() => release(false)}
          type="button"
        >
          {phase === "transcribing"
            ? <LoaderCircle aria-hidden="true" className="vk-spin" size={16} />
            : <Mic aria-hidden="true" size={16} />}
          {phase === "recording" ? <span className="vk-voice-time">{remainingSeconds}s</span> : null}
        </button>
      </HoverCardTrigger>
      <HoverCardContent align="end" className="vk-cap-sum-pop" side="top" sideOffset={8}>
        <VoiceInputCard
          configured={configured}
          maxDurationSeconds={maxDurationSeconds}
          onConfigure={() => {
            setCardOpen(false);
            setSetupOpen(true);
          }}
        />
      </HoverCardContent>
    </HoverCard>
    <Dialog onOpenChange={setSetupOpen} open={setupOpen}>
      <DialogContent>
        <DialogHeader><DialogTitle>{t("composer.voice.title")}</DialogTitle></DialogHeader>
        {setupOpen ? (
          <React.Suspense fallback={<p className="vk-cap-sum-note">{t("composer.voice.loading")}</p>}>
            <LazyVoiceInputSettingsSection onSaved={() => setSetupOpen(false)} />
          </React.Suspense>
        ) : null}
      </DialogContent>
    </Dialog>
    </>
  );
}
