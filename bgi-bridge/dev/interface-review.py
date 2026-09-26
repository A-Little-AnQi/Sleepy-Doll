"""生成完整 BGI 公共接口审阅表；输入是源码盘点及真实类型只读契约。"""
import argparse
import collections
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def cell(value):
    return str(value or "—").replace("|", "\\|").replace("\n", " ").replace("\r", " ")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("contracts", type=Path)
    args = parser.parse_args()
    source = json.loads((ROOT / "docs/bgi/feature-coverage.json").read_text(encoding="utf-8"))
    contracts = json.loads(args.contracts.read_text(encoding="utf-8"))
    native = {"cmd." + item["Name"]: item for item in contracts["commands"]}
    stable = {item["Id"]: item for item in contracts["stable"]}
    assert set(stable) == {item["methodId"] for item in source["stableEntries"]}
    removed = [item for item in source["commands"] if not item["publicExposure"]]
    retired_settings = [item for item in source["settings"] if not item["publicExposure"]]
    assert not ({item["methodId"] for item in removed} & native.keys())
    for item in source["commands"]:
        if item["publicExposure"]:
            assert item["methodId"] in native, item["methodId"]
    rust = (ROOT / "src/bridge/mod.rs").read_text(encoding="utf-8")
    start = rust.index("pub fn register_tools(")
    end = rust.index("    for (name, label, description, schema, function)", start)
    tool_pattern = r'\(\s*"(bgi\.[\w.]+)"\s*,\s*"([^"\n]+)"\s*,\s*"([^"\n]+)"'
    tools = [{"id": match[1], "title": match[2], "summary": match[3], "source": "src/bridge/mod.rs",
              "line": rust[:start + match.start()].count("\n") + 1} for match in re.finditer(tool_pattern, rust[start:end])]
    assert len(tools) == len({item["id"] for item in tools})
    output = {"sourceRevision": source["sourceRevision"], "sourceAssembly": contracts["sourceAssembly"],
              "validation": "源码与只读类型／契约检查；不等同于实机全功能执行", "agentTools": tools,
              "stableInterfaces": [stable[key] for key in sorted(stable)], "nativeCommands": [native[key] for key in sorted(native)],
              "removedCommands": removed, "settings": source["settings"], "scriptApis": source["scriptApis"],
              "resourceModels": source["resourceModels"], "views": source["views"]}
    doc = ROOT / "docs/bgi/interface-review.md"
    (doc.with_suffix(".json")).write_text(json.dumps(output, ensure_ascii=False, indent=2), encoding="utf-8")
    lines = ["# BGI 全部接口审阅表", "", "审阅范围是 Sleepy Doll 的 BGI 插件与桥。BGI 本体源码保持原样；删除的是插件补造的旧实现及公开接口，宿主原有用户配置文件不迁移、不删字段。",
             "", f"源码版本：`{source['sourceRevision']}`。真实类型检查：`{contracts['sourceAssembly']}`。", "",
             f"公开 Agent 工具 {len(tools)} 个、稳定桥接口 {len(stable)} 个、动态命令 {len(native)} 个。源码命令 319 项逐项审查后，移除 {len(removed)} 项；622 个设置叶节点移除 {len(retired_settings)} 项。继承别名和源码清单的数量不同，真实程序集动态命令由上一阶段的 343 项减少为 {len(native)} 项。", "",
             "完整 Schema、参数、前置条件、副作用与核验规则见 [结构化接口审阅表](interface-review.json)。每一项命令和设置均在下面列出；运行时仍以当前实例的 api.describe 为准。没有执行全部游戏任务、通知测试、升级、账号操作或资源删除，不能把源码审查称为实机全通过。", "",
             "## 删除规则与替代链路", "",
             "- `[Obsolete]`、空方法、仅注释占位、已无界面绑定且无源码调用的旧入口不发布。",
             "- 生命周期／鼠标／下拉框等内部输入事件不作为独立业务 API。",
             "- 没有输入和收尾适配的模态编辑窗口／ContentDialog 不发布；反射发现不代表能自动执行。",
             "- 未进入源码审查记录的宿主新成员不自动发布，避免升级后又复活旧功能。",
             "- 旧跟踪按钮和旧教程入口已删除；正常地图追踪走 resolve → repo/subscription → prepare_pathing_group → run_script_group。",
             "- 空测试按钮删除；图像测试保留当前原生 start_capture_test。旧表单删除；设置、资源编辑与当前窗口的真实业务入口保留。",
             "- 设置的只读限制与功能弃用分开：缺 setter／安全写入 Schema 的有效设置保留读取，不伪装为可写。", "",
             "## Agent 可见工具", "", "| 工具 | 用途 | 定义 |", "|---|---|---|"]
    for item in tools:
        lines.append(f"| `{item['id']}` | {cell(item['summary'])} | {item['source']}:{item['line']} |")
    lines += ["", "## 稳定桥接口：全部保留", "", "| 接口 | 用途 | 效果 | 必需参数 | 核验 |", "|---|---|---|---|---|"]
    for key, item in sorted(stable.items()):
        lines.append("| " + " | ".join(map(cell, [key, item["Guide"]["Purpose"], item["Effect"], ", ".join(item["InputSchema"].get("required", [])), item["Guide"]["Verification"]])) + " |")
    lines += ["", "## 已删除的源码命令", "", "下面是删除证据，不是功能菜单，也不进入 Agent 功能索引。", "", "| 原入口 | 删除原因 | 源码 |", "|---|---|---|"]
    for item in removed:
        lines.append(f"| `{item['methodId']}` | {cell(item['exposureReason'])} | {item['source']}:{item['line']} |")
    lines += ["", "## 公开动态命令：全部列出", "", "每项保留真实上下文、参数和选择依赖。复杂对象使用真实 objectId；执行结果仍按契约核验。", "",
              "| 接口 | 用途 | 参数／依赖 | 核验 | 源码 |", "|---|---|---|---|---|"]
    for key, item in sorted(native.items()):
        dependencies = [item.get("ParameterType") or "无参数", "contextId"]
        if item["NeedsDialogInput"]:
            dependencies.append("dialogInput 必需")
        if item["SelectionSchema"].get("properties"):
            dependencies.append("selection：当前目标")
        if item["RequiresGameReady"]:
            dependencies.append("游戏截图器就绪")
        lines.append("| " + " | ".join(map(cell, [key, item["Guide"]["Purpose"], "; ".join(dependencies), item["Guide"]["Verification"], item["Guide"].get("SourceReference")])) + " |")
    lines += ["", "## 全部设置叶节点", "", "保留表示设置读取入口保留；是否能写由当前 writable、Schema 和联动事务决定。", "",
              "| 设置入口 | 处理 | 含义 | 类型／变更钩子 | 源码 |", "|---|---|---|---|---|"]
    for item in source["settings"]:
        state = "保留；当前契约判定可写性" if item["publicExposure"] else "已删除：" + item["exposureReason"]
        lines.append("| " + " | ".join(map(cell, [item["methodId"], state, item["summary"], str(item["valueType"]) + ("；有变更钩子" if item["hasCustomChangeHook"] else ""), f"{item['source']}:{item['line']}"])) + " |")
    lines += ["", "## 脚本 API、资源字段与页面", "", "脚本 API 和资源字段属于 JS 运行环境，不是 Agent 直接工具。完整逐项清单仍保留于 [源码功能清单](feature-coverage.md)，结构化审阅表包含全部 140 个脚本 API、35 个资源字段及 68 个页面。已弃用项目不生成 Agent 功能卡；源码盘点保留事实与删除原因。", "",
              "[当前完整调用链路图](agent-chain.md)已同步删除旧补接实现。"]
    doc.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(json.dumps({"agentTools": len(tools), "stable": len(stable), "nativeCommands": len(native), "removedSourceCommands": len(removed), "removedSettings": len(retired_settings)}))


if __name__ == "__main__":
    main()
