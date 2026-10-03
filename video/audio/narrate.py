"""保存语音及服务返回的逐词边界；供画面和混音读取同一组提示点。"""
import asyncio
import hashlib
import json
import os
import pathlib
import subprocess
import edge_tts

ROOT = pathlib.Path(__file__).resolve().parents[2]
RECIPE = json.loads((ROOT / "video/recording.json").read_text(encoding="utf-8-sig"))
OUTPUT = ROOT / "target/video-build/audio/voice"

async def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    voice = RECIPE["narration"]["voice"]
    rate = RECIPE["narration"]["rate"]
    semaphore = asyncio.Semaphore(3)

    async def create(index, segment):
        identity = json.dumps([voice, rate, segment["text"], "WordBoundary-v1"], ensure_ascii=False)
        digest = hashlib.sha256(identity.encode()).hexdigest()[:20]
        media = OUTPUT / (digest + ".mp3")
        metadata = OUTPUT / (digest + ".json")
        async with semaphore:
            if not media.exists() or not metadata.exists() or media.stat().st_size < 1000:
                words = []
                partial = OUTPUT / (digest + ".partial")
                with partial.open("wb") as output:
                    speech = edge_tts.Communicate(segment["text"], voice, rate=rate, boundary="WordBoundary")
                    async for chunk in speech.stream():
                        if chunk["type"] == "audio":
                            output.write(chunk["data"])
                        elif chunk["type"] == "WordBoundary":
                            words.append({"text": chunk["text"], "start": chunk["offset"] / 10000, "end": (chunk["offset"] + chunk["duration"]) / 10000})
                if not words or partial.stat().st_size < 1000:
                    raise RuntimeError(f"旁白 {index + 1} 缺少有效声音或逐词边界，停止制作。")
                os.replace(partial, media)
                metadata.write_text(json.dumps(words, ensure_ascii=False, indent=2), encoding="utf-8")
        words = json.loads(metadata.read_text(encoding="utf-8"))
        seconds = float(subprocess.check_output(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1", str(media)], creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)).decode().strip())
        print(f"旁白 {index + 1}: {seconds:.2f}s，{len(words)} 个词边界", flush=True)
        return dict(segment, file=str(media), duration=seconds, words=words)

    clips = await asyncio.gather(*(create(i, segment) for i, segment in enumerate(RECIPE["narration"]["segments"])))
    (OUTPUT / "manifest.json").write_text(json.dumps(clips, ensure_ascii=False, indent=2), encoding="utf-8")

if __name__ == "__main__":
    asyncio.run(main())
