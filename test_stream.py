"""检查 GPT-SoVITS 流式输出的原始字节格式"""
import requests
import sys

BASE_URL = "http://127.0.0.1:9880"
REF_AUDIO = "D:/VS_python/TTS/可琳/v4/可琳/reference_audios/中文/emotions/【开心】你今天看起来很高兴呢.wav"

body = {
    "text": "こんにちは、今日もいい天気ですね。マスター、何をしていますか。",
    "text_lang": "ja",
    "ref_audio_path": REF_AUDIO,
    "prompt_lang": "ja",
    "prompt_text": "你今天看起来很高兴呢",
    "streaming_mode": True,
    "media_type": "wav",
}

print(f"POST /tts with streaming_mode=True")
print(f"ref_audio_path: {REF_AUDIO}")
print()

resp = requests.post(f"{BASE_URL}/tts", json=body, stream=True)
print(f"Status: {resp.status_code}")
print(f"Headers: {dict(resp.headers)}")
print()

chunk_index = 0
total_bytes = 0
for chunk in resp.iter_content(chunk_size=None):
    if chunk:
        print(f"  Chunk {chunk_index}: {len(chunk)} bytes")
        if chunk_index == 0:
            # 第一块：显示前 64 字节 hex
            preview = chunk[:64].hex()
            print(f"    First 64 bytes (hex): {preview}")
            # 检查 WAV header
            if len(chunk) >= 44:
                riff = chunk[0:4]
                wave = chunk[8:12]
                fmt_mark = chunk[12:16]
                audio_fmt = int.from_bytes(chunk[20:22], 'little')
                channels = int.from_bytes(chunk[22:24], 'little')
                sample_rate = int.from_bytes(chunk[24:28], 'little')
                bits_per_sample = int.from_bytes(chunk[34:36], 'little')
                data_mark = chunk[36:40] if len(chunk) >= 40 else b''
                print(f"    RIFF: {riff}, WAVE: {wave}, fmt: {fmt_mark}")
                print(f"    AudioFormat: {audio_fmt}, Channels: {channels}")
                print(f"    SampleRate: {sample_rate}, BitsPerSample: {bits_per_sample}")
                print(f"    data marker: {data_mark}")
                if riff == b'RIFF':
                    header_pcm = chunk[44:]
                    print(f"    Header PCM: {len(header_pcm)} bytes ({len(header_pcm)/2} samples @ 16bit)")
        else:
            # 后续块：检查对齐
            is_aligned = len(chunk) % 2 == 0
            print(f"    16-bit aligned: {is_aligned}, first 16 bytes hex: {chunk[:16].hex()}")
        chunk_index += 1
        total_bytes += len(chunk)

print(f"\nTotal: {total_bytes} bytes in {chunk_index} chunks")
