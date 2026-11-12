import { afterEach, expect, it, vi } from "vitest";
import { encodeVoiceInputWav } from "./voice-input-wav";

afterEach(() => { vi.unstubAllGlobals(); });

it("keeps the in-limit speech when the browser records extra frames", async () => {
  // Model one second of decoded audio and a delayed stop with a different sample at the tail.
  // 模拟一秒解码音频，以及停止延迟后多录的一帧不同采样。
  const sampleRate = 16_000;
  // Keep the last in-limit sample distinguishable from the extra sample.
  // 让时限内最后一帧与多录的一帧可区分。
  const samples = new Float32Array(sampleRate + 1);
  samples[sampleRate - 1] = 0.5;
  samples[sampleRate] = -0.5;
  // Substitute deterministic decoded samples for browser audio hardware.
  // 用可预测的解码采样代替浏览器音频硬件。
  vi.stubGlobal("OfflineAudioContext", class {
    // Decode the supplied test recording as a mono buffer with one extra frame.
    // 将输入测试录音解码为多出一帧的单声道缓冲区。
    async decodeAudioData(_recording: ArrayBuffer) {
      return { length: samples.length, sampleRate, numberOfChannels: 1, getChannelData: () => samples };
    }
  });

  // Read back the encoded WAV container and its last retained PCM sample.
  // 读回编码后的 WAV 容器及保留的最后一个 PCM 采样。
  const recording = await encodeVoiceInputWav(new Blob(["browser audio"]), 1);
  expect(recording?.type).toBe("audio/wav");
  if (!recording) throw new Error("Expected an audible WAV recording");
  // A mono 16-bit WAV has two bytes per frame after its 44-byte header.
  // 单声道 16 位 WAV 在 44 字节的文件头之后每帧占两个字节。
  const wav = new DataView(await recording.arrayBuffer());
  expect(wav.byteLength).toBe(44 + sampleRate * 2);
  expect(wav.getUint32(4, true)).toBe(wav.byteLength - 8);
  expect(wav.getUint32(40, true)).toBe(sampleRate * 2);
  expect(wav.getInt16(wav.byteLength - 2, true)).toBe(16_384);
});
