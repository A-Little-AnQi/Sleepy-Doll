import type { RunApproval } from "../../ipc/types";

export interface ApprovalChange {
  label: string;
  value: string;
}

export interface ApprovalPresentation {
  title: string;
  summary: string;
  changes: ApprovalChange[];
}

/** 值统一限长，超长标明省略；按 unicode 码点截断，避免劈开代理对。 */
const MAX_VALUE_CHARS = 300;
const VALUE_TRUNCATED = "…（已截断）";
/** 非 JSON 文件内容最多展示的行数。 */
const MAX_CONTENT_LINES = 40;
/** JSON 内容最多展开的条目数，超出显式标注省略数量。 */
const MAX_CONTENT_ITEMS = 40;
/** 内容嵌套展开的最大深度，超出显式标注。 */
const MAX_CONTENT_DEPTH = 8;

/** 叶子中文标签；其余保留完整参数名，不假造中文。 */
const LEAF_LABELS: Record<string, string> = {
  enableCombatTargeting: "战斗中持续索敌",
  lockLostWaitTime: "脱锁等待时间（秒）",
  fastCheckEnabled: "更快触发战斗结束检查",
  fastCheckParams: "快速检查触发条件",
  checkEndDelay: "检查战斗结束的延时",
  beforeDetectDelay: "按键触发后检查延时",
  blockCheckBeforeBattleSeconds: "战斗开始后禁止结束检查时长（秒）",
  partyName: "队伍",
  hurryAvatar: "赶路角色",
  combatScriptName: "战斗策略",
  path: "目标文件",
  groupName: "配置组",
  groupNames: "配置组",
  name: "名称",
  scriptGroupName: "任务",
  folderName: "文件夹",
  file: "文件",
  command: "命令",
  backup: "备份",
  jobId: "任务",
  recursive: "包含子目录",
  cwd: "运行位置",
  closeGameAfter: "完成后关闭游戏",
};

const KNOWN_PARAM_KEYS = new Set([
  "path",
  "groupName",
  "groupNames",
  "command",
  "backup",
  "jobId",
  "recursive",
  "cwd",
  "closeGameAfter",
]);

/** 敏感键名：secret/password/token/apiKey/privateKey/单独 key/cookie/authorization 等。
 *  单独 key 用分隔符边界匹配，避免误伤 monkey、keyboard 之类普通词。 */
const SECRET_KEY =
  /secret|password|passwd|token|api_?key|private_?key|authorization|cookie|credential|(^|[^a-z0-9])key([^a-z0-9]|$)/i;

export function limitValue(value: string): string {
  if ([...value].length <= MAX_VALUE_CHARS) return value;
  return [...value].slice(0, MAX_VALUE_CHARS).join("") + VALUE_TRUNCATED;
}

/** 深拷贝式脱敏视图：不改原参数；命中敏感键名的值全部遮住；
 *  content 里能解析成 JSON 的字符串同样按嵌套结构脱敏后再序列化回字符串。 */
export function safeApprovalArguments(args: unknown): unknown {
  return sanitize(args, false, 0);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function sanitize(value: unknown, secret: boolean, depth: number): unknown {
  if (depth > 12) return secret ? "（已隐藏）" : "…（层级过深，省略）";
  if (value === null) return secret ? "（已隐藏）" : null;
  if (Array.isArray(value)) {
    return value.map((item) => sanitize(item, secret, depth + 1));
  }
  if (isRecord(value)) {
    const out: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(value)) {
      const childSecret = secret || SECRET_KEY.test(key);
      if (key === "content" && typeof item === "string") {
        try {
          const parsed: unknown = JSON.parse(item);
          if (isRecord(parsed) || Array.isArray(parsed)) {
            out[key] = JSON.stringify(sanitize(parsed, childSecret, depth + 1));
            continue;
          }
        } catch {
          /* 不是 JSON 文本，按普通字符串处理 */
        }
      }
      out[key] = sanitize(item, childSecret, depth + 1);
    }
    return out;
  }
  if (typeof value === "string") {
    return secret ? "（已隐藏）" : limitValue(value);
  }
  return secret ? "（已隐藏）" : value;
}

/** 后端 presentation 里出现这些内部实现词（宿主/原生构造器/深拷贝/CAS/
 *  pathingConfig/同名同类型/有效战斗策略等实现或策略面规约）时，说明它
 *  描述的是实现规约而非用户影响，title/summary/changes 整体回退到按真实
 *  操作重算，不做机械词替换，也不保留夹带内部词的旧行。 */
const INTERNAL_JARGON =
  /宿主|原生|同名同类型|有效战斗策略|策略面|深拷贝|\bCAS\b|pathingConfig/;

/** 审批卡正文三件套：优先后端 presentation 的非空内容；空 title/summary/changes
 *  都回退到真实参数推导的可读文案，不做任何假 diff。
 *  binding.description 是给模型的接口说明，绝不进入用户 summary。 */
export function approvalPresentation(
  request: RunApproval["request"],
): ApprovalPresentation {
  const { methodId, arguments: args } = unwrapInvoke(
    request.methodId,
    request.arguments,
  );
  const presentation = request.presentation;
  const backendTitle = presentation?.title?.trim();
  const backendSummary = presentation?.summary?.trim();
  const jargonHit =
    (!!backendTitle && INTERNAL_JARGON.test(backendTitle)) ||
    (!!backendSummary && INTERNAL_JARGON.test(backendSummary));
  if (jargonHit) {
    // 任一端带内部规约词就整套重算，避免旧 generic 标题或英文参数行残留。
    return {
      title: fallbackTitle(methodId),
      summary: describeEffect(methodId, args),
      changes: changesFromArguments(args, methodId),
    };
  }
  const title = backendTitle || fallbackTitle(methodId);
  const summary = backendSummary || describeEffect(methodId, args);
  const changes =
    presentation?.changes?.length
      ? presentation.changes.map((change) => ({
          label: limitValue(change.label),
          value: limitValue(change.value),
        }))
      : changesFromArguments(args, methodId);
  return { title, summary, changes };
}

/** bgi.api.invoke 包装：真实方法在 arguments.methodId/arguments.arguments 里；
 *  只读出来用，不修改 request 本身（详情面板仍展示原始 JSON）。 */
function unwrapInvoke(
  methodId: unknown,
  args: unknown,
): { methodId?: string; arguments: unknown } {
  let id = typeof methodId === "string" ? methodId : undefined;
  let current = args;
  for (let depth = 0; depth < 3; depth += 1) {
    if (id !== "bgi.api.invoke" || !isRecord(current)) break;
    const innerId = current.methodId;
    const innerArgs = current.arguments;
    if (typeof innerId !== "string" || !isRecord(innerArgs)) break;
    id = innerId;
    current = innerArgs;
  }
  if (id === undefined) return { arguments: current };
  return { methodId: id, arguments: current };
}

/** 按真实方法名精确匹配标题，不猜。 */
function fallbackTitle(methodId?: string): string {
  const id = methodId ?? "";
  if (id === "bgi.user.write" || id === "workspace.write") return "修改配置文件";
  if (id === "workspace.delete") return "删除文件";
  if (id === "bgi.user.restore") return "恢复备份";
  if (id === "workspace.shell") return "运行命令";
  if (id === "bgi.job.cancel") return "停止任务";
  if (id === "bgi.run_script_group" || id === "bgi.run_script_groups")
    return "运行任务";
  if (id === "bgi.prepare_pathing_group") return "创建路线任务";
  if (id === "bgi.set_pathing_party") return "设置路线队伍";
  if (id === "bgi.exit_game") return "关闭原神";
  if (id === "bgi.start_game") return "启动原神";
  if (id === "" || id === "unknown") return "执行操作";
  return "执行操作";
}

/** 路线目录只展示 basename，完整 path 留给详情面板的原始 JSON。 */
function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? path;
}

/** 无 presentation（或 presentation 含内部实现词）时的真实影响说明，
 *  按方法与真实参数精确生成，不写客套话，不虚报未提供的字段。 */
function describeEffect(methodId: string | undefined, args: unknown): string {
  const id = methodId ?? "";
  if (isWriteMethod(id))
    return "更新所选文件的内容；以下列出将写入的内容，未核对旧值。";
  if (id === "workspace.delete") return "删除所列文件或目录。";
  if (id === "bgi.user.restore") return "使用所列备份恢复目标文件。";
  if (id === "workspace.shell") return "在所列位置运行以下命令。";
  if (id === "bgi.prepare_pathing_group") {
    const groupName = readString(args, ["groupName"]);
    // 无显式名称时由 C# 按路线名称自动生成（GroupNaming.Default 还会清理
    // 作者/日期等后缀），目录名本身不是精确任务名，不拿来冒充。
    return groupName
      ? `将所选路线保存为「${groupName}」任务，沿用 BetterGI 当前设置。`
      : "将所选路线保存为新任务，名称按路线名称自动生成，沿用 BetterGI 当前设置。";
  }
  if (id === "bgi.set_pathing_party") {
    const groupName = readString(args, ["groupName"]);
    const partyName = readString(args, ["partyName"]);
    // 未提供的字段不改已有值（已有赶路角色继续生效），只有显式空字符串
    // 才表示不使用技能赶路；两者不能混为一谈。
    const rawHurry = isRecord(args)
      ? typeof args.hurryAvatar === "string"
        ? args.hurryAvatar.trim()
        : undefined
      : undefined;
    const target = groupName ? `「${groupName}」` : "该配置组";
    const parts: string[] = [];
    if (partyName) parts.push(`把${target}的队伍切换为「${partyName}」`);
    if (rawHurry !== undefined) {
      parts.push(rawHurry ? `赶路使用「${rawHurry}」的技能` : "不使用技能赶路");
    }
    if (parts.length === 0) return `没有为${target}提供可更改项，不做更改。`;
    return `${parts.join("，")}。`;
  }
  if (id === "bgi.run_script_group") {
    const name = readString(args, ["name", "groupName", "scriptGroupName"]);
    return name ? `按顺序运行任务「${name}」。` : "按顺序运行所列任务。";
  }
  if (id === "bgi.run_script_groups") {
    const names = readGroupNames(args);
    if (names.length === 0) return "运行所选任务。";
    const list = `：${names.join("、")}`;
    // 只有显式 closeGameAfter=true 才说会关游戏；其余情况不附加否定句。
    const close = isRecord(args) && args.closeGameAfter === true;
    return close
      ? `按顺序运行 ${names.length} 组任务${list}，全部成功后关闭原神。`
      : `按顺序运行 ${names.length} 组任务${list}。`;
  }
  if (id === "bgi.exit_game") return "关闭原神。";
  if (id === "bgi.start_game") return "启动原神。";
  return "执行以下操作。";
}

/** 未知方法不展示 binding.description，也不把 raw methodId 暴露给用户。 */
function readString(args: unknown, keys: string[]): string | undefined {
  if (!isRecord(args)) return undefined;
  for (const key of keys) {
    const value = args[key];
    if (typeof value === "string" && value.trim() !== "") return value;
  }
  return undefined;
}

function readGroupNames(args: unknown): string[] {
  if (!isRecord(args)) return [];
  const value = args.groupNames ?? args.names;
  if (Array.isArray(value)) {
    return value.filter(
      (item): item is string => typeof item === "string" && item.trim() !== "",
    );
  }
  const single = readString(args, ["groupName", "name", "scriptGroupName"]);
  return single ? [single] : [];
}

/** write 类方法才有"将替换该文件内容"；delete/shell/restore 只列目标文件，不谎称替换。 */
function isWriteMethod(methodId?: string): boolean {
  return methodId === "bgi.user.write" || methodId === "workspace.write";
}

function changesFromArguments(
  args: unknown,
  methodId?: string,
): ApprovalChange[] {
  if (!isRecord(args)) return [];
  const specific = methodSpecificChanges(args, methodId);
  if (specific) return specific;
  const out: ApprovalChange[] = [];
  if (typeof args.path === "string") {
    // write 类正文只放目标 basename，完整路径留在详情原始 JSON；
    // delete/restore 保留完整路径作为真实目标。
    const shownPath = isWriteMethod(methodId) ? baseName(args.path) : args.path;
    const note = isWriteMethod(methodId) ? "（将更新该文件内容）" : "";
    out.push({
      label: LEAF_LABELS.path ?? "目标文件",
      value: limitValue(shownPath + note),
    });
  }
  for (const [key, value] of Object.entries(args)) {
    if (key === "path" || key === "content") continue;
    if (!KNOWN_PARAM_KEYS.has(key)) continue;
    const label = LEAF_LABELS[key] ?? key;
    if (SECRET_KEY.test(key)) {
      out.push({ label, value: "（已隐藏）" });
      continue;
    }
    const text = formatValue(value);
    if (text !== null) out.push({ label, value: limitValue(text) });
  }
  const content = args.content;
  if (typeof content === "string" || isRecord(content) || Array.isArray(content)) {
    const parsed = parseContent(content);
    if (parsed !== null) {
      out.push(...flattenContent(parsed));
    } else if (typeof content === "string") {
      out.push(...contentLinesChange(content));
    }
  }
  // 未命中已知参数时，把顶层原始参数逐项列出，保证真实参数可见；
  // 疑似代码/长文本（含换行或超长字符串）不进正文，细节留在详情的原始 JSON；
  // 无中文映射的英文原始字段不进普通正文，计数提示而非静默丢弃。
  if (out.length === 0) {
    let omittedUnknown = 0;
    for (const [key, value] of Object.entries(args)) {
      if (SECRET_KEY.test(key)) {
        out.push({ label: LEAF_LABELS[key] ?? key, value: "（已隐藏）" });
        continue;
      }
      const label = LEAF_LABELS[key];
      if (!label && !/[\u4e00-\u9fff]/.test(key)) {
        omittedUnknown += 1;
        continue;
      }
      const text = formatValue(value);
      if (text === null) continue;
      if (looksLikeCode(text)) continue;
      out.push({ label: label ?? key, value: limitValue(text) });
    }
    if (omittedUnknown > 0) {
      out.push({ label: "其它参数", value: `其余 ${omittedUnknown} 项见详情` });
    }
  }
  return out.slice(0, MAX_CONTENT_ITEMS);
}

/** 换行或多段脚本体量的字符串视为代码字段，正文不直接 dump。 */
function looksLikeCode(text: string): boolean {
  return text.includes("\n") || [...text].length > 200;
}

/** 已知 BGI 操作的用户可读变更行；返回 null 走通用参数罗列。
 *  正文只放任务名/路线目录 basename，完整 path 在详情原始 JSON 里。 */
function methodSpecificChanges(
  args: Record<string, unknown>,
  methodId?: string,
): ApprovalChange[] | null {
  const id = methodId ?? "";
  if (id === "bgi.prepare_pathing_group") {
    const out: ApprovalChange[] = [];
    const groupName = readString(args, ["groupName"]);
    // 显式名称 trim 后照用；缺省时名称由路线名称自动生成，不拿目录名冒充。
    out.push({
      label: "任务名",
      value: groupName ? groupName.trim() : "按路线名称自动生成",
    });
    const path = readString(args, ["path"]);
    if (path) out.push({ label: "路线目录", value: baseName(path) });
    return out;
  }
  if (id === "bgi.set_pathing_party") {
    const out: ApprovalChange[] = [];
    const groupName = readString(args, ["groupName"]);
    if (groupName) out.push({ label: "配置组", value: groupName });
    const partyName = readString(args, ["partyName"]);
    if (partyName) out.push({ label: "队伍", value: partyName });
    // 与摘要同口径：字段提供但为空 = 不使用技能赶路，未提供则不出行。
    if (isRecord(args) && typeof args.hurryAvatar === "string") {
      const hurry = args.hurryAvatar.trim();
      out.push({
        label: "赶路角色",
        value: hurry ? hurry : "不使用技能赶路",
      });
    }
    return out;
  }
  if (id === "bgi.run_script_group") {
    const name = readString(args, ["name", "groupName", "scriptGroupName"]);
    return name ? [{ label: "任务", value: name }] : [];
  }
  if (id === "bgi.run_script_groups") {
    const names = readGroupNames(args);
    const out: ApprovalChange[] = names.map((name, index) => ({
      label: `第 ${index + 1} 组`,
      value: name,
    }));
    // 只有显式 true 才展示该行，避免"否"式否定被当成一项更改。
    if (isRecord(args) && args.closeGameAfter === true) {
      out.push({ label: "收尾", value: "全部成功后关闭原神" });
    }
    return out;
  }
  return null;
}

function parseContent(content: unknown): unknown {
  if (typeof content === "string") {
    try {
      const parsed: unknown = JSON.parse(content);
      if (isRecord(parsed) || Array.isArray(parsed)) return parsed;
      return null;
    } catch {
      return null;
    }
  }
  return isRecord(content) || Array.isArray(content) ? content : null;
}

/** JSON content 按叶路径逐项列出"将写入：值"；无旧基线，不做 diff。
 *  条目数/深度超限时显式标注省略数量，不静默丢弃。 */
function flattenContent(value: unknown): ApprovalChange[] {
  const out: ApprovalChange[] = [];
  let omitted = 0;
  const walk = (node: unknown, prefix: string, depth: number) => {
    if (isRecord(node) || Array.isArray(node)) {
      const entries = isRecord(node)
        ? Object.entries(node)
        : node.map((item, index) => [String(index), item] as const);
      if (depth > MAX_CONTENT_DEPTH) {
        omitted += entries.length;
        if (out.length < MAX_CONTENT_ITEMS)
          out.push({
            label: prefix || "内容",
            value: `…（嵌套过深，该层 ${entries.length} 项省略）`,
          });
        return;
      }
      for (const [key, item] of entries) {
        if (out.length >= MAX_CONTENT_ITEMS) {
          omitted += 1;
          continue;
        }
        const path = prefix ? `${prefix}.${key}` : key;
        const childSecret = SECRET_KEY.test(key);
        if (childSecret) {
          out.push({ label: LEAF_LABELS[key] ?? path, value: "将写入：（已隐藏）" });
        } else if (isRecord(item) || Array.isArray(item)) {
          walk(item, path, depth + 1);
        } else {
          out.push({
            label: LEAF_LABELS[key] ?? path,
            value: `将写入：${limitValue(formatScalar(item))}`,
          });
        }
      }
    }
  };
  walk(value, "", 0);
  if (omitted > 0) {
    out.push({ label: "…", value: `其余 ${omitted} 项省略` });
  }
  return out;
}

function contentLinesChange(content: string): ApprovalChange[] {
  const lines = content.split(/\r?\n/);
  const shown = lines.slice(0, MAX_CONTENT_LINES);
  const rest = lines.length - shown.length;
  const value =
    rest > 0
      ? `${shown.join("\n")}\n…（其余 ${rest} 行省略）`
      : shown.join("\n");
  return [{ label: "内容", value: limitValue(value) }];
}

/** 技术参数保持原输入文本，只在 details 层做脱敏/限长。 */
function formatValue(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(safeApprovalArguments(value));
}

function formatScalar(value: unknown): string {
  if (value === null || value === undefined) return String(value);
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}
