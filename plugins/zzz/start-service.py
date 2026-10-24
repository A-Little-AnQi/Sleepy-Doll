"""绝区零一条龙 MCP 服务启动器。"""
import argparse
import os
from pathlib import Path
import sys


def main() -> None:
    parser = argparse.ArgumentParser(
        prog='zzz-mcp',
        description='启动绝区零一条龙 MCP 服务，供 Sleepy Doll 或其他 MCP 客户端连接。',
    )
    parser.add_argument('--one-dragon-dir', required=True, type=Path, help='绝区零一条龙源码根目录')
    parser.add_argument('--port', type=int, default=23001, help='本机 MCP 服务端口，默认 23001')
    args = parser.parse_args()
    root = args.one_dragon_dir.resolve(strict=True)
    if not (root / 'src/zzz_od/backend/entry/server.py').is_file():
        raise SystemExit('未找到绝区零一条龙 MCP 服务入口。请检查 --one-dragon-dir 指向的源码目录。')
    runtime = root / 'target/dev/mcp'
    temporary = root / 'target/tmp/mcp'
    runtime.mkdir(parents=True, exist_ok=True)
    temporary.mkdir(parents=True, exist_ok=True)
    for name in ('TEMP', 'TMP', 'TMPDIR'):
        os.environ[name] = str(temporary)
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(root / 'src'))

    from one_dragon.utils import os_utils

    # 默认 logger 在首次导入时确定路径，随后恢复真正的资源和配置根目录。
    os_utils.set_work_dir(runtime)
    try:
        from one_dragon.utils import log_utils  # noqa: F401
    finally:
        os_utils.set_work_dir(root)

    from zzz_od.backend.entry.server import main as start_backend

    sys.argv = [sys.argv[0], '--host', '127.0.0.1', '--port', str(args.port)]
    start_backend()


if __name__ == '__main__':
    main()
