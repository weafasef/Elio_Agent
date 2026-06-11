"""GPT-SoVITS 流式输出分析"""
import requests
import struct

REF = 'D:/VS_python/TTS/可琳/v4/可琳/reference_audios/中文/emotions/【默认】扫厨已经完成了，不过没有发现瑞恩小姐。.wav'

# ── 非流式 ──
print('=== 非流式 ===')
r = requests.post('http://127.0.0.1:9880/tts', json={
    'text': 'こんにちは、今日もいい天気ですね。マスター、何をしていますか。',
    'text_lang': 'ja',
    'ref_audio_path': REF,
    'prompt_lang': 'ja',
    'prompt_text': '扫厨已经完成了，不过没有发现瑞恩小姐。',
    'streaming_mode': False,
    'media_type': 'wav',
})
print(f'Status: {r.status_code}, Length: {len(r.content)}')
if r.status_code != 200:
    print(r.text[:300])
    exit(1)

h = r.content[:44]
print(f'RIFF:{h[:4]} WAVE:{h[8:12]} fmt:{h[12:16]}')
ch = struct.unpack_from('<H', h, 22)[0]
sr = struct.unpack_from('<I', h, 24)[0]
bps = struct.unpack_from('<H', h, 34)[0]
ds = struct.unpack_from('<I', h, 40)[0]
print(f'Ch:{ch} SR:{sr} BPS:{bps} DataSize:{ds}')
print(f'Duration: {ds/(sr*ch*bps/8):.1f}s')
with open('test_nostream.wav', 'wb') as f:
    f.write(r.content)
print('Saved: test_nostream.wav')

# ── 流式 ──
print('\n=== 流式 ===')
r = requests.post('http://127.0.0.1:9880/tts', json={
    'text': 'こんにちは、今日もいい天気ですね。マスター、何をしていますか。',
    'text_lang': 'ja',
    'ref_audio_path': REF,
    'prompt_lang': 'ja',
    'prompt_text': '扫厨已经完成了，不过没有发现瑞恩小姐。',
    'streaming_mode': True,
    'media_type': 'wav',
}, stream=True)
print(f'Status: {r.status_code}')
hdr = None
for i, chunk in enumerate(r.iter_content(chunk_size=None)):
    if not chunk:
        continue
    if i == 0:
        hdr = chunk[:44]
        print(f'Chunk 0: {len(chunk)} bytes')
        print(f'  RIFF:{hdr[:4]} WAVE:{hdr[8:12]}')
        print(f'  Ch:{struct.unpack_from("<H", hdr, 22)[0]} SR:{struct.unpack_from("<I", hdr, 24)[0]}')
        print(f'  BPS:{struct.unpack_from("<H", hdr, 34)[0]}')
        pcm = chunk[44:]
        print(f'  Header PCM: {len(pcm)} bytes ({len(pcm)/2} @16bit)')
        # 保存为独立 WAV
        wav = bytearray(hdr)
        struct.pack_into('<I', wav, 4, 36 + len(pcm))
        struct.pack_into('<I', wav, 40, len(pcm))
        wav.extend(pcm)
        with open(f'test_stream_{i}.wav', 'wb') as f:
            f.write(wav)
    else:
        aligned = len(chunk) % 2 == 0
        # 检查是否是有效 PCM（不是 WAV header）
        is_header = chunk[:4] == b'RIFF'
        print(f'Chunk {i}: {len(chunk)} bytes, aligned={aligned}, has_header={is_header}')
        # 保存为独立 WAV
        wav = bytearray(hdr)
        struct.pack_into('<I', wav, 4, 36 + len(chunk))
        struct.pack_into('<I', wav, 40, len(chunk))
        wav.extend(chunk)
        with open(f'test_stream_{i}.wav', 'wb') as f:
            f.write(wav)

print('Done - check test_stream_*.wav files')
