// 宣传片 12-60 秒九镜（D 审批门 / E 交接 / F 回执 / G 保存 / H 复用 /
// I 模型 / J 扩展 / K 本地桥 / L 收束）。
// 所有画面状态都是时钟 t（绝对毫秒）的纯函数：无 Date.now / Math.random / CSS transition。
// 实体件（纸条、金带、牌、印章、插头、月牙）用 DOM/SVG 做 2.5D；业务界面直接复用真实组件。
// 每镜顶部有 data-director-shot 便于导演审查；事件时间为本镜数据常量，可整体平移对齐旁白。
import { useEffect, useRef, useState, type CSSProperties, type ReactElement, type ReactNode } from "react";
import { useClock, readClock, seg, lerp, ease } from "../clock";
import { ProductWindow } from "../components/ProductWindow";
import { bootstrapAt, type FilmSnapshot } from "../film";
import type {
  Bootstrap,
  MessageInfo,
  TaskInfo,
  TaskSummary,
} from "../../web/src/ipc/types";
import { Transcript } from "../../web/src/components/chat/Transcript";
import { RunPlanCard } from "../../web/src/pages/chat/ChatPage";
import { ComposerDeck } from "../../web/src/components/chat/ComposerDeck";
import { ComposerField } from "../../web/src/components/chat/ComposerField";
import { Select } from "../../web/src/components/controls/Select";
import { ContextMeter } from "../../web/src/components/chat/ContextMeter";
import { SendIcon } from "../../web/src/components/icons";
import { TaskCard } from "../../web/src/components/tasks/TaskCard";
import { TasksPage } from "../../web/src/pages/tasks/TasksPage";
import { PREVIEW_PERMISSION } from "../../web/src/ipc/types";
import { ModelsPage } from "../../web/src/pages/settings/ModelsPage";
import { ExtensionsPage } from "../../web/src/pages/extensions/ExtensionsPage";
import { BridgePage } from "../../web/src/pages/bridge/BridgePage";
import { BrandIcon } from "../../web/src/brand/BrandIcon";
import { useT } from "../../web/src/i18n";
import "./late-film.css";

/** 本模块覆盖 [12000, 60000)；0-12 秒由导演段负责。 */
export const LATE_COVERAGE_MS = 60000;
const LATE_START_MS = 12000;

// ---- 全片共用底色与角色 ----
const NIGHT = "#101719";
const WARM = "#E8E5DD";

// ---- 演示数据（真实接口，只声明提交语义）----
const TOOL_LABELS: Record<string, string> = {
  "bgi.get_status": "读取状态",
  "bgi.run_script_group": "提交运行",
};
const CONV_ID = "conv-late-demo";
const USER_LINE = "运行演示配置组";
const SAVE_LINE = "创建快捷任务：演示配置组";
const STAMP_BASE = "2026-10-01T10:";
const mk = (m: number, s = 0) => `${STAMP_BASE}${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;

/** D 镜：接 11.8s 的计划，审批「启动 BetterGI 的演示配置组」。 */
const D = {
  windowIn: [12000, 12400] as const,
  titleIn: [12150, 12700] as const,
  planIn: [12200, 12600] as const,
  approvalIn: [12500, 13100] as const,
  latchTravel: [12900, 13200] as const, // 金带两段滑入，停在各侧
  pointerMove: [13600, 14150] as const,
  allowPress: [14200, 14450] as const, // 下压 2px，仅一次
  latchOut: [14350, 14800] as const, // 门闩沿自身轴向抽走
  slipThrough: [14400, 14900] as const, // 被挡的纸条穿过门
  approvalOut: [14650, 15200] as const,
  allowedNote: [14950, 15400] as const, // “已允许。”回到聊天
  bandRight: [14800, 16950] as const, // 金带继续指向右，走出画面
};
/** E 镜：任务交接（纸条穿过槽）。 */
const E = {
  railDraw: [17050, 17350] as const,
  platesIn: [17150, 17500] as const,
  sourceIn: [17300, 17700] as const, // 真实聊天消息作为纸条来源
  stripHead: [17500, 18300] as const,
  stripTail: [18300, 19000] as const,
  stripLand: [18800, 19050] as const, // 落入 BetterGI 侧
  bridgeIn: [19300, 19900] as const, // 轨道收成 BridgePage 已连接边框
  borderGold: [19900, 20300] as const,
  chatIn: [20050, 20500] as const, // 真实聊天提交回执（与 F 同位）
  titleOut: [19900, 20300] as const,
};
/** F 镜：回执收束（印章压一次 + 输入下一件事）。 */
const F = {
  stampPress: [23200, 23800] as const, // 只压一次
  stampSettle: [23800, 24000] as const,
  reframe: [23000, 23600] as const, // 聊天界面收成回执 + 输入框特写
  titleIn: [23100, 23500] as const,
  pointerToInput: [24800, 25250] as const,
  typing: [25300, 25950] as const,
  sendPress: [25960, 26100] as const,
  foldStrip: [26050, 26600] as const, // 对折
  intoClip: [26600, 27400] as const, // 装入短纸夹（交给 G）
  settle: [27400, 27950] as const,
};
/** G 镜：保存为快捷任务（三折压成 TaskCard）。 */
const G = {
  titleIn: [28100, 28600] as const,
  pull: [28250, 28750] as const, // 从纸夹拉出长纸条
  fold1: [28700, 29100] as const, // 第一折 14°
  fold2: [29150, 29500] as const, // 第二折 -11°（不同顺序与角度）
  compress: [29600, 30400] as const, // 压成 TaskCard 大小的入口
  cardIn: [30000, 30800] as const, // 真实 TaskCard 出现
  sealPress: [29800, 30100] as const, // 窄金封签压在纸夹角
  sealTick: [30000, 30130] as const, // 短清脆确认
  sourceIn: [30600, 31200] as const, // 原生聊天：已保存快捷任务
  listIn: [30800, 31600] as const, // 真实 TasksPage 列表
  cardMerge: [31200, 31800] as const, // 卡归位任务列表（31.8s 换真卡）
  pointerIn: [32600, 33300] as const, // 指针入场，接 H
};
/** H 镜：一键运行（固定特写）。 */
const H = {
  ticksIn: [34000, 34600] as const, // 目标/配置/确认 刻在金带上
  pointerMove: [34400, 34750] as const,
  runPress: [34900, 35050] as const,
  running: 35000, // 按钮「停止」、状态「运行中」
  ticksFold: [35000, 35600] as const, // 三标记折入卡背
  sloganIn: [35800, 36300] as const, // 一次保存。下次直达。
  switch1: 36600, // 运行中 → 等待工具完成
  switch2: 37300, // 等待工具完成 → 可停止
  railForm: [38200, 39600] as const, // 金带沿底边拉直成滑轨
};
/** I 镜：模型选择。云端/本地两词与 n09 语音对齐由导演最终调整，此处只留事件常量。 */
const I = {
  windowIn: [40000, 40400] as const,
  labelsIn: [40300, 40700] as const,
  railIn: [40600, 40950] as const,
  slide: [42000, 42650] as const, // 选择片一次滑动
  remountLocal: 42650, // 原生列表选中切到 Ollama（重挂载边界）
  editorZoom: [41800, 42400] as const, // 本地模型详情特写
  spineFold: [42800, 43400] as const, // 列表折成纵向书脊过场
};
/** J 镜：技能与插件（榫接扩展）。 */
const J = {
  pageIn: [45000, 45400] as const,
  spine1: [45500, 46200] as const, // 水平插接
  spine2: [46400, 47000] as const, // 垂直另一轴插接
  lineToDot: [47000, 47600] as const, // 金线走到 enabled 状态点
  dotGlow: [47500, 47900] as const,
  pluginsIn: [47600, 48000] as const, // 插件页真实行
  tab: [48000, 48600] as const, // 书脊缩收成面板侧边的实体 tab
  lineOut: [49000, 49950] as const, // 线延长去下一连接镜
};
/** K 镜：本地桥（连接导通）。 */
const K = {
  plugsIn: [50050, 50600] as const,
  dock: [50600, 51000] as const, // 51.0s 对接
  conduct: [51000, 51450] as const, // 金线由断到通，亮点走一次
  sink: [51800, 52600] as const, // 连接器沉成连接卡金边
  pageIn: [51800, 52400] as const,
  steady: [52600, 54300] as const,
  retract: [54300, 54950] as const, // 线退回月牙开口
};
/** L 镜：收束。 */
const L = {
  crescentIn: [55000, 55600] as const,
  coilIn: [55300, 56200] as const, // 金线盘回星体只一次
  nameIn: [55350, 56000] as const,
  settle: [56600, 57100] as const, // 旁白结束词时主句归位
  hold: [57100, 59000] as const,
  dim: [59000, 60000] as const, // 温和暗下 20%
};

const CLOUD_MODEL = {
  id: "model-glm",
  name: "智谱 GLM",
  protocol: "anthropic-messages",
  model: "glm-4.5",
  baseUrl: "https://open.bigmodel.cn/api/anthropic/v1",
  active: true,
  contextWindow: 128000,
};
const OLLAMA_MODEL = {
  id: "model-ollama",
  name: "Ollama 本机",
  protocol: "ollama-chat",
  model: "qwen3:4b",
  baseUrl: "http://127.0.0.1:11434",
  active: false,
  contextWindow: 32768,
};

/** 一份干净的演示 bootstrap：替换掉旧每日路线 fixture 的全部内容。 */
function lateBootstrap(patch: {
  workflows?: TaskSummary[];
  tasks?: TaskInfo[];
  models?: Bootstrap["models"];
  bridge?: Bootstrap["bridge"];
}): Bootstrap {
  const base = bootstrapAt(15000);
  return {
    ...base,
    conversations: [
      {
        id: CONV_ID,
        title: "演示配置组",
        createdAt: mk(0, 10),
        updatedAt: mk(12),
        taskCount: 1,
      },
    ],
    tools: [
      {
        name: "bgi.get_status",
        label: "读取状态",
        description: "读取一次截图器、游戏窗口与任务锁状态。",
        source: "bettergi",
      },
      {
        name: "bgi.run_script_group",
        label: "提交运行",
        description: "按 groupName 提交运行，默认只等启动交接。",
        source: "bettergi",
      },
    ],
    skills: [
      {
        name: "create-shortcut",
        description:
          "创建快捷任务：把已有的任务、配置或资源封装成一键入口。",
        source: "product",
        tags: ["快捷任务", "封装", "入口"],
        enabled: true,
        available: true,
      },
    ],
    plugins: [
      {
        manifest: {
          id: "bgi-host",
          name: "BetterGI 连接",
          version: "1.0.0",
          description: "把 BetterGI 的配置组接成本地工具。",
        },
        status: "enabled",
        configuredEnabled: true,
        host: true,
      },
    ],
    workflows: patch.workflows ?? [],
    tasks: patch.tasks ?? [],
    models: patch.models ?? [{ ...CLOUD_MODEL }],
    bridge: patch.bridge ?? {
      enabled: true,
      connected: false,
      baseUrl: "http://127.0.0.1:47124",
    },
  };
}

/** 快捷任务「演示配置组」：绑定真实接口，等待完成。 */
function demoWorkflow(stateLabel: string): TaskSummary {
  return {
    id: "wf-demo",
    name: "演示配置组",
    description: "运行指定的已有配置组",
    state: "readyVerified",
    stateLabel,
    actionLabel: "运行",
    shortcut: {
      applicationName: "BetterGI",
      targetName: "演示配置组",
      action: {
        tool: "bgi.run_script_group",
        arguments: { groupName: "演示配置组", waitForCompletion: true },
      },
    },
    runnable: true,
    pinned: true,
    sourceConversationId: CONV_ID,
    sourceTitleSnapshot: "演示配置组",
    sourceDeleted: false,
    publishedRevision: 1,
    revision: 1,
    modelUsage: "none",
    zeroToken: true,
    nodeCount: 1,
    updatedAt: mk(28),
    lastRunId: null,
    issue: null,
  };
}

function demoRun(state: TaskInfo["state"]): TaskInfo {
  return {
    id: "run-late-1",
    conversationId: CONV_ID,
    prompt: "演示配置组",
    state,
    createdAt: mk(34, 55),
    updatedAt: mk(34, 55),
    contextTokens: 96,
    contextWindow: 128000,
    inputTokens: 0,
    outputTokens: 0,
    source: { kind: "savedWorkflow", workflowId: "wf-demo", workflowRevision: 1 },
  };
}

/** D 镜批准后的短回执：原生 Transcript，只说“已允许”，不宣称业务结果。 */
function allowedMessages(): MessageInfo[] {
  return [
    { role: "user", content: USER_LINE, createdAt: mk(0, 10) },
    { role: "assistant", content: "已允许。", createdAt: mk(15) },
  ];
}

/** 审批特写内容：真实 .run-approval DOM（标题/描述/参数/允许/拒绝按钮）。 */
function ApprovalContent({
  description,
  progress,
  out,
}: {
  description: string;
  progress: number;
  out: number;
}) {
  const t = useT();
  const enter = ease.panel(progress);
  return (
    <section
      className="run-approval"
      style={{
        opacity: enter * (1 - out),
        transform: `translateY(${lerp(10, 0, enter)}px)`,
      }}
    >
      <h3>{t.chat.confirmExec}</h3>
      <p>{description}</p>
      <details>
        <summary>{t.chat.opParams}</summary>
        <pre>{`{\n  "groupName": "演示配置组"\n}`}</pre>
      </details>
      <div className="detail-actions">
        <button
          type="button"
          className="primary-action"
          data-video-anchor="late-allow"
          tabIndex={-1}
        >
          允许执行
        </button>
        <button type="button" className="secondary-action" tabIndex={-1}>
          拒绝执行
        </button>
      </div>
    </section>
  );
}

/** E/F 镜聊天消息：提交回执（accepted 只表示提交成功）。 */
function receiptMessages(): MessageInfo[] {
  return [
    { role: "user", content: USER_LINE, createdAt: mk(0, 10) },
    {
      role: "assistant",
      content: "",
      createdAt: mk(19, 10),
      toolCalls: [
        { id: "lc1", name: "bgi.get_status", arguments: {} },
        {
          id: "lc2",
          name: "bgi.run_script_group",
          arguments: { groupName: "演示配置组", waitForCompletion: false },
        },
      ],
    },
    { role: "tool", content: '{"ok":true}', toolCallId: "lc1" },
    { role: "tool", content: '{"accepted":true}', toolCallId: "lc2" },
    {
      role: "assistant",
      content: "已提交运行「演示配置组」。\n\n可以继续输入下一个目标。",
      createdAt: mk(19, 40),
    },
  ];
}

/** G 镜来源证据：创建快捷任务的对话。 */
function saveMessages(): MessageInfo[] {
  return [
    { role: "user", content: SAVE_LINE, createdAt: mk(26) },
    {
      role: "assistant",
      content: "已保存快捷任务「演示配置组」。在「快捷任务」里可直接运行。",
      createdAt: mk(27, 30),
    },
  ];
}

const filmIdle: FilmSnapshot = {
  page: "chat",
  welcome: false,
  welcomeFadeOut: 0,
  conversationFadeIn: 1,
  composerDraft: "",
  messages: [],
  seconds: 0,
  planEnter: 1,
  planShiftPx: 0,
  approvalVisible: false,
  approvalEnter: 0,
  approvalExit: 0,
  saveVisible: false,
  saveDisabled: false,
  savedCardVisible: 0,
  workflows: [],
  workflowBusy: false,
  toolLabels: TOOL_LABELS,
};

const noop = () => undefined;
const reload = async () => undefined;

/* ---- 真实 DOM 锚点：fonts.ready 后测一次、缓存 state，之后不再读变形 rect ---- */

export interface LateAnchor {
  x: number;
  y: number;
}
export interface LateRect {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface AnchorState {
  shot: string;
  /** 缺控件时置 false，根节点标 data-late-anchor-ready='0'，不静默回退猜测常量。 */
  ready: boolean;
  missing?: string;
  allow?: LateAnchor;
  approval?: LateRect;
  run?: LateAnchor;
}

/** 各镜需要测量的真实控件选择器。 */
const ANCHOR_SELECTORS: Record<string, { allow?: string; run?: string; slot?: string }> = {
  approval: { allow: ".lf-approval-slot .primary-action", slot: ".lf-approval-slot" },
  reuse: { run: ".tasks-page .task-card .primary-action" },
};

/* ---------------- 独立聊天适配：真实组件 + 真实 .run-approval DOM ---------------- */

function LateChatPane({
  messages,
  plan,
  approval,
  approvalProgress,
  approvalOut,
  draft,
  height,
}: {
  messages: MessageInfo[];
  plan?: Parameters<typeof RunPlanCard>[0]["plan"];
  approval?: string;
  approvalProgress: number;
  approvalOut: number;
  draft: string;
  height: number;
}) {
  const t = useT();
  return (
    <section className="chat-workspace lf-pane" style={{ height }}>
      <div className="chat-scene-switch">
        <div className="chat-scroll">
          <div className="conversation-scene">
            <div className="conversation-flow">
              <Transcript
                key={messages.map((m) => `${m.role}:${m.content}`).join("|")}
                messages={messages}
                stream=""
                phase={undefined}
                seconds={0}
                toolLabels={TOOL_LABELS}
              />
              {plan && (
                <div style={{ width: "100%", display: "grid", justifyItems: "start" }}>
                  <RunPlanCard plan={plan} />
                </div>
              )}
              {approval && (
                <div className="lf-approval-slot">
                  <ApprovalContent
                    description={approval}
                    progress={approvalProgress}
                    out={approvalOut}
                  />
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
      <div className="composer-dock">
        <ComposerDeck>
          <ComposerField
            aria-label={t.chat.message}
            placeholder={t.chat.composerPlaceholderNew}
            value={draft}
            tabIndex={-1}
            onChange={noop}
          />
          <div className="composer-actions">
            <div className="composer-menu composer-approval">
              <Select
                label={t.chat.approvalLevel}
                value="askEach"
                options={PREVIEW_PERMISSION.levels}
                onChange={noop}
              />
            </div>
            <div className="composer-submit">
              <ContextMeter used={64} window={128000} />
              <div className="composer-menu composer-model">
                <Select
                  label={t.chat.model}
                  value={CLOUD_MODEL.id}
                  options={[
                    {
                      value: CLOUD_MODEL.id,
                      label: CLOUD_MODEL.name,
                      description: CLOUD_MODEL.model,
                    },
                  ]}
                  onChange={noop}
                />
              </div>
              <button
                type="button"
                className="send-action"
                aria-label={t.chat.send}
                disabled={!draft.trim()}
                tabIndex={-1}
              >
                <SendIcon className="button-icon" />
              </button>
            </div>
          </div>
        </ComposerDeck>
      </div>
    </section>
  );
}

/** 缩放卡片：内部按 CSS 原尺寸布局，外层按 k 缩放；origin 左上。 */
function Scaled({
  left,
  top,
  width,
  height,
  k,
  className,
  style,
  children,
}: {
  left: number;
  top: number;
  width: number;
  height: number;
  k: number;
  className?: string;
  style?: CSSProperties;
  children: ReactNode;
}) {
  return (
    <div
      className={`lf-card ${className ?? ""}`}
      style={{ left, top, width: width * k, height: height * k, ...style }}
    >
      <div
        style={{
          width,
          height,
          transform: `scale(${k})`,
          transformOrigin: "0 0",
          position: "absolute",
          left: 0,
          top: 0,
        }}
      >
        {children}
      </div>
    </div>
  );
}

/** 44×54 实体指针；press 期间整体下压 2px；target 供审查标注。 */
function Pointer({
  x,
  y,
  press,
  target,
}: {
  x: number;
  y: number;
  press?: boolean;
  target?: string;
}) {
  return (
    <div
      className="lf-pointer"
      data-pointer-target={target}
      style={{ left: x - 5, top: y - 4, transform: press ? "translateY(2px)" : undefined }}
    >
      <svg width="44" height="54" viewBox="0 0 44 54" aria-hidden="true">
        <path
          d="M8 2 L8 44 L17.5 35.5 L24 50 L30 47.5 L23.5 33 L36 32 Z"
          className="lf-pointer-body"
        />
        <path
          d="M8 2 L8 44 L17.5 35.5 L24 50 L30 47.5 L23.5 33 L36 32 Z"
          fill="none"
          className="lf-pointer-edge"
        />
      </svg>
    </div>
  );
}

/** 纸条：金边 + 折角厚度；text 32px。 */
function PaperStrip({
  x,
  y,
  w,
  h,
  text,
  angle,
  alpha = 1,
}: {
  x: number;
  y: number;
  w: number;
  h: number;
  text: string;
  angle?: number;
  alpha?: number;
}) {
  return (
    <div
      className="lf-strip"
      style={{
        left: x,
        top: y,
        width: w,
        height: h,
        opacity: alpha,
        transform: angle ? `rotate(${angle}deg)` : undefined,
      }}
    >
      <span className="lf-strip-text">{text}</span>
      <span className="lf-strip-fold" />
    </div>
  );
}

/** 标题块：一到两行 60-68px；width 限制为左列宽度，不压右侧 UI。 */
function Title({
  x,
  y,
  lines,
  size,
  dark,
  progress,
  out,
  width,
}: {
  x: number;
  y: number;
  lines: string[];
  size: number;
  dark?: boolean;
  progress: number;
  out?: number;
  width?: number;
}) {
  const k = ease.camera(progress);
  return (
    <h2
      className={`lf-title${dark ? " is-dark" : ""}`}
      style={{
        left: x,
        top: y,
        fontSize: size,
        maxWidth: width,
        opacity: k * (1 - (out ?? 0)),
        transform: `translateY(${(1 - k) * 26}px)`,
      }}
    >
      {lines.map((line) => (
        <span key={line}>{line}</span>
      ))}
    </h2>
  );
}

/* ---------------- D 12-17s 审批门（纸折解锁） ---------------- */

// 审批特写卡：只有真实 .run-approval 组件（ApprovalContent），padding 32，
// k=1.5 时 heading≈24px 屏显、正文≈21px、按钮≈21px；批准后换成原生短回执。
const D_CARD = { x: 740, y: 315, w: 660, h: 245, k: 1.5 };
/** 批准后换回执的时刻（约 15s，审批淡出后）。 */
const D_RECEIPT_AT = 15100;

function ShotApproval({
  t,
  bg,
  allow,
  approval,
}: {
  t: number;
  bg: string;
  allow?: LateAnchor;
  approval?: LateRect;
}) {
  const approvalText = "启动 BetterGI 的演示配置组";
  const latchK = ease.panel(seg(t, D.latchTravel[0], D.latchTravel[1]));
  const outK = ease.panel(seg(t, D.latchOut[0], D.latchOut[1]));
  // 门闩接真实 approval 矩形：两段停在左右边外 24px，不横穿面板。
  const latchY = approval ? approval.y + approval.h : undefined;
  const leftFinal = approval ? approval.x - 24 - 300 : undefined;
  const rightFinal = approval ? approval.x + approval.w + 24 : undefined;
  const slip = seg(t, D.slipThrough[0], D.slipThrough[1]);
  const bandTail = ease.camera(seg(t, D.bandRight[0], D.bandRight[1]));
  const press = t >= D.allowPress[0] && t < D.allowPress[1];
  const pointerK = ease.ui(seg(t, D.pointerMove[0], D.pointerMove[1]));
  return (
    <div className="lf-shot" data-director-shot="approval" style={{ background: bg }}>
      <Title
        x={120}
        y={140}
        lines={["关键操作，", "由你确认。"]}
        size={68}
        progress={seg(t, D.titleIn[0], D.titleIn[1])}
      />
      <div style={{ opacity: seg(t, D.windowIn[0], D.windowIn[1]) }}>
        <Scaled
          left={D_CARD.x}
          top={D_CARD.y}
          width={D_CARD.w}
          height={D_CARD.h}
          k={D_CARD.k}
          className="lf-window"
        >
          {t < D_RECEIPT_AT ? (
            <div className="lf-approval-slot lf-approval-card">
              <ApprovalContent
                description={approvalText}
                progress={seg(t, D.approvalIn[0], D.approvalIn[1])}
                out={seg(t, D.approvalOut[0], D.approvalOut[1] + 300)}
              />
            </div>
          ) : (
            <div className="lf-transcript-crop">
              <Transcript
                messages={allowedMessages()}
                stream=""
                phase={undefined}
                seconds={0}
                toolLabels={TOOL_LABELS}
              />
            </div>
          )}
        </Scaled>
      </div>
      {/* 门闩：两段金带沿 approval 左右边停住；点击后沿自身轴向抽走 */}
      {latchY != null && leftFinal != null && rightFinal != null && t < D.latchOut[1] + 200 && (
        <>
          <div
            className="lf-latch"
            style={{
              left: lerp(leftFinal - 260, leftFinal, latchK) - outK * 420,
              top: latchY,
              width: 300,
              height: 12,
            }}
          />
          <div
            className="lf-latch"
            style={{
              left: lerp(rightFinal + 260, rightFinal, latchK) + outK * 420,
              top: latchY,
              width: 300,
              height: 12,
            }}
          />
        </>
      )}
      {/* 被阻挡的纸条在闩抽走后穿过门 */}
      {latchY != null && slip > 0 && slip < 1 && (
        <PaperStrip
          x={lerp(360, 1560, ease.camera(slip))}
          y={latchY - 30}
          w={260}
          h={40}
          text="演示配置组"
          angle={-2}
        />
      )}
      {/* 金带继续指向右，走出画面（接 E 镜轨道） */}
      {latchY != null && t >= D.bandRight[0] && t < LATE_COVERAGE_MS && (
        <div
          className="lf-goldline"
          style={{
            left: lerp(D_CARD.x + D_CARD.w * D_CARD.k, 1960, bandTail),
            top: latchY,
            width: 420,
            height: 6,
            opacity: 1 - seg(t, D.bandRight[1] - 200, D.bandRight[1]),
          }}
        />
      )}
      {allow != null && t >= D.pointerMove[0] && t < D.approvalOut[0] && (
        <Pointer
          x={lerp(1180, allow.x, pointerK)}
          y={lerp(320, allow.y, pointerK)}
          press={press}
          target="late-allow"
        />
      )}
    </div>
  );
}

/* ---------------- E 17-23s 交给宿主（机械交接） ---------------- */

const E_RAIL = { y: 560, x1: 200, x2: 1650 };

function ShotHandoff({ t, bg }: { t: number; bg: string }) {
  const railK = ease.panel(seg(t, E.railDraw[0], E.railDraw[1]));
  const plateK = (i: number) =>
    ease.panel(seg(t, E.platesIn[0] + i * 180, E.platesIn[1]));
  const tail = ease.camera(seg(t, E.stripTail[0], E.stripTail[1]));
  const land = ease.panel(seg(t, E.stripLand[0], E.stripLand[1]));
  // 纸条沿贝塞尔弧从聊天消息边缘穿过中间窄槽，落入 BetterGI 侧
  const bez = (u: number) => ({
    x: lerp(lerp(700, 925, 0.5), lerp(925, 1450, u), u),
    y: 505 - Math.sin(u * Math.PI) * 120 + land * 42,
    a: lerp(-4, 3, u),
  });
  const stripPos = bez(tail);
  const stripVis = t >= E.stripHead[0] && tail < 1;
  const chatK = seg(t, E.chatIn[0], E.chatIn[1]);
  // 阶段切换：19.3–19.9s 旧景（轨道/牌/槽/旧源聊天）整体退场，19.9s 后卸载；
  // BridgePage 同窗入场，20.05–20.5s 让位给回执并卸载，20.6s 起只剩回执。
  const legacyOut = seg(t, E.bridgeIn[0], E.bridgeIn[1]);
  const legacyOn = t < E.bridgeIn[1];
  return (
    <div className="lf-shot" data-director-shot="handoff" style={{ background: bg }}>
      <Title
        x={120}
        y={120}
        lines={["你下目标。", "工具负责执行。"]}
        size={68}
        progress={seg(t, 17050, 17500)}
        out={seg(t, E.titleOut[0], E.titleOut[1])}
      />
      {/* 金带轨道 */}
      {legacyOn && (
        <div
          className="lf-goldline"
          style={{
            left: E_RAIL.x1,
            top: E_RAIL.y,
            width: (E_RAIL.x2 - E_RAIL.x1) * railK,
            height: 8,
            opacity: 1 - legacyOut,
          }}
        />
      )}
      {/* 左：真实 Sleepy 品牌牌；右：BetterGI 字标牌（刻字、8px 厚板） */}
      {legacyOn && (
        <>
          <div
            className="lf-plate"
            style={{
              left: 340 - 165,
              top: E_RAIL.y - 78 + (1 - plateK(0)) * 40,
              width: 330,
              opacity: plateK(0) * (1 - legacyOut),
            }}
          >
            <BrandIcon className="lf-plate-icon" />
            <span>Sleepy Doll</span>
          </div>
          <div
            className="lf-plate is-host"
            style={{
              left: 1510 - 165,
              top: E_RAIL.y - 78 + (1 - plateK(1)) * 40,
              width: 330,
              opacity: plateK(1) * (1 - legacyOut),
            }}
          >
            <span>BetterGI</span>
          </div>
          {/* 中间窄槽：承载纸条的实体接口 */}
          <div
            className="lf-slot"
            style={{ left: 925 - 34, top: E_RAIL.y - 66, opacity: 1 - legacyOut }}
          />
        </>
      )}
      {/* 真实聊天消息作为纸条来源 */}
      {legacyOn && (
      <div style={{ opacity: seg(t, E.sourceIn[0], E.sourceIn[1]) * (1 - legacyOut) }}>
        <Scaled left={150} top={388} width={520} height={128} k={1.12} className="lf-window">
          <LateChatPane
            messages={[{ role: "user", content: USER_LINE, createdAt: mk(0, 10) }]}
            approvalProgress={0}
            approvalOut={0}
            draft=""
            height={128}
          />
        </Scaled>
      </div>
      )}
      {stripVis && (
        <PaperStrip
          x={stripPos.x - 275}
          y={stripPos.y - 44}
          w={550}
          h={88}
          text="演示配置组"
          angle={stripPos.a}
        />
      )}
      {/* 落位：纸条停在 BetterGI 侧牌前，BridgePage 出现时让位 */}
      {tail >= 1 && t < E.bridgeIn[0] && (
        <PaperStrip
          x={1180}
          y={E_RAIL.y + land * 42 - 44}
          w={550}
          h={88}
          text="演示配置组"
          alpha={1 - seg(t, E.bridgeIn[0] - 300, E.bridgeIn[0])}
        />
      )}
      {/* 轨道收成 BridgePage 已连接边框：19.3–19.9 入场，20.05–20.5 让位回执并卸载 */}
      {t >= E.bridgeIn[0] && t < E.chatIn[1] && (
        <div style={{ opacity: seg(t, E.bridgeIn[0], E.bridgeIn[1]) * (1 - chatK) }}>
          <ProductWindow
            t={t}
            box={{ x: 660, y: 300, w: 720, h: 502 }}
            viewport={{ width: 1060, height: 740 }}
            opacity={1}
            brandFade={0}
            blur={0}
            film={filmIdle}
            presses={{}}
            presentation={{
              page: "bridge",
              bootstrap: lateBootstrap({ bridge: { enabled: true, connected: true, baseUrl: "http://127.0.0.1:47124" } }),
              content: <BridgePage bootstrap={lateBootstrap({ bridge: { enabled: true, connected: true, baseUrl: "http://127.0.0.1:47124" } })} reload={reload} />,
            }}
          />
        </div>
      )}
      {t >= E.borderGold[0] && t < E.borderGold[1] + 200 && (
        <div
          className="lf-gold-frame"
          style={{
            left: 660,
            top: 300,
            width: 720,
            height: 502,
            opacity: seg(t, E.borderGold[0], E.borderGold[0] + 250) * (1 - seg(t, E.borderGold[1], E.borderGold[1] + 200)),
          }}
        />
      )}
      {/* 真实产品聊天：已提交运行（与 F 同位 match） */}
      {chatK > 0 && (
        <div style={{ opacity: chatK }}>
          <ReceiptStage typed={0} />
        </div>
      )}
    </div>
  );
}

/* ---------------- F 23-28s 回执收束（印压 + 下一件事） ---------------- */

// 回执与输入框两个原生特写（E 末尾与 F 共用同一位置）。
const F_RECEIPT = { x: 880, y: 150, w: 620, k: 1.45 };
const F_COMPOSER = { x: 880, y: 620, w: 620, k: 1.4 };
const F_INPUT_TARGET = { x: F_COMPOSER.x + 180, y: F_COMPOSER.y + 62 };
// 印章在回执右侧空白处（92×92），不压“已提交运行”正文；24.6s 后淡去。
const F_STAMP = { x: 1630, y: 245 };
const F_STAMP_FADE = [24600, 24900] as const;

function ReceiptStage({ typed }: { typed: number }) {
  // typed: 0..1，“创建快捷任务：演示配置组”的输入进度
  const line = SAVE_LINE;
  const draft = line.slice(0, Math.ceil(line.length * typed));
  return (
    <>
      <Scaled
        left={F_RECEIPT.x}
        top={F_RECEIPT.y}
        width={F_RECEIPT.w}
        height={196}
        k={F_RECEIPT.k}
        className="lf-window"
      >
        <div className="lf-transcript-crop">
          <Transcript
            messages={receiptMessages()}
            stream=""
            phase={undefined}
            seconds={0}
            toolLabels={TOOL_LABELS}
          />
        </div>
      </Scaled>
      <Scaled
        left={F_COMPOSER.x}
        top={F_COMPOSER.y}
        width={F_COMPOSER.w}
        height={128}
        k={F_COMPOSER.k}
        className="lf-window"
      >
        <LateChatPane
          messages={[]}
          approvalProgress={0}
          approvalOut={0}
          draft={draft}
          height={128}
        />
      </Scaled>
    </>
  );
}

function ShotReceipt({ t, bg }: { t: number; bg: string }) {
  const press = ease.press(seg(t, F.stampPress[0], F.stampPress[1]));
  const typed = seg(t, F.typing[0], F.typing[1]);
  const fold = ease.panel(seg(t, F.foldStrip[0], F.foldStrip[1]));
  const intoClip = ease.panel(seg(t, F.intoClip[0], F.intoClip[1]));
  const pointerK = ease.ui(seg(t, F.pointerToInput[0], F.pointerToInput[1]));
  return (
    <div className="lf-shot" data-director-shot="receipt" style={{ background: bg }}>
      <Title
        x={120}
        y={360}
        lines={["已提交运行"]}
        size={64}
        dark
        progress={seg(t, F.titleIn[0], F.titleIn[1])}
      />
      <p
        className="lf-sub"
        style={{
          left: 120,
          top: 470,
          opacity: seg(t, F.titleIn[0] + 300, F.titleIn[1] + 200),
        }}
      >
        继续安排下一件事。
      </p>
      <ReceiptStage typed={typed} />
      {/* 印章只压一次：压在回执右侧空白，正文始终可读，24.6s 后淡去 */}
      {t < F_STAMP_FADE[1] && (
        <div
          className="lf-stamp"
          style={{
            left: F_STAMP.x,
            top: F_STAMP.y - press * 10 + 6,
            transform: `scale(${lerp(1.18, 1, press)}) rotate(-8deg)`,
            opacity:
              seg(t, F.stampPress[0] - 150, F.stampPress[0] + 150) *
              (1 - intoClip) *
              (1 - seg(t, F_STAMP_FADE[0], F_STAMP_FADE[1])),
          }}
        >
          <span>已提交</span>
        </div>
      )}
      {/* 发送后：消息对折装入短纸夹（交给 G 镜） */}
      {t >= F.foldStrip[0] && (
        <div
          className="lf-foldpaper"
          style={{
            left: 980,
            top: 430,
            transform: `translate(${intoClip * 0}px, ${intoClip * 168}px) scale(${lerp(1, 0.7, intoClip)})`,
            opacity: 1 - seg(t, F.intoClip[1] - 250, F.intoClip[1]),
          }}
        >
          <div className="lf-foldpaper-a" style={{ transform: `rotateX(${fold * 85}deg)` }}>
            <span>创建快捷任务</span>
          </div>
          <div className="lf-foldpaper-b">
            <span>演示配置组</span>
          </div>
        </div>
      )}
      {/* 短纸夹：对折后收于此，G 镜从同一位置拉开 */}
      {t >= F.intoClip[0] - 400 && (
        <div
          className="lf-clip"
          style={{
            left: 950,
            top: 646,
            opacity: seg(t, F.intoClip[0] - 400, F.intoClip[0] + 200) * (1 - seg(t, F.settle[1] - 300, F.settle[1])),
          }}
        />
      )}
      {t >= F.pointerToInput[0] && t < F.intoClip[0] && (
        <Pointer
          x={lerp(560, F_INPUT_TARGET.x, pointerK)}
          y={lerp(900, F_INPUT_TARGET.y, pointerK)}
          press={t >= F.sendPress[0] && t < F.sendPress[1]}
        />
      )}
    </div>
  );
}

/* ---------------- G 28-34s 保存为快捷任务（三折压卡） ---------------- */

const G_CARD = { x: 780, y: 430, cssW: 460, k: 1.52 };
const G_LIST_BOX = { x: 880, y: 100, w: 940, h: 860 };

function ShotSave({ t, bg }: { t: number; bg: string }) {
  const pull = ease.camera(seg(t, G.pull[0], G.pull[1]));
  const fold1 = ease.panel(seg(t, G.fold1[0], G.fold1[1]));
  const fold2 = ease.panel(seg(t, G.fold2[0], G.fold2[1]));
  const compress = ease.panel(seg(t, G.compress[0], G.compress[1]));
  const merged = t >= G.cardMerge[1];
  const stripW = lerp(1080, 460, compress);
  const stripX = lerp(lerp(560, 420, pull), G_CARD.x + (G_CARD.cssW - stripW) / 2 * G_CARD.k, Math.max(compress, pull));
  const stripY = lerp(lerp(640, 480, pull), G_CARD.y, compress);
  const seal = ease.press(seg(t, G.sealPress[0], G.sealPress[1]));
  const tick = seg(t, G.sealTick[0], G.sealTick[1]);
  const emphasized = t >= G.cardIn[0] && !merged;
  const sourceK = seg(t, G.sourceIn[0], G.sourceIn[1]);
  const listK = seg(t, G.listIn[0], G.listIn[1]);
  const mergeK = ease.panel(seg(t, G.cardMerge[0], G.cardMerge[1]));
  const pointerK = ease.ui(seg(t, G.pointerIn[0], G.pointerIn[1]));
  return (
    <div className="lf-shot" data-director-shot="save" style={{ background: bg }}>
      <Title
        x={120}
        y={130}
        lines={["把常用操作，", "留在手边。"]}
        size={68}
        dark
        progress={seg(t, G.titleIn[0], G.titleIn[1])}
      />
      {/* 真实 TasksPage：卡最终归位的任务列表 */}
      {listK > 0 && (
        <div style={{ opacity: listK }}>
          <ProductWindow
            t={t}
            box={G_LIST_BOX}
            viewport={{ width: 1060, height: 740 }}
            opacity={1}
            brandFade={0}
            blur={0}
            film={filmIdle}
            presses={{}}
            presentation={{
              page: "tasks",
              bootstrap: lateBootstrap({ workflows: [demoWorkflow("可以运行")] }),
              content: (
                <TasksPage
                  bootstrap={lateBootstrap({ workflows: [demoWorkflow("可以运行")] })}
                  reload={reload}
                  onOpenConversation={noop}
                />
              ),
            }}
          />
        </div>
      )}
      {/* 三折纸条：两道折痕，先后不同角度 */}
      {t >= G.pull[0] && !merged && (
        <div
          className="lf-trifold"
          style={{
            left: stripX,
            top: stripY,
            width: stripW,
            transform: `scale(${lerp(1, G_CARD.k, compress)}) translateY(${lerp(0, 120, compress)}px)`,
            opacity: 1 - seg(t, G.compress[1] - 200, G.compress[1]),
          }}
        >
          <div className="lf-trifold-seg is-a">
            <span>创建快捷任务</span>
          </div>
          <div
            className="lf-trifold-seg is-b"
            style={{ transform: `rotateX(${fold1 * 14}deg)` }}
          >
            <span>：{SAVE_LINE.slice(7)}</span>
          </div>
          <div
            className="lf-trifold-seg is-c"
            style={{ transform: `rotateY(${fold2 * -11}deg)` }}
          >
            <span />
          </div>
        </div>
      )}
      {/* 压成的真实 TaskCard 入口（31.8s 归位列表后由真卡接管） */}
      {emphasized && (
        <div
          style={{
            position: "absolute",
            left: lerp(G_CARD.x, G_LIST_BOX.x + 460, mergeK),
            top: lerp(G_CARD.y, G_LIST_BOX.y + 200, mergeK),
            transform: `scale(${lerp(G_CARD.k, 0.9, mergeK)})`,
            transformOrigin: "0 0",
          }}
        >
          <div style={{ width: G_CARD.cssW }}>
            <TaskCard
              task={demoWorkflow("可以运行")}
              actions={{ run: noop, stop: noop, open: noop }}
            />
          </div>
        </div>
      )}
      {/* 窄金封签压在纸夹角 + 短促确认 */}
      {t >= G.sealPress[0] - 150 && !merged && (
        <div
          className="lf-seal-strip"
          style={{
            left: 962,
            top: 618 - seal * 6,
            transform: `rotate(-14deg) scale(${lerp(1.1, 1 + 0.05 * (1 - Math.abs(tick - 0.5) * 2), seal)})`,
            opacity: seg(t, G.sealPress[0] - 150, G.sealPress[0] + 150),
          }}
        />
      )}
      {/* 纸夹留在原地（与 F 同位） */}
      {t < G.compress[1] && (
        <div className="lf-clip" style={{ left: 950, top: 646 }} />
      )}
      {/* 原生来源证据：创建快捷任务的对话 */}
      {sourceK > 0 && (
        <div style={{ opacity: sourceK }}>
          <Scaled left={130} top={690} width={560} height={220} k={1.12} className="lf-window">
            <div className="lf-transcript-crop">
              <Transcript
                messages={saveMessages()}
                stream=""
                phase={undefined}
                seconds={0}
                toolLabels={TOOL_LABELS}
              />
            </div>
          </Scaled>
        </div>
      )}
      {t >= G.pointerIn[0] && (
        <Pointer x={lerp(320, 1210, pointerK)} y={lerp(940, 640, pointerK)} />
      )}
    </div>
  );
}


/* ---------------- H 34-40s 一键运行（固定特写） ---------------- */

const H_CROP = { x: 600, y: 110, w: 1180, h: 850 };
// 视口内 TaskCard 位置（CSS 坐标，约在任务网格首格）；box 用裁切内局部坐标居中卡片。
const H_CARD_VP = { x: 320, y: 180, w: 560 };
// 运行按钮中心来自真实 DOM 锚点（.tasks-page .task-card .primary-action），
// 文案变「停止」后仍是同一按钮节点，沿用缓存，不每帧读变形 rect。

function activeRunAt(t: number): TaskInfo | undefined {
  if (t < H.running) return undefined;
  const state: TaskInfo["state"] =
    t >= H.switch2 ? "running" : t >= H.switch1 ? "waitingJob" : "running";
  return demoRun(state);
}

function ShotReuse({ t, bg, run }: { t: number; bg: string; run?: LateAnchor }) {
  const bootstrap = lateBootstrap({
    workflows: [demoWorkflow(t >= H.running ? "运行中" : "可以运行")],
    tasks: activeRunAt(t) ? [activeRunAt(t)!] : [],
  });
  const pointerK = ease.ui(seg(t, H.pointerMove[0], H.pointerMove[1]));
  const press = t >= H.runPress[0] && t < H.runPress[1];
  const ticksFold = ease.panel(seg(t, H.ticksFold[0], H.ticksFold[1]));
  const railK = ease.panel(seg(t, H.railForm[0], H.railForm[1]));
  return (
    <div className="lf-shot" data-director-shot="reuse" style={{ background: bg }}>
      <div className="lf-crop" style={{ left: H_CROP.x, top: H_CROP.y, width: H_CROP.w, height: H_CROP.h }}>
        <ProductWindow
          t={t}
          box={{
            // 裁切内局部坐标：把任务网格首格的 TaskCard 居中放进裁切框
            x: (H_CROP.w - H_CARD_VP.w * 1.42) / 2 - H_CARD_VP.x * 1.42,
            y: 300 - H_CARD_VP.y * 1.42,
            w: 1060 * 1.42,
            h: 740 * 1.42,
          }}
          viewport={{ width: 1060, height: 740 }}
          opacity={1}
          brandFade={0}
          blur={0}
          film={filmIdle}
          presses={{}}
          presentation={{
            page: "tasks",
            bootstrap,
            content: <TasksPage bootstrap={bootstrap} reload={reload} onOpenConversation={noop} />,
          }}
        />
      </div>
      {/* 目标/配置/确认 三个小标记嵌在同一金带上，点击后一起折入卡背 */}
      {t < H.ticksFold[1] && (
        <div
          className="lf-ticks"
          style={{
            left: 760,
            top: 906,
            transform: `translateY(${ticksFold * 60}px) rotateX(${ticksFold * 80}deg)`,
            opacity: (1 - ticksFold) * seg(t, H.ticksIn[0], H.ticksIn[1]),
          }}
        >
          <span>目标</span>
          <span>配置</span>
          <span>确认</span>
        </div>
      )}
      <Title
        x={120}
        y={150}
        lines={["一次保存。", "下次直达。"]}
        size={60}
        dark
        progress={seg(t, H.sloganIn[0], H.sloganIn[1])}
      />
      {run != null && t >= H.pointerMove[0] && t < H.railForm[0] && (
        <Pointer
          x={lerp(1250, run.x, pointerK)}
          y={lerp(300, run.y, pointerK)}
          press={press}
          target="late-run"
        />
      )}
      {/* 金带沿底边拉直，形成下一场滑轨（接 I） */}
      {t >= H.railForm[0] && (
        <div
          className="lf-goldline"
          style={{ left: 140, top: 948, width: 1660 * railK, height: 8 }}
        />
      )}
    </div>
  );
}

/* ---------------- I 40-45s 模型选择（滑轨选择） ---------------- */

const I_BOX = { x: 600, y: 110, w: 1230, h: 858 };
const I_ZOOM = { x: 660, y: 130, w: 1120, h: 850 };

function ShotModels({ t, bg }: { t: number; bg: string }) {
  const local = t >= I.remountLocal;
  const models = local
    ? [{ ...OLLAMA_MODEL, active: true }, { ...CLOUD_MODEL, active: false }]
    : [{ ...CLOUD_MODEL }, { ...OLLAMA_MODEL }];
  const bootstrap = lateBootstrap({ models });
  const zoomK = ease.panel(seg(t, I.editorZoom[0], I.editorZoom[1]));
  const slide = ease.ui(seg(t, I.slide[0], I.slide[1]));
  const spineFold = ease.panel(seg(t, I.spineFold[0], I.spineFold[1]));
  const windowStyle: CSSProperties = {
    opacity: seg(t, I.windowIn[0], I.windowIn[1]),
  };
  return (
    <div className="lf-shot" data-director-shot="models" style={{ background: bg }}>
      <Title
        x={120}
        y={130}
        lines={["模型，", "由你选择。"]}
        size={64}
        width={440}
        progress={seg(t, I.windowIn[0] + 300, I.windowIn[0] + 800)}
      />
      <div style={windowStyle}>
        {/* 宽景：列表 + 编辑区；特写进入时淡出，特写定住后卸载，消除重影 */}
        {zoomK < 0.99 && (
          <div style={{ opacity: 1 - zoomK, position: "absolute", inset: 0 }}>
            <ProductWindow
              t={t}
              box={I_BOX}
              viewport={{ width: 1060, height: 740 }}
              opacity={1}
              brandFade={0}
              blur={0}
              film={filmIdle}
              presses={{}}
              presentation={{
                page: "models",
                bootstrap,
                content: (
                  <ModelsPage
                    key={local ? "local" : "cloud"}
                    bootstrap={bootstrap}
                    reload={reload}
                  />
                ),
              }}
            />
          </div>
        )}
      </div>
      {/* 特写：本地模型详情（当前 preset 基础字段） */}
      {zoomK > 0.02 && (
        <div
          className="lf-crop"
          style={{ left: I_ZOOM.x, top: I_ZOOM.y, width: I_ZOOM.w, height: I_ZOOM.h, opacity: zoomK }}
        >
          <ProductWindow
            t={t}
            box={{ x: -320 * 1.5, y: -60 * 1.5, w: 1060 * 1.5, h: 740 * 1.5 }}
            viewport={{ width: 1060, height: 740 }}
            opacity={1}
            brandFade={0}
            blur={0}
            film={filmIdle}
            presses={{}}
            presentation={{
              page: "models",
              bootstrap,
              content: (
                <ModelsPage
                  key={local ? "local-z" : "cloud-z"}
                  bootstrap={bootstrap}
                  reload={reload}
                />
              ),
            }}
          />
        </div>
      )}
      {/* 云端 / 本地 概念滑轨放左列，不穿模型字段：一次左→右选择动作 */}
      <div
        className="lf-rail-labels"
        style={{ opacity: seg(t, I.labelsIn[0], I.labelsIn[1]) }}
      >
        <span>云端</span>
        <span>本地</span>
      </div>
      <div
        className="lf-rail"
        style={{ opacity: seg(t, I.railIn[0], I.railIn[1]) }}
      />
      <div
        className="lf-selector"
        style={{ left: lerp(150, 400, slide), top: 515 }}
      />
      {/* 列表折成纵向书脊：去下一镜的过场 */}
      {spineFold > 0 && (
        <div
          className="lf-list-spine"
          style={{
            left: 640,
            top: 300,
            transform: `rotateY(${spineFold * 62}deg)`,
            opacity: spineFold * (1 - seg(t, I.spineFold[1], I.spineFold[1] + 250)),
          }}
        />
      )}
    </div>
  );
}

/* ---------------- J 45-50s 技能与插件（榫接扩展） ---------------- */

const J_BOX = { x: 650, y: 170, w: 1120, h: 780 };

function ShotExtensions({ t, bg }: { t: number; bg: string }) {
  const bootstrap = lateBootstrap({
    bridge: { enabled: true, connected: true, baseUrl: "http://127.0.0.1:47124" },
  });
  const spine1 = ease.panel(seg(t, J.spine1[0], J.spine1[1]));
  const spine2 = ease.panel(seg(t, J.spine2[0], J.spine2[1]));
  const lineK = ease.camera(seg(t, J.lineToDot[0], J.lineToDot[1]));
  const pluginsK = seg(t, J.pluginsIn[0], J.pluginsIn[1]);
  const tabK = ease.panel(seg(t, J.tab[0], J.tab[1]));
  const lineOut = ease.camera(seg(t, J.lineOut[0], J.lineOut[1]));
  return (
    <div className="lf-shot" data-director-shot="extensions" style={{ background: bg }}>
      <Title
        x={120}
        y={130}
        lines={["技能与插件，", "按需扩展。"]}
        size={60}
        dark
        width={440}
        progress={seg(t, J.pageIn[0] + 200, J.pageIn[0] + 700)}
      />
      <div style={{ opacity: seg(t, J.pageIn[0], J.pageIn[1]) }}>
        <ProductWindow
          t={t}
          box={J_BOX}
          viewport={{ width: 1060, height: 740 }}
          opacity={1}
          brandFade={0}
          blur={0}
          film={filmIdle}
          presses={{}}
          presentation={{
            page: "extensions",
            bootstrap,
            content: (
              <ExtensionsPage bootstrap={bootstrap} reload={reload} tab="skills" />
            ),
          }}
        />
      </div>
      {/* 左侧两枚米白书脊：技能 / 插件。进入方向不同，全部留在左区 x250..550、
          y380..780，再向扩展面板左边缘对接（停在 x576，不压真实插件名）；
          48s 后缩收成面板侧边的实体 tab */}
      <div
        className="lf-spine"
        style={{
          left: lerp(300, 576, spine1),
          top: 420,
          height: 400,
          transform: `translateX(${tabK * 26}px) scale(${lerp(1, 0.62, tabK)})`,
          transformOrigin: "top left",
          opacity: seg(t, J.pageIn[0], J.pageIn[0] + 400),
        }}
      >
        技能
      </div>
      <div
        className="lf-spine is-short"
        style={{
          left: 424,
          top: lerp(300, 620, spine2),
          height: 300,
          transform: `translateX(${tabK * 40}px) scale(${lerp(1, 0.6, tabK)})`,
          transformOrigin: "top left",
          opacity: seg(t, J.pageIn[0] + 150, J.pageIn[0] + 550),
        }}
      >
        插件
      </div>
      {/* 金线随连接走到真实 enabled 状态点 */}
      {t >= J.lineToDot[0] && t < J.lineOut[1] && (
        <div
          className="lf-goldline"
          style={{
            left: lerp(660, 1640, Math.min(lineK, 1)),
            top: 330,
            width: Math.max(40, (1640 - 660) * Math.min(lineK, 1) + 60),
            height: 5,
            opacity: 1 - seg(t, J.lineOut[0], J.lineOut[0] + 400),
          }}
        />
      )}
      {/* 状态点短时亮起后落定 */}
      <div
        className="lf-dot"
        style={{
          left: 1640,
          top: 318,
          opacity: seg(t, J.dotGlow[0], J.dotGlow[0] + 150) * (1 - 0.4 * seg(t, J.dotGlow[1], J.dotGlow[1] + 200)) + 0.4 * seg(t, J.dotGlow[0], J.dotGlow[0] + 150),
        }}
      />
      {/* 插件页真实行（BetterGI 连接） */}
      {pluginsK > 0 && (
        <div style={{ opacity: pluginsK }}>
          <Scaled left={140} top={640} width={460} height={300} k={1.05} className="lf-window">
            <ExtensionsPage bootstrap={bootstrap} reload={reload} tab="plugins" />
          </Scaled>
        </div>
      )}
      {/* 线从状态点延长至下一连接镜 */}
      {t >= J.lineOut[0] && (
        <div
          className="lf-goldline"
          style={{ left: 1640, top: 330, width: 180 * lineOut, height: 5 }}
        />
      )}
    </div>
  );
}

/* ---------------- K 50-55s 本地桥（连接导通） ---------------- */

function ShotBridge({ t, bg }: { t: number; bg: string }) {
  const dock = ease.panel(seg(t, K.dock[0], K.dock[1]));
  const conduct = seg(t, K.conduct[0], K.conduct[1]);
  const pageK = seg(t, K.pageIn[0], K.pageIn[1]);
  // 旧景（插头/连接器/金线）随 pageIn 退去，pageK>0.9 后全部卸载；
  // 全镜只有一个真实 BridgePage 实例，无推镜、无两页 alpha 叠加。
  const legacyOn = pageK < 0.9;
  const legacy = 1 - pageK;
  const retract = ease.camera(seg(t, K.retract[0], K.retract[1]));
  const bootstrap = lateBootstrap({
    bridge: { enabled: true, connected: true, baseUrl: "http://127.0.0.1:47124" },
  });
  return (
    <div className="lf-shot" data-director-shot="bridge" style={{ background: bg }}>
      <Title
        x={120}
        y={140}
        lines={["原版工具。", "本地连接。"]}
        size={64}
        width={440}
        progress={seg(t, 50050, 50500)}
        out={seg(t, 54000, 54400)}
      />
      {/* 两枚实体插头沿水平轴靠近、对接（51.0s 前完成，随 pageIn 退去） */}
      {legacyOn && (
        <>
          <div
            className="lf-plug"
            style={{ left: 250 - 110 + (1 - dock) * -170, top: 560 - 70, opacity: legacy }}
          >
            <BrandIcon className="lf-plate-icon" />
            <span>Sleepy Doll</span>
            <i className="lf-plug-pin" />
          </div>
          <div
            className="lf-plug is-host"
            style={{ left: 1320 - 120 + (1 - dock) * 170, top: 560 - 70, opacity: legacy }}
          >
            <span>BetterGI</span>
            <i className="lf-plug-socket" />
          </div>
          {/* 连接器：实体外壳、阴影、插缝（沉下后消隐） */}
          <div
            className="lf-connector"
            style={{
              left: 560,
              top: 560 - 46 + pageK * 300,
              opacity: Math.min(legacy, 1 - pageK),
            }}
          >
            <i className="lf-connector-seam" />
          </div>
          {/* 金线由断到通，亮点沿线走一次 */}
          <div
            className="lf-goldline"
            style={{
              left: 480 + 80 * dock,
              top: 560,
              width: 900 * dock,
              height: 6,
              opacity: dock * legacy,
            }}
          />
          {conduct > 0 && conduct < 1 && (
            <div
              className="lf-spark"
              style={{ left: lerp(560, 1080, ease.camera(conduct)), top: 560 - 5 }}
            />
          )}
        </>
      )}
      {/* 唯一 BridgePage：pageIn 直接落定整页（无推镜），保持到镜尾 */}
      {pageK > 0 && (
        <ProductWindow
          t={t}
          box={{ x: 620, y: 180, w: 1140, h: 796 }}
          viewport={{ width: 1060, height: 740 }}
          opacity={pageK}
          brandFade={0}
          blur={0}
          film={filmIdle}
          presses={{}}
          presentation={{
            page: "bridge",
            bootstrap,
            content: <BridgePage bootstrap={bootstrap} reload={reload} />,
          }}
        />
      )}
      {/* 连接线退回月牙开口（为品牌结尾留 match） */}
      {t >= K.retract[0] && (
        <div
          className="lf-goldline"
          style={{
            left: lerp(1100, 1680, retract),
            top: lerp(560, 300, retract),
            width: 260 * (1 - retract * 0.4),
            height: 5,
            transform: `rotate(${retract * -28}deg)`,
            opacity: 1,
          }}
        />
      )}
    </div>
  );
}

/* ---------------- L 55-60s 收束 ---------------- */

/** 品牌月牙轮廓（与 icon.svg 同源路径）。 */
const CRESCENT =
  "M17.71 5.01A9 9 0 1 0 17.71 16.99A6.5 6.5 0 1 1 17.71 5.01Z";
/** 挤出背层颜色：由深到浅连续过渡，正面完全盖住背层轮廓。 */
const CRESCENT_LAYERS = ["#b7ab8a", "#bfb394", "#c8bd9c", "#d0c6a6", "#d8ceb0", "#ddd4b6"];

function ShotEnd({ t, bg }: { t: number; bg: string }) {
  const inK = ease.camera(seg(t, L.crescentIn[0], L.crescentIn[1]));
  const coil = ease.camera(seg(t, L.coilIn[0], L.coilIn[1]));
  const nameK = ease.camera(seg(t, L.nameIn[0], L.nameIn[1]));
  const settle = ease.panel(seg(t, L.settle[0], L.settle[1]));
  const dim = seg(t, L.dim[0], L.dim[1]) * 0.2;
  return (
    <div className="lf-shot is-end" data-director-shot="end" style={{ background: bg }}>
      {/* 月牙 2.5D：层叠挤出（独立 SVG，不用 brand3d 模块） */}
      <div
        className="lf-crescent"
        style={{
          left: 960 - 130,
          top: 350 - 130 + (1 - inK) * 40 + settle * 8,
          opacity: inK,
          transform: `scale(${lerp(0.94, 1, inK) + settle * 0.015})`,
        }}
      >
        <svg width="260" height="260" viewBox="0 0 22 22" aria-hidden="true">
          <defs>
            <radialGradient id="lf-moon-face" cx="13.2" cy="8.6" r="11.2" gradientUnits="userSpaceOnUse">
              <stop offset="0" stopColor="#f7f1e2" />
              <stop offset="0.6" stopColor="#efe7d2" />
              <stop offset="1" stopColor="#d9cfb2" />
            </radialGradient>
            <linearGradient id="lf-star" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#C6A567" />
              <stop offset="1" stopColor="#e6cf96" />
            </linearGradient>
          </defs>
          <g transform="rotate(-35 11 11)">
            {/* 2.5D 厚度：12 个 ≤1px（viewBox ≤0.085 单位）偏移层叠出连续挤出，
                单一轮廓，正面完全盖住背层，不再露出多个尖角 */}
            {Array.from({ length: 12 }, (_, i) => {
              const k = i / 11;
              const color =
                CRESCENT_LAYERS[
                  Math.min(
                    CRESCENT_LAYERS.length - 1,
                    Math.floor(k * CRESCENT_LAYERS.length),
                  )
                ]!;
              return (
                <path
                  key={i}
                  d={CRESCENT}
                  fill={color}
                  transform={`translate(${(0.085 * (1 - k)).toFixed(4)} ${(0.085 * (1 - k)).toFixed(4)})`}
                />
              );
            })}
            <path d={CRESCENT} fill="url(#lf-moon-face)" />
            <path
              d="M22.602 11.291Q24 11 22.602 10.709L18.13 9.776L17.124 7.228Q16.6 5.9 16.037 7.213L15.07 9.47L12.813 10.437Q11.5 11 12.813 11.563L15.07 12.53L16.037 14.787Q16.6 16.1 17.124 14.772L18.13 12.224Z"
              fill="url(#lf-star)"
            />
          </g>
        </svg>
      </div>
      {/* 金线从底角盘回星体只一次 */}
      {coil > 0 && coil < 1 && (
        <div
          className="lf-goldline"
          style={{
            left: lerp(240, 1105, coil),
            top: lerp(880, 330, coil),
            width: 220,
            height: 5,
            transform: `rotate(${lerp(38, -12, coil)}deg)`,
            opacity: 1 - coil * 0.3,
          }}
        />
      )}
      <h1
        className="lf-endname"
        style={{
          left: 960,
          top: 620 + (1 - nameK) * 26 + settle * 4,
          opacity: nameK,
        }}
      >
        Sleepy Doll
      </h1>
      <p
        className="lf-endtag"
        style={{ left: 960, top: 736, opacity: nameK * seg(t, L.nameIn[1], L.nameIn[1] + 400) }}
      >
        你的本地桌面助手
      </p>
      <div className="lf-dim" style={{ opacity: dim }} />
    </div>
  );
}

/* ---------------- 入口 ---------------- */

/** 视频页里兜住真实组件的展示期网络 stub：只拦本地 IPC，不发真请求。 */
function useFilmIpcStub(active: boolean) {
  useEffect(() => {
    if (!active) return;
    const original = window.fetch;
    const stub = (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input instanceof Request ? input.url : input);
      if (url.includes("/ipc") || url.includes("127.0.0.1:47124")) {
        const body = (input instanceof Request ? undefined : init?.body) ?? "{}";
        let id = "film";
        try {
          id = (JSON.parse(String(body)) as { id?: string }).id ?? id;
        } catch {
          /* 保持缺省 id */
        }
        return Promise.resolve(
          new Response(JSON.stringify({ id, ok: true, result: null }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
        );
      }
      return original(input, init);
    };
    window.fetch = stub as typeof fetch;
    return () => {
      window.fetch = original;
    };
  }, [active]);
}

export function LateFilm(): ReactElement {
  const t = useClock();
  useFilmIpcStub(t >= LATE_START_MS);
  const rootRef = useRef<HTMLDivElement>(null);
  const [anchors, setAnchors] = useState<AnchorState>({ shot: "", ready: true });
  const shotKey =
    t < 17000
      ? "approval"
      : t < 23000
        ? "handoff"
        : t < 28000
          ? "receipt"
          : t < 34000
            ? "save"
            : t < 40000
              ? "reuse"
              : t < 45000
                ? "models"
                : t < 50000
                  ? "extensions"
                  : t < 55000
                    ? "bridge"
                    : "end";
  // 锚点测量：该镜挂载完成 + 字体就绪后读一次真实 rect，换算成 1920×1080 根坐标后缓存。
  // 缺控件时显式置 not-ready 并在控制台报出选择器，不静默回退到猜测常量。
  useEffect(() => {
    const selectors = ANCHOR_SELECTORS[shotKey];
    if (!selectors) {
      setAnchors((prev) => (prev.shot === shotKey ? prev : { shot: shotKey, ready: true }));
      return;
    }
    let cancelled = false;
    const measure = () => {
      if (cancelled) return;
      const root = rootRef.current;
      if (!root) return;
      const rootRect = root.getBoundingClientRect();
      const scale = rootRect.width / 1920 || 1;
      const toStage = (b: DOMRect) => ({
        x: (b.left + b.width / 2 - rootRect.left) / scale,
        y: (b.top + b.height / 2 - rootRect.top) / scale,
      });
      if (selectors.allow && selectors.slot) {
        const btn = root.querySelector(selectors.allow);
        const slot = root.querySelector(selectors.slot);
        if (!btn || !slot) {
          console.error("[LateFilm] 锚点控件缺失:", selectors.allow, selectors.slot);
          setAnchors({ shot: shotKey, ready: false, missing: selectors.allow });
          return;
        }
        const b = btn.getBoundingClientRect();
        const s = slot.getBoundingClientRect();
        // 审批入场是 translateY 的纯函数，测量时按当时 t 补偿回落定位置。
        const progress = seg(readClock(), D.approvalIn[0], D.approvalIn[1]);
        const enterOffset = lerp(10, 0, ease.panel(progress));
        setAnchors({
          shot: shotKey,
          ready: true,
          allow: { x: toStage(b).x, y: toStage(b).y - enterOffset },
          approval: {
            x: (s.left - rootRect.left) / scale,
            y: (s.top - rootRect.top) / scale,
            w: s.width / scale,
            h: s.height / scale,
          },
        });
        return;
      }
      if (selectors.run) {
        const btn = root.querySelector(selectors.run);
        if (!btn) {
          console.error("[LateFilm] 锚点控件缺失:", selectors.run);
          setAnchors({ shot: shotKey, ready: false, missing: selectors.run });
          return;
        }
        // 展示期标注，不改 web 业务源；文案变「停止」后仍是同一按钮节点。
        btn.setAttribute("data-video-anchor", "late-run");
        setAnchors({ shot: shotKey, ready: true, run: toStage(btn.getBoundingClientRect()) });
      }
    };
    if (document.fonts?.ready) void document.fonts.ready.then(measure);
    else measure();
    return () => {
      cancelled = true;
    };
  }, [shotKey]);

  if (t < LATE_START_MS) return <div className="late-film" />;
  const dark = NIGHT;
  const warm = WARM;
  const shotAnchors = anchors.shot === shotKey ? anchors : undefined;
  let shot: ReactElement;
  if (t < 17000)
    shot = (
      <ShotApproval t={t} bg={dark} allow={shotAnchors?.allow} approval={shotAnchors?.approval} />
    );
  else if (t < 23000) shot = <ShotHandoff t={t} bg={dark} />;
  else if (t < 28000) shot = <ShotReceipt t={t} bg={warm} />;
  else if (t < 34000) shot = <ShotSave t={t} bg={warm} />;
  else if (t < 40000) shot = <ShotReuse t={t} bg={warm} run={shotAnchors?.run} />;
  else if (t < 45000) shot = <ShotModels t={t} bg={dark} />;
  else if (t < 50000) shot = <ShotExtensions t={t} bg={warm} />;
  else if (t < 55000) shot = <ShotBridge t={t} bg={dark} />;
  else shot = <ShotEnd t={t} bg={dark} />;
  return (
    <div
      className="late-film"
      ref={rootRef}
      style={{ width: 1920, height: 1080 }}
      data-late-anchor-ready={shotAnchors?.ready === false ? "0" : "1"}
      data-late-anchor-missing={shotAnchors?.missing}
    >
      {shot}
    </div>
  );
}
