import * as React from "react";
import type { VoiceInputSummary } from "@/types";

// Load the microphone controls after the composer mounts and keep its toolbar space reserved.
// 输入区挂载后加载麦克风控件，并保留其工具栏位置。
const VoiceInputButton = React.lazy(() => import("./voice-input-button")
  .then(({ VoiceInputButton }) => ({ default: VoiceInputButton })));

// Latin words get a separating space; CJK text joins without one.
// 拉丁文单词之间补空格；中日韩文本直接相连。
const WORD_END = /[A-Za-z0-9.,!?;:]$/;
const WORD_START = /^[A-Za-z0-9]/;

// ComposerVoiceInputOptions connect the microphone to one composer draft.
// ComposerVoiceInputOptions 把麦克风接到一个输入区草稿。
interface ComposerVoiceInputOptions {
  voiceInput: VoiceInputSummary | null;
  // Whether the composer currently rejects new input.
  // 输入区当前是否拒绝新输入。
  disabled: boolean;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  setDraftInput: React.Dispatch<React.SetStateAction<string>>;
}

// useComposerVoiceInput returns the microphone control and whether a recording or transcription holds sending.
// Transcripts are inserted at the caret and selected so the user reviews them; nothing is sent automatically.
// useComposerVoiceInput 返回麦克风控件，以及录音或转写是否暂缓发送。
// 转写文本插入光标处并被选中，供用户确认；不会自动发送。
export function useComposerVoiceInput(options: ComposerVoiceInputOptions): { busy: boolean; control: React.ReactNode } {
  const { textareaRef, setDraftInput } = options;
  const [busy, setBusy] = React.useState(false);
  const insertTranscript = React.useCallback((text: string) => {
    const textarea = textareaRef.current;
    let range: [number, number] | null = null;
    setDraftInput((current) => {
      const start = Math.min(textarea?.selectionStart ?? current.length, current.length);
      const end = Math.min(Math.max(textarea?.selectionEnd ?? start, start), current.length);
      const before = current.slice(0, start);
      const after = current.slice(end);
      const lead = WORD_END.test(before) && WORD_START.test(text) ? " " : "";
      const trail = WORD_END.test(text) && WORD_START.test(after) ? " " : "";
      range = [start + lead.length, start + lead.length + text.length];
      return `${before}${lead}${text}${trail}${after}`;
    });
    window.requestAnimationFrame(() => {
      const field = textareaRef.current;
      if (!field) return;
      field.focus({ preventScroll: true });
      if (range) field.setSelectionRange(range[0], range[1]);
    });
  }, [setDraftInput, textareaRef]);
  return {
    busy,
    control: (
      <React.Suspense fallback={<span aria-hidden="true" className="vk-voice" />}>
        <VoiceInputButton
          disabled={options.disabled}
          onBusyChange={setBusy}
          onTranscript={insertTranscript}
          textareaRef={textareaRef}
          voiceInput={options.voiceInput}
        />
      </React.Suspense>
    ),
  };
}
