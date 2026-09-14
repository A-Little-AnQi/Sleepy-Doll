#!/usr/bin/env python3
"""把真实会话库灌进 mock 的库，供浏览器开发模式复现。

mock 后端（`npm run mock`）绑的是真正的 AppController，所以只要把库换成真实数据的
一致快照，`npm run dev` 看到的就是完全一致的会话 —— 调界面时不必调用真实模型。

    python scripts/seed-mock-db.py [源库路径]

默认取 `dist/Sleepy-Doll/user/.sleepy-doll/sleepy-doll.db`，写出 `.sleepy-doll/mock.db`
以及同名的附件目录。两个路径都在 .gitignore 里。

用 SQLite 的备份接口而不是复制文件：源库通常带着未合并的 WAL，直接复制会丢掉最近的提交。
"""

import os
import shutil
import sqlite3
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "dist/Sleepy-Doll/user/.sleepy-doll/sleepy-doll.db"
TARGET = ROOT / ".sleepy-doll/mock.db"
TABLES = [
    "conversations",
    "messages",
    "runtime_runs",
    "runtime_events",
    "runtime_message_owners",
    "tool_calls",
]


def artifacts(path: Path) -> Path:
    return path.with_suffix(".artifacts")


def seed(source: Path, target: Path) -> None:
    if not source.is_file():
        raise SystemExit(f"源库不存在：{source}\n先在应用里产生一次会话，或用参数指定路径。")

    target.parent.mkdir(parents=True, exist_ok=True)
    for path in (target, Path(f"{target}-wal"), Path(f"{target}-shm")):
        path.unlink(missing_ok=True)

    started = time.time()
    with sqlite3.connect(f"file:{source}?mode=ro", uri=True) as src:
        with sqlite3.connect(target) as dst:
            src.backup(dst)
    print(f"库：{source}\n → {target}  ({target.stat().st_size / 1024 / 1024:.1f} MB, {time.time() - started:.2f}s)")

    source_artifacts, target_artifacts = artifacts(source), artifacts(target)
    if source_artifacts.is_dir():
        if target_artifacts.is_dir():
            shutil.rmtree(target_artifacts)
        shutil.copytree(source_artifacts, target_artifacts)
        count = sum(len(files) for _, _, files in os.walk(target_artifacts))
        print(f"附件：{source_artifacts}\n → {target_artifacts}  ({count} 个文件)")

    # 逐表对数，确认快照完整而不是「看起来跑完了」。
    with sqlite3.connect(f"file:{source}?mode=ro", uri=True) as src, sqlite3.connect(
        f"file:{target}?mode=ro", uri=True
    ) as dst:
        print("\n%-24s %8s %8s" % ("表", "源库", "mock"))
        for table in TABLES:
            try:
                left = src.execute(f"select count(*) from {table}").fetchone()[0]
                right = dst.execute(f"select count(*) from {table}").fetchone()[0]
            except sqlite3.OperationalError:
                continue
            mark = "" if left == right else "   ← 不一致"
            print("%-24s %8d %8d%s" % (table, left, right, mark))


if __name__ == "__main__":
    seed(Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else SOURCE, TARGET)
