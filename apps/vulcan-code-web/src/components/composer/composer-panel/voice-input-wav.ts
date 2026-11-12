// Resample speech at 16 kHz to keep the full recording within the ASR request size limit.
// 语音重采样至 16 kHz，使最长录音也能满足语音识别请求体积限制。
const VOICE_SAMPLE_RATE = 16_000;

// A PCM WAV file uses a 44-byte header before the sample data.
// PCM WAV 文件在采样数据之前使用 44 字节的文件头。
const WAV_HEADER_BYTES = 44;
// The PCM format uses signed 16-bit samples, stored as two little-endian bytes.
// PCM 格式使用有符号 16 位采样，每个采样占两个小端字节。
const PCM_SAMPLE_BYTES = 2;

/**
 * Decode the finished browser recording and return one mono PCM WAV blob for transcription.
 * 解码已录制完成的浏览器音频，返回一个供转写使用的单声道 PCM WAV Blob。
 * @param recording A complete recording produced by MediaRecorder.
 * recording 是 MediaRecorder 生成的完整录音。
 * @param maxDurationSeconds The recording limit reported by the voice service, in seconds.
 * maxDurationSeconds 是语音服务公布的录音时长上限，单位为秒。
 * @returns A 16 kHz WAV blob, or null when every encoded PCM sample is silent.
 * 返回采样率为 16 kHz 的 WAV Blob；所有编码后的 PCM 采样为静音时返回 null。
 */
export async function encodeVoiceInputWav(recording: Blob, maxDurationSeconds: number): Promise<Blob | null> {
  // The offline decoder resamples to its context rate without playing the microphone recording.
  // 离线解码器会重采样至上下文的采样率，且不会播放麦克风录音。
  const decoder = new OfflineAudioContext(1, 1, VOICE_SAMPLE_RATE);
  // The decoded PCM buffer keeps its original channel count.
  // 解码后的 PCM 缓冲区保留原声道数量。
  const audio = await decoder.decodeAudioData(await recording.arrayBuffer());
  // Browser timers and recorder stop can run late, but the service enforces the limit by PCM byte count.
  // 浏览器计时器和录音器停止可能延迟，而服务按 PCM 字节数强制限制时长。
  const frameCount = Math.min(audio.length, maxDurationSeconds * audio.sampleRate);
  // Store one PCM sample per retained frame after mixing the channels.
  // 混合声道后，每个保留帧保存一个 PCM 采样。
  const sampleBytes = frameCount * PCM_SAMPLE_BYTES;
  // Allocate the header and sample data together for a single WAV blob.
  // 为文件头和采样数据分配同一块缓冲区，生成一个 WAV Blob。
  const bytes = new ArrayBuffer(WAV_HEADER_BYTES + sampleBytes);
  // The view writes the RIFF identifiers and little-endian PCM fields into that buffer.
  // 通过该视图将 RIFF 标识和小端 PCM 字段写入缓冲区。
  const wav = new DataView(bytes);
  wav.setUint32(0, 0x52494646); // RIFF
  wav.setUint32(4, bytes.byteLength - 8, true);
  wav.setUint32(8, 0x57415645); // WAVE
  wav.setUint32(12, 0x666d7420); // fmt
  wav.setUint32(16, 16, true);
  wav.setUint16(20, 1, true);
  wav.setUint16(22, 1, true);
  wav.setUint32(24, audio.sampleRate, true);
  wav.setUint32(28, audio.sampleRate * PCM_SAMPLE_BYTES, true);
  wav.setUint16(32, PCM_SAMPLE_BYTES, true);
  wav.setUint16(34, PCM_SAMPLE_BYTES * 8, true);
  wav.setUint32(36, 0x64617461); // data
  wav.setUint32(40, sampleBytes, true);

  // Average channels before quantizing so either input channel can carry speech.
  // 量化前取各声道平均值，保留任一输入声道上的语音。
  const channels = Array.from({ length: audio.numberOfChannels }, (_, channel) => audio.getChannelData(channel));
  // Use quantized samples to avoid dropping a genuinely quiet recording by an arbitrary loudness cutoff.
  // 根据量化后的采样判断静音，避免以任意响度阈值丢弃声音较轻的录音。
  let hasSignal = false;
  for (let frame = 0; frame < frameCount; frame += 1) {
    // Sum this frame across all channels for the mono mix.
    // 累加本帧的各声道采样，用于混合成单声道。
    let sum = 0;
    for (const channel of channels) sum += channel[frame];
    // Clamp the quantized average so full-scale samples cannot wrap around.
    // 限制量化后平均值的范围，避免满幅采样发生溢出回绕。
    const sample = Math.round(Math.max(-32_768, Math.min(32_767, sum / channels.length * 32_768)));
    wav.setInt16(WAV_HEADER_BYTES + frame * PCM_SAMPLE_BYTES, sample, true);
    if (sample !== 0) hasSignal = true;
  }

  return hasSignal ? new Blob([bytes], { type: "audio/wav" }) : null;
}
