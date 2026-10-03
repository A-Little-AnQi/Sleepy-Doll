# 制作产品介绍视频

在项目根目录双击 **make-video.bat**。这是唯一的双击入口（支持 `check` / `director` 参数）。

默认制作完整 60 秒、1080p60、Qwen 中文旁白的产品介绍片。窗口显示进度；结束或出错后保留窗口。

## 唯一视频目录

成片固定为 `video/output/sleepy-doll.mp4`，同目录还有 `manifest.json`、`poster.jpg`、`subtitles.zh-CN.srt`。编码分段与临时 MP4 只写入该目录，成功后自动移除。中间数据都在 `target/video-build/`（UI、音频、reports、runtime、npm .cache），不使用系统临时目录。

## 语音与缓存

旁白由笔记本上的 Qwen3-TTS（`ssh 31931@10.11.45.183` → `127.0.0.1:9880/synth`，Kirara、chinese、seed 1234）合成，远端不落盘。**首次合成需要笔记本在线；缓存齐全后离线可制作**（缓存不完整时才会检查 SSH 可达性，不会因 health 预检失败挡住缓存复用）。12 段 WAV 缓存在 `target/video-build/audio/voice/`，清单为 `manifest-qwen-ssh.json`。提示点为句级能量 VAD 起止（`target/video-build/director-cues.json`，`wordTiming:false`），不是逐词对齐；语音不变速、不裁剪。

## 双击后依次执行

1. 清理旧产物（拒绝未知文件与越界路径），确认唯一视频目录。
2. 检查 Node 22.13+、FFmpeg/ffprobe、SSH、项目 npm 依赖与 Chromium；缺项明确报错，不自动安装。
3. 生成/复用 12 段 Qwen 旁白 → 构建导演提示点与字幕 → 合成 108 BPM 配乐与 SFX。
4. 类型检查并构建当前产品 UI（注入 director-cues）。
5. 导演审计（20 时点截图与真实检查）→ 混音（语音 -17 LUFS、音乐 .17 侧链 duck、SFX .8、两遍 -14 LUFS/TP -2）。
6. 校验覆盖（DirectorFilm 12000 + LateFilm `LATE_COVERAGE_MS=60000` + main 已使用 LateFilm）后逐帧录制 60s。
7. 移除临时文件，`video/output/` 只保留四个交付文件与 Qwen 语音缓存。

## 命令

```sh
node video/make-video.mjs --check     # 只读：清理范围、依赖、缓存状态
node video/make-video.mjs --director  # 仅 tsc/vite/导演截图（开发用，不录片）
node video/make-video.mjs --prepare   # 跑到审计+混音为止，保留构建供 QA
node video/make-video.mjs --preview   # 完整制作，30fps 草稿（时长不变）
node video/make-video.mjs             # 与双击入口相同
```

制作由 `.video-build.lock`（记录精确 PID）互斥；持有进程退出后自动接管陈旧锁。`--prepare` 保留 UI 构建供父助手检查；正式制作会重建并最终清理 UI/runtime/reports/音乐/混音中间件，仅保留四件交付物与 Qwen 语音缓存（旧 Edge 哈希 mp3/json 与 cues.json 在确认 Qwen 缓存完整后按清单移除，未知文件不动）。

成功双击会自动打开新 MP4；最终成片需人工视觉审核后交付。
