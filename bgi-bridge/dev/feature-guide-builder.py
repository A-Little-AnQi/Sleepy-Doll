"""Generate factual BGI declarations; procedures are read from plugin skills."""
import hashlib
import errno
import json
import os
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
AUDIT = ROOT / "bgi-bridge/dev/fixtures/host-features.json"
METHODS = ROOT / "bgi-bridge/dev/bgi-method-metadata.json"
HOST_DOCUMENTATION = ROOT / "bgi-bridge/managed/generated/host-documentation.json"
OUTPUT = ROOT / "plugins/bgi/resources/feature-index.json"
HOST_SURFACE = ROOT / "bgi-bridge/dev/fixtures/host-surface.json"


def content_bytes(path):
    # Git checkout 的换行和 UTF-8 BOM 不应让同一份 JSON 元数据失配。
    return path.read_text(encoding="utf-8-sig").encode("utf-8")


def ref(path, skill="bgi-operator"):
    return {"skill": skill, "path": "references/" + path}


def build(data, methods, documentation, surface):
    assert data.get("sourceRevision") == documentation.get("sourceRevision"), "Source audit and declarations use different revisions"
    assert data.get("sourceTreeHash") == documentation.get("sourceTreeHash"), "Source audit and declarations use different trees"
    assert surface.get("sourceRevision") == documentation.get("sourceRevision"), "Host methods and declarations use different revisions"
    assert surface.get("sourceTreeHash") == documentation.get("sourceTreeHash"), "Host methods and declarations use different trees"
    result = []
    revision = data["sourceRevision"]
    published_names = {item["name"] for item in data["commands"] if item.get("publicExposure", True)}
    for item in data["commands"]:
        if not item.get("publicExposure", True):
            continue
        result.append({"id": item["methodId"], "kind": "command",
            "title": item.get("label") or item.get("summary") or item["name"],
            "summary": item.get("summary") or item["name"], "keywords": [item["name"], item["owner"]],
            "availability": item["chainAssessment"], "methodIds": [item["methodId"]],
            "inputSources": item.get("parameters") or [],
            "dependencies": {key: item[key] for key in ["needsDialogInput", "usesSelection", "asyncVoid", "hasImplementation"]},
            "references": [ref("native-ui.md")], "source": {"path": item["source"], "line": item["line"]}})
    for item in data["settings"]:
        if not item.get("publicExposure", True):
            continue
        result.append({"id": item["methodId"], "kind": "setting", "title": item["summary"] or item["path"],
            "summary": item["summary"], "keywords": [item["path"], item["name"], item["owner"]],
            "availability": "sourceDeclaration", "methodIds": [item["methodId"]],
            "inputSources": [{"path": item["path"], "type": item["valueType"],
                "hasChangeHook": item["hasCustomChangeHook"], "sourceDefault": item.get("initial"), "sourceRange": item.get("range")}],
            "references": [ref("settings.md")], "source": {"path": item["source"], "line": item["line"]}})
    for item in data["views"]:
        owner = item["class"] or item["source"]
        result.append({"id": "view." + owner, "kind": "page",
            "title": owner.split(".")[-1] if item["class"] else Path(owner).stem,
            "summary": "页面／窗口及实际绑定", "keywords": [item["source"]], "availability": "sourceDeclaration",
            "inputSources": [], "bindings": {"commands": [binding for binding in item["commands"]
                if any(name in binding["binding"] for name in published_names)]},
            "references": [ref("native-ui.md")], "source": {"path": item["source"]}})
    for item in documentation.get("scriptBindings", []):
        result.append({"id": "js.binding." + item["alias"], "kind": "scriptApi", "title": item["alias"],
            "summary": "JS 实际注入：" + item["clrType"], "keywords": [item["alias"], item["clrType"], item["kind"]],
            "availability": "sourceDeclaration", "inputSources": [], "declaration": item,
            "references": [ref("writing.md", "bgi-javascript")], "source": {"path": item["source"], "line": item["line"]}})
    for item in data.get("uiDeclarations", []):
        result.append({"id": item["id"], "kind": "page", "title": item.get("label") or item.get("name") or item["control"],
            "summary": "界面绑定／事件：" + item["source"],
            "keywords": [item.get("label") or "", item.get("name") or "", item["control"]]
                + [field["binding"] for field in item["bindings"]],
            "availability": "sourceDeclaration", "inputSources": item["bindings"], "declaration": item,
            "references": [ref("native-ui.md")], "source": {"path": item["source"]}})
    for kind, records in [("scriptApi", data["scriptApis"]), ("resourceModel", data["resourceModels"])]:
        for item in records:
            if item.get("exposureReason"):
                continue
            result.append({"id": item["symbol"], "kind": kind, "title": item["summary"] or item["name"],
                "summary": item["summary"], "keywords": [item["name"], item["owner"]], "availability": "sourceDeclaration",
                "inputSources": item.get("parameters") or [{"type": item["valueType"]}],
                "declaration": {key: item.get(key) for key in ["owner", "name", "valueType", "isStatic", "writable", "jsonName", "initial"]},
                "references": [ref("writing.md", "bgi-javascript")] if kind == "scriptApi" else [ref("resources.md")],
                "source": {"path": item["source"], "line": item["line"]}})
    # These are direct compiler-derived type and member declarations, not workflow hints.
    by_id = {item["id"]: item for item in result}
    for name, definition in documentation["scriptTypes"].items():
        result.append({"id": "T:" + name, "kind": "scriptApi", "title": name.split(".")[-1],
            "summary": definition.get("summary") or name, "keywords": [name], "availability": "sourceDeclaration",
            "inputSources": [], "declaration": {key: value for key, value in definition.items() if key != "members"},
            "references": [ref("writing.md", "bgi-javascript")], "source": {"path": definition["source"], "line": definition["line"]}})
        for member in definition["members"]:
            prefix = {"method": "M", "constructor": "M", "property": "P", "field": "F"}[member["kind"]]
            parameters = member.get("parameters") or []
            suffix = "(" + ",".join(p.get("modifier", "") + " " + p["type"] if p.get("modifier") else p["type"] for p in parameters) + ")" if member["kind"] in ("method", "constructor") else ""
            symbol = prefix + ":" + name + "." + member["name"] + suffix
            declaration = dict(member, owner=name)
            if symbol in by_id:
                by_id[symbol]["declaration"] = declaration
                continue
            record = {"id": symbol, "kind": "scriptApi", "title": member["name"],
                "summary": member.get("summary") or (member["returnType"] + " " + member["name"]),
                "keywords": [name, member["name"]], "availability": "sourceDeclaration", "inputSources": parameters,
                "declaration": declaration, "references": [ref("writing.md", "bgi-javascript")],
                "source": {"path": member["source"], "line": member["line"]}}
            result.append(record)
            by_id[symbol] = record
    # Stable methods come from actual MethodRegistry descriptors, including new registrations.
    for method, item in sorted(methods.items()):
        guide = item["guide"]
        schema = item["inputSchema"]
        required = set(schema.get("required", []))
        result.append({"id": method, "kind": "stable", "title": guide["title"], "summary": guide["purpose"],
            "keywords": [method, item["group"]], "availability": "registrationSnapshot", "methodIds": [method],
            "effect": item["effect"], "readOnly": item["readOnly"],
            "inputSources": [{"name": key, "type": value.get("type"), "description": value.get("description"),
                "required": key in required} for key, value in schema.get("properties", {}).items()],
            "references": [ref("shared.md")], "source": item["source"]})
    ids = [item["id"] for item in result]
    assert len(ids) == len(set(ids)), "Duplicate capability ID"
    assert {item["methodId"] for item in data["stableEntries"]} <= methods.keys(), "Registration snapshot missing audited methods"
    for item in result:
        assert item["source"]["path"]
    host_types = {}
    existing = {item["id"] for item in result}
    for name, definition in surface.get("types", {}).items():
        profile = {key:definition[key] for key in ["name","source","line","summary","isAbstract","isStatic"] if key in definition}
        profile.update(kind="enum" if definition["kind"] == "enum" else "type", members=[], bases=definition.get("bases", []), baseResolution="sourceSyntaxOnly")
        if definition["kind"] == "enum":
            profile["enumValues"] = definition["values"]
        for constructor in definition.get("constructors", []):
            profile["members"].append(dict(constructor, name=".ctor", kind="constructor", returnType=name))
        host_types[name] = profile
        if "T:" + name not in existing:
            result.append({"id":"T:"+name,"kind":"hostApi","title":name.split(".")[-1],
                "summary":definition.get("summary") or "宿主源码类型声明；不代表可直接调用", "keywords":[name], "availability":"sourceDeclaration",
                "inputSources":[],"declaration":dict(profile, members=[]),"references":[ref("capabilities.md")],
                "source":{"path":definition["source"],"line":definition["line"]}})
    for method in surface.get("methods", {}).values():
        member = {key:method[key] for key in ["name","returnType","parameters","source","line","summary","isStatic","hasImplementation","asyncVoid"]}
        member.update(kind="method", declarationRole=method["kind"])
        host_types[method["owner"]]["members"].append(member)
        result.append({"id":"H:"+method["id"][2:], "kind":"hostApi","title":method["name"],
            "summary":method.get("summary") or method["returnType"]+" "+method["name"], "keywords":[method["owner"],method["name"]],
            "availability":"sourceDeclaration", "inputSources":method["parameters"], "declaration":dict(member,owner=method["owner"]), "references":[ref("capabilities.md")],
            "source":{"path":method["source"],"line":method["line"]}})
    for symbol, entry in documentation["entries"].items():
        if not symbol.startswith("P:"): continue
        owner, name = symbol[2:].rsplit(".",1)
        if owner in host_types:
            host_types[owner]["members"].append({"name":name,"kind":"property","returnType":entry.get("valueType"),
                "readable":True,"writable":entry.get("writable",False),"isStatic":entry.get("isStatic",False),
                "source":entry["source"],"line":entry["line"]})
    assert len(result) == len({item["id"] for item in result}), "Duplicate source capability ID"
    source_hash = hashlib.sha256(content_bytes(HOST_DOCUMENTATION) + b"\0" + content_bytes(HOST_SURFACE)).hexdigest()
    inputs_hash = hashlib.sha256(b"".join(hashlib.sha256(content_bytes(path)).digest() for path in [AUDIT, METHODS, HOST_DOCUMENTATION, HOST_SURFACE])).hexdigest()
    return {"format": 2, "sourceRevision": revision, "sourceTreeHash":documentation["sourceTreeHash"], "sourceSnapshotHash":source_hash, "indexInputsHash":inputs_hash, "scriptTypes": documentation["scriptTypes"], "hostTypes":host_types, "items": result}


def main():
    index = build(json.loads(AUDIT.read_text(encoding="utf-8-sig")), json.loads(METHODS.read_text(encoding="utf-8-sig")), json.loads(HOST_DOCUMENTATION.read_text(encoding="utf-8-sig")), json.loads(HOST_SURFACE.read_text(encoding="utf-8-sig")))
    encoded = json.dumps(index, ensure_ascii=False, separators=(",", ":"))
    if "--check" in sys.argv:
        if not OUTPUT.exists() or OUTPUT.read_text(encoding="utf-8") != encoded:
            raise SystemExit("功能索引已过期；运行 feature-guide-builder.py 后再验证")
    else:
        for attempt in range(8):
            try:
                OUTPUT.write_text(encoded, encoding="utf-8")
                break
            except OSError as error:
                # Windows 编译器短暂映射 include_str 输入时不能截断；只重试同一生成文件。
                if os.name != "nt" or error.errno not in {errno.EACCES, errno.EINVAL} or attempt == 7:
                    raise
                time.sleep(0.05 * (attempt + 1))
    print(json.dumps({"items": len(index["items"]), "bytes": OUTPUT.stat().st_size}))


if __name__ == "__main__":
    main()
