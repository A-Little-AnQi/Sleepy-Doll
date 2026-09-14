#!/usr/bin/env python3
"""把一段真实会话做成开发模式的「模拟对话」。

产出两份，取自同一次会话：

  `.sleepy-doll/replay.json`  录制下来的助手轮次，mock 后端按序回放
  `web/src/demo.ts`           只有那句开场白，前端用它替你发出第一条消息

演出走的是真实链路：前端提交 → mock 后端回放 → 事件流 → 界面。所以后端或链路出
问题，在演示里就能看见，而不是被一段前端动画盖过去。

    python scripts/build-demo-conversation.py [会话 ID 或标题片段]

数据来自 mock 库，先跑 seed-mock-db.py。
"""

import json
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATABASE = ROOT / ".sleepy-doll/mock.db"
REPLAY = ROOT / ".sleepy-doll/replay.json"
MODULE = ROOT / "web/src/demo.ts"


def conversations(connection):
    return list(
        connection.execute(
            "SELECT c.id, c.title, COUNT(m.id) AS n FROM conversations c "
            "LEFT JOIN messages m ON m.conversation_id = c.id "
            "GROUP BY c.id ORDER BY n DESC"
        )
    )


def pick(connection, needle):
    """标题命中时取消息最多的那条 —— 用户随时会新建同名对话，短的往往是试手的。"""
    hits = [
        row
        for row in conversations(connection)
        if row[0] == needle or needle in (row[1] or "")
    ]
    return hits[0] if hits else None


def main():
    if not DATABASE.is_file():
        raise SystemExit(f"找不到 mock 库：{DATABASE}\n先运行 python scripts/seed-mock-db.py")

    connection = sqlite3.connect(f"file:{DATABASE}?mode=ro", uri=True)
    if len(sys.argv) < 2:
        print("可选会话（把 ID 或标题片段作为参数传进来）：\n")
        for cid, title, count in conversations(connection):
            print(f"  {cid}\n    {title}  ·  {count} 条消息")
        return

    found = pick(connection, sys.argv[1])
    if found is None:
        raise SystemExit(f"没有匹配的会话：{sys.argv[1]}")

    rows = list(
        connection.execute(
            "SELECT role, content, tool_calls_json, reasoning_json FROM messages "
            "WHERE conversation_id = ? ORDER BY id",
            (found[0],),
        )
    )
    prompt = rows[0][1]

    turns, calls = [], 0
    for role, content, tool_calls, reasoning in rows:
        if role != "assistant":
            continue
        parsed = json.loads(tool_calls or "[]")
        calls += len(parsed)
        turns.append(
            {
                "text": content,
                "thinking": (json.loads(reasoning or "null") or {}).get("text", ""),
                "calls": [
                    {"id": call["id"], "name": call["name"], "arguments": call["arguments"]}
                    for call in parsed
                ],
            }
        )
    if not turns:
        raise SystemExit(f"这个会话没有助手轮次：{found[1]}")

    REPLAY.parent.mkdir(parents=True, exist_ok=True)
    REPLAY.write_text(
        json.dumps({"prompt": prompt, "turns": turns}, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    MODULE.write_text(
        "// 由 scripts/build-demo-conversation.py 生成：开发模式「模拟对话」的开场白。\n"
        "// 演出本身由 mock 后端回放 .sleepy-doll/replay.json 完成，这里只有这一句。\n\n"
        "/** 点开「模拟对话」时替你发出的第一条消息。 */\n"
        f"export const DEMO_PROMPT = {json.dumps(prompt, ensure_ascii=False)};\n",
        encoding="utf-8",
    )

    print(f"会话：{found[1]}  （{found[2]} 条消息）")
    print(f"开场白：{prompt}")
    print(f"回放：{REPLAY}  （{len(turns)} 轮助手回复，{calls} 次工具调用）")
    print(f"前端：{MODULE}")


if __name__ == "__main__":
    main()
