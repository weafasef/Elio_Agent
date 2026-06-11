#!/usr/bin/env bun
/**
 * TTS Bridge — Bun 子进程，由 elio-server (Rust) 调用来完成流式 TTS 合成
 *
 * 利用 Bun 的 fetch() + ReadableStream 保留 HTTP chunk 边界（每句一个 chunk），
 * 避免 Rust reqwest 合并/拆分 chunk 导致的噪声问题。
 *
 * 用法:
 *   echo '<json>' | bun run tts-bridge.ts
 *
 * 输入 (stdin JSON):
 *   { "text": "...", "ref_audio_path": "...", "prompt_text": "...",
 *     "prompt_lang": "ja", "lang": "ja" }
 *
 * 输出 (stdout JSON lines):
 *   {"type":"chunk","index":0,"data":"<base64 wav>"}
 *   {"type":"chunk","index":1,"data":"<base64 wav>"}
 *   {"type":"done","count":2}
 *
 * 参考: Elio_Agent/src/server/services/ttsService.ts (v1)
 */

const TTS_API = 'http://127.0.0.1:9880/tts';

// ── 读取 stdin JSON ──────────────────────────────────────────────────────────

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString('utf-8');
}

// ── 输出 JSON 到 stdout（确保每行独立写入） ─────────────────────────────────────

function emit(obj: Record<string, unknown>) {
  // 用 write() 而不是 console.log() 避免 Bun 的异步输出缓冲
  const line = JSON.stringify(obj) + '\n';
  const encoder = new TextEncoder();
  Bun.stdout.write(encoder.encode(line));
}

// ── 主函数 ─────────────────────────────────────────────────────────────────────

async function main() {
  const input = JSON.parse(await readStdin());

  const text: string = input.text;
  const refAudioPath: string = input.ref_audio_path || '';
  const promptText: string = input.prompt_text || '';
  const promptLang: string = input.prompt_lang || 'ja';
  const lang: string = input.lang || 'ja';

  // 流式请求体 — streaming_mode=true + parallel_infer=false
  // parallel_infer=true 会每句话内部切子段 → 多个 chunk → 播放碎片化
  // parallel_infer=false → 每句话一个完整 PCM chunk → 干净播放
  const body = {
    text,
    text_lang: lang,
    ref_audio_path: refAudioPath,
    prompt_lang: promptLang,
    prompt_text: promptText,
    media_type: 'wav',
    streaming_mode: true,
    parallel_infer: false,
  };

  const res = await fetch(TTS_API, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

  if (!res.ok) {
    const errText = await res.text().catch(() => res.statusText);
    emit({ type: 'error', message: `HTTP ${res.status}: ${errText}` });
    process.exit(1);
  }

  const reader = res.body!.getReader();
  let wavHeader: Uint8Array | null = null;
  let chunkIndex = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!value || value.length === 0) continue;

    // 第一块：提取 WAV header (44 bytes) + 首句 PCM
    if (!wavHeader) {
      if (value.length < 44) continue;
      wavHeader = value.slice(0, 44);
      const pcm = value.slice(44);
      if (pcm.length > 0) {
        // 构造完整 WAV
        const wav = buildWav(wavHeader, pcm);
        emit({
          type: 'chunk',
          index: chunkIndex,
          data: Buffer.from(wav).toString('base64'),
        });
        chunkIndex++;
      }
      continue;
    }

    // 后续块：每块 = 一个完整句子的 PCM
    if (value.length > 0) {
      const wav = buildWav(wavHeader, value);
      emit({
        type: 'chunk',
        index: chunkIndex,
        data: Buffer.from(wav).toString('base64'),
      });
      chunkIndex++;
    }
  }

  emit({ type: 'done', count: chunkIndex });
}

// ── WAV 构造（与 Rust 侧 build_wav 逻辑一致） ──────────────────────────────────

function buildWav(header: Uint8Array, pcm: Uint8Array): Uint8Array {
  const wav = new Uint8Array(header.length + pcm.length);
  wav.set(header, 0);
  wav.set(pcm, header.length);

  // 更新 RIFF size (offset 4): 36 + pcm.length
  const riffSize = new Uint8Array(4);
  const dv = new DataView(riffSize.buffer);
  dv.setUint32(0, 36 + pcm.length, true);
  wav.set(riffSize, 4);

  // 更新 data size (offset 40)
  const dataSize = new Uint8Array(4);
  const dv2 = new DataView(dataSize.buffer);
  dv2.setUint32(0, pcm.length, true);
  wav.set(dataSize, 40);

  return wav;
}

main().catch((err) => {
  emit({ type: 'error', message: err instanceof Error ? err.message : String(err) });
  process.exit(1);
});
