import { subscribeNativeEvents } from "../ipc/api";

/**
 * 桌面客户端统计：把原生层推送的白名单事件经官方 gtag.js 直连 Google Analytics
 * （独立「桌面客户端」数据流），不经过官网。事件内容与开关完全由原生层决定，
 * 这里只做加载控制、consent、缓冲与丢弃；不读取页面内容、不新增定时轮询。
 * dev/preview（无桌面壳标记）与禁用状态完全不加载脚本。
 *
 * 关键语义：
 * - 开启控制只解除 `ga-disable` 并置位开关，不加载脚本；脚本加载以「收到完整
 *   有效的白名单事件（携带合法 clientId）」为信号，且本进程内只尝试一次。
 * - 显式 disable 后缓冲清空、置 `ga-disable`，积压旧事件不能重新启用；
 *   只能由显式 enable 控制恢复（已加载的 tag 可复用）。
 * - 脚本 onerror / 加载超时是本进程终态：标记失败、清缓冲，不假 loaded、
 *   不 flush、不为后续事件再次注入脚本；晚于超时的 onload 一律忽略。
 */

const MEASUREMENT_ID = "G-J691D7H7BY";
const SCRIPT_URL = `https://www.googletagmanager.com/gtag/js?id=${MEASUREMENT_ID}`;
/** 固定虚拟 page_location：仅作为 GA 里的数据标签，界面从不访问该网址。 */
const PAGE_LOCATION = "https://sleepy-doll.localhost/app";
const PAGE_TITLE = "Sleepy Doll Desktop";
const MAX_BUFFERED = 32;
const MAX_AGE_MS = 30_000;
const LOAD_TIMEOUT_MS = 15_000;
const EVENT_NAMES = new Set([
  "app_start",
  "update_check",
  "update_available",
  "update_download_complete",
  "update_success",
  "update_failed",
]);
const CLIENT_ID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const VERSION_PATTERN = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;

interface NativePayload {
  clientId?: unknown;
  event?: unknown;
  version?: unknown;
  targetVersion?: unknown;
  channel?: unknown;
  platform?: unknown;
  sessionId?: unknown;
  analyticsEnabled?: unknown;
}

interface BufferedEvent {
  name: string;
  params: Record<string, unknown>;
  at: number;
}

declare global {
  interface Window {
    dataLayer?: unknown[];
    gtag?: (...args: unknown[]) => void;
  }
}

let initialized = false;
let enabled = false;
let explicitlyDisabled = false;
let loading = false;
let loaded = false;
let failed = false;
let configured = false;
let clientId: string | undefined;
let channel: string | undefined;
let loadTimer = 0;
const buffer: BufferedEvent[] = [];

function setDisableFlag(disabled: boolean) {
  (window as unknown as Record<string, unknown>)[
    `ga-disable-${MEASUREMENT_ID}`
  ] = disabled;
}

function send(name: string, params: Record<string, unknown>) {
  window.gtag?.("event", name, params);
}

/** 脚本可用后按序补发缓冲；超过 30 秒的直接丢弃，不重试。 */
function flush() {
  if (!loaded || !enabled) {
    return;
  }
  const now = Date.now();
  while (buffer.length > 0 && now - buffer[0]!.at > MAX_AGE_MS) {
    buffer.shift();
  }
  while (buffer.length > 0) {
    const event = buffer.shift()!;
    send(event.name, event.params);
  }
}

/** 配置只做一次，client_id 用原生层已验证的稳定值，绝不随机生成 fallback。 */
function configure() {
  if (configured || !loaded || !enabled || !clientId) {
    return;
  }
  configured = true;
  window.gtag?.("config", MEASUREMENT_ID, {
    client_id: clientId,
    cookie_domain: "none",
    send_page_view: false,
    allow_google_signals: false,
    allow_ad_personalization_signals: false,
    page_location: PAGE_LOCATION,
    page_title: PAGE_TITLE,
    page_referrer: "",
    ...(channel === "test" ? { debug_mode: true } : {}),
  });
}

/**
 * 脚本加载结束（成功 / onerror / 超时）。超时或失败后 `loading` 已复位，
 * 晚到的 onload 会被挡在门外；失败是本进程终态，不重试。
 */
function finishLoad(succeeded: boolean) {
  if (!loading) {
    return;
  }
  if (loadTimer) {
    window.clearTimeout(loadTimer);
    loadTimer = 0;
  }
  loading = false;
  if (!succeeded) {
    failed = true;
    buffer.length = 0;
    return;
  }
  loaded = true;
  configure();
  flush();
}

function loadScript() {
  loading = true;
  window.dataLayer = window.dataLayer || [];
  window.gtag =
    window.gtag ??
    function gtag(..._args: unknown[]) {
      // eslint-disable-next-line prefer-rest-params
      window.dataLayer?.push(arguments);
    };
  window.gtag("js", new Date());
  // 广告相关三项永远拒绝；analytics_storage 只有用户主动开启统计才授予。
  // 官方 gtag 命令先进 dataLayer 队列，脚本加载后按序执行。
  window.gtag("consent", "default", {
    ad_storage: "denied",
    ad_user_data: "denied",
    ad_personalization: "denied",
    analytics_storage: "denied",
  });
  window.gtag("consent", "update", { analytics_storage: "granted" });
  loadTimer = window.setTimeout(() => {
    loadTimer = 0;
    finishLoad(false);
  }, LOAD_TIMEOUT_MS);
  const script = document.createElement("script");
  script.async = true;
  script.src = SCRIPT_URL;
  script.dataset.sleepyAnalytics = MEASUREMENT_ID;
  script.onload = () => finishLoad(true);
  script.onerror = () => finishLoad(false);
  document.head.appendChild(script);
}

/** 显式开启：只解除禁用并标记开关；加载由完整有效事件驱动，这里不注入脚本。 */
function enableAnalytics() {
  enabled = true;
  explicitlyDisabled = false;
  setDisableFlag(false);
  if (loaded) {
    configure();
    flush();
  }
}

/** 显式关闭：清缓冲、置 ga-disable；旧事件不得再启用，禁用控制本身绝不上报。 */
function disableAnalytics() {
  enabled = false;
  explicitlyDisabled = true;
  setDisableFlag(true);
  buffer.length = 0;
}

/** 事件负载必须完整且全部字段合法，才允许作为加载与上报依据。 */
function validEventPayload(data: NativePayload): boolean {
  return (
    typeof data.clientId === "string" &&
    CLIENT_ID_PATTERN.test(data.clientId) &&
    typeof data.event === "string" &&
    EVENT_NAMES.has(data.event) &&
    typeof data.version === "string" &&
    VERSION_PATTERN.test(data.version) &&
    // 原生序列化 Option None 为 null，null/undefined 都是合法缺省。
    (data.targetVersion == null ||
      (typeof data.targetVersion === "string" &&
        VERSION_PATTERN.test(data.targetVersion))) &&
    (data.channel === "test" || data.channel === "stable") &&
    data.platform === "windows" &&
    typeof data.sessionId === "number" &&
    Number.isSafeInteger(data.sessionId) &&
    data.sessionId > 0
  );
}

function handlePayload(payload: unknown) {
  if (!payload || typeof payload !== "object") {
    return;
  }
  const data = payload as NativePayload;
  if (typeof data.analyticsEnabled === "boolean") {
    if (data.analyticsEnabled) {
      enableAnalytics();
    } else {
      disableAnalytics();
    }
    return;
  }
  if (!validEventPayload(data)) {
    return;
  }
  // 原生只在开关开启时才推送事件：完整有效的负载本身就是加载信号；
  // 但显式 disable 之后积压的旧事件不能重新启用，只能等显式 enable。
  if (!enabled) {
    if (explicitlyDisabled) {
      return;
    }
    enabled = true;
    setDisableFlag(false);
  }
  clientId ??= data.clientId as string;
  channel ??= data.channel as string;
  const params: Record<string, unknown> = {};
  params.app_version = data.version as string;
  params.release_channel = data.channel as string;
  params.platform = data.platform as string;
  if (typeof data.targetVersion === "string") {
    params.target_version = data.targetVersion;
  }
  params.session_id = data.sessionId as number;
  // 每条事件显式写入 debug_mode：跟随自身通道，避免首次 config 为 test 时
  // 稳定通道事件沿用 debug_mode。true/false 都写，保证换通道后无残留。
  params.debug_mode = data.channel === "test";
  if (loaded) {
    send(data.event as string, params);
    return;
  }
  if (!loading && !failed) {
    loadScript();
  }
  // 出口未就绪：有界缓冲；超龄事件在入队时顺手清掉，不做周期任务。
  const now = Date.now();
  while (buffer.length > 0 && now - buffer[0]!.at > MAX_AGE_MS) {
    buffer.shift();
  }
  if (buffer.length >= MAX_BUFFERED) {
    buffer.shift();
  }
  buffer.push({ name: data.event as string, params, at: now });
}

/** 在 React 渲染前调用一次（须先于 DesktopReady）；重复调用与非桌面环境都是空操作。 */
export function initAnalytics() {
  if (initialized || !window.__SLEEPY_DOLL_DESKTOP__) {
    return;
  }
  initialized = true;
  subscribeNativeEvents((name, payload) => {
    if (name === "analytics") {
      handlePayload(payload);
    }
  });
}
