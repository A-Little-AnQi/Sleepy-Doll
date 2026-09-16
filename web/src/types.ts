import type { GroupLayout } from "./conversation-groups";

export interface ModelInfo {
  id: string;
  name: string;
  protocol: string;
  model: string;
  baseUrl: string;
  active: boolean;
  timeoutMs?: number;
  contextWindow?: number;
  maxOutputTokens?: number;
  auth?: "auto" | "apiKey" | "bearer";
  promptCache?: boolean;
}

export interface SkillInfo {
  name: string;
  description: string;
  source: string;
  tags: string[];
  enabled?: boolean;
  alwaysLoad?: boolean;
  /** 需要哪些领域提供方在线，例如 `bgi`。 */
  requiresProviders?: string[];
  /**
   * 用户开关是开的、依赖也在线，这份说明才会真的进入模型上下文。
   * `enabled` 只表示用户没有关掉它。
   */
  available?: boolean;
  unavailableReason?: string;
  instructions?: string;
}
export interface PluginInfo {
  manifest: { id: string; name: string; version: string; description?: string };
  status: string;
  configuredEnabled?: boolean;
  error?: string;
}
export interface ConversationInfo {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  pinned?: boolean;
  archived?: boolean;
  /** 会话绑定的模型。已被删除时由运行时回落到默认模型。 */
  modelId?: string | null;
  taskCount?: number;
}
export interface ToolInfo {
  name: string;
  description: string;
  source: string;
}
export interface MessageInfo {
  role: "system" | "user" | "assistant" | "tool";
  content: string;
  toolCallId?: string;
  toolCalls?: Array<{ id: string; name: string; arguments: unknown }>;
  /**
   * 模型的推理内容。只有推理类模型会返回，且 `text` 可能为空 ——
   * `blocks` 是回传给提供方的原样载荷，界面不解析它。
   */
  reasoning?: {
    protocol: string;
    blocks?: unknown[];
    text?: string;
  };
}
export interface TaskInfo {
  id: string;
  conversationId: string;
  prompt: string;
  state:
    | "answered"
    | "queued"
    | "preflighting"
    | "blocked"
    | "running"
    | "deciding"
    | "executing"
    | "awaitingUser"
    | "awaitingApproval"
    | "waitingJob"
    | "verifying"
    | "recovering"
    | "succeeded"
    | "partial"
    | "needsReview"
    | "failed"
    | "cancelling"
    | "cancelled";
  result?: string;
  error?: string;
  createdAt: string;
  updatedAt: string;
  modelId?: string | null;
  inputTokens?: number;
  outputTokens?: number;
  usageEstimated?: boolean;
  contextTokens?: number;
  contextWindow?: number;
  contextCompacted?: boolean;
  cacheReadTokens?: number;
  source?:
    | { kind: "agent" }
    | { kind: "savedStrategy"; strategyId: string }
    | { kind: "savedWorkflow"; workflowId: string; workflowRevision: number };
}

/** 快捷任务的可见状态。文案与主按钮由后端给出，界面不另立一套。 */
export type DefinitionState =
  | "draft"
  | "invalid"
  | "readyUnverified"
  | "readyVerified"
  | "unavailable"
  | "archived"
  | "deleted";

export interface TaskSummary {
  id: string;
  name: string;
  description: string;
  state: DefinitionState;
  stateLabel: string;
  actionLabel: string;
  runnable: boolean;
  pinned: boolean;
  sourceConversationId?: string | null;
  sourceTitleSnapshot: string;
  sourceDeleted: boolean;
  publishedRevision?: number | null;
  revision?: number | null;
  modelUsage: "none" | "possible" | "unknown";
  /** 运行不调用模型。只有全部依赖都声明为确定性工具时才为真。 */
  zeroToken: boolean;
  nodeCount: number;
  updatedAt: string;
  lastRunId?: string | null;
  issue?: string | null;
}

export interface TaskIssue {
  nodeId: string;
  message: string;
}

export interface TaskValidation {
  modelUsage: "none" | "possible" | "unknown";
  nodeCount: number;
  maxExpansion: number;
  issues: TaskIssue[];
  missingBindings: string[];
}

export interface WorkflowDetail {
  summary: TaskSummary;
  definition: {
    id: string;
    name: string;
    description: string;
    sourceConversationId?: string | null;
    sourceTitleSnapshot: string;
    publishedRevision?: number | null;
    draftRevision?: number | null;
    pinned: boolean;
    lastRunId?: string | null;
    createdAt: string;
    updatedAt: string;
  };
  revision?: {
    revision: number;
    modelUsage: string;
    validation: TaskValidation;
  } | null;
  runs: string[];
}
export interface StrategyStep {
  id: string;
  title: string;
  capabilityId: string;
  arguments: Record<string, unknown>;
  dependsOn: string[];
}
export interface SavedStrategy {
  id: string;
  name: string;
  sourceRunId: string;
  plan: { revision: number; goal: string; steps: StrategyStep[] };
  createdAt: string;
  updatedAt: string;
  lastRunId?: string;
}
export interface ResourceInfo {
  id: string;
  providerId: string;
  kind: string;
  displayName: string;
  version: string;
  capabilities: string[];
  presentation?: unknown;
}
export interface OperationInfo {
  id: string;
  title: string;
  providerId: string;
  state: string;
  risk: string;
  createdAt: string;
  updatedAt: string;
  error?: string;
}
export interface WorkflowInfo {
  id: string;
  revision: number;
  name: string;
  description: string;
  steps: Array<{ id: string; title: string; tool: string }>;
  unattended: "allowed" | "forbidden";
  verifiedAt: string;
  verifiedFromRun: string;
}

/** 一次运行的步骤结果，供「运行记录」详情展开。 */
export interface StepRecord {
  id: string;
  title: string;
  outcome: string;
  detail?: string;
}
export interface DiagnosticInfo {
  id: string;
  providerId: string;
  kind:
    | "userConfiguration"
    | "thirdPartyScript"
    | "hostApplication"
    | "environment"
    | "unknown";
  title: string;
  summary: string;
  evidenceIds: string[];
  repairAction?: unknown;
  createdAt: string;
}
export interface NotificationInfo {
  id: string;
  kind: string;
  title: string;
  message: string;
  createdAt: string;
}
export interface RunEvent {
  sequence: number;
  conversationId: string;
  runId: string;
  kind: string;
  data: Record<string, unknown>;
}
export interface RunApproval {
  id: string;
  runId: string;
  request: {
    methodId?: string;
    instanceId?: string;
    arguments?: unknown;
    binding?: { description?: string };
  };
  expiresAt: number;
}
/** 审批级别。文案由运行时给出，界面不另写一套。 */
export interface PermissionLevel {
  value: string;
  label: string;
  description: string;
}
export interface PermissionState {
  mode: string;
  label: string;
  description: string;
  levels: PermissionLevel[];
}

/**
 * 旧 mock 的 bootstrap 没有 permission 字段。正式运行时会带上完整清单；
 * 这里只垫一层，避免预览整页崩掉。文案与 `PermissionMode::levels` 对齐。
 */
export const PREVIEW_PERMISSION: PermissionState = {
  mode: "standard",
  label: "替我审批",
  description: "普通修改直接执行；删除文件和大幅改配置才问你一次",
  levels: [
    {
      value: "planOnly",
      label: "只读",
      description: "只查询和阅读，任何修改都不执行",
    },
    {
      value: "askEach",
      label: "请求审批",
      description: "每次修改前都问你一次",
    },
    {
      value: "standard",
      label: "替我审批",
      description: "普通修改直接执行；删除文件和大幅改配置才问你一次",
    },
    {
      value: "fullAccess",
      label: "完全控制",
      description: "一律直接执行，不再询问。删除和大范围覆盖也直接做",
    },
  ],
};

export function withPermission(bootstrap: Bootstrap): Bootstrap {
  if (bootstrap.permission?.mode && bootstrap.permission.levels?.length) {
    return bootstrap;
  }
  return { ...bootstrap, permission: PREVIEW_PERMISSION };
}

export interface Bootstrap {
  preview?: boolean;
  /** Absolute path of the configuration file, so the interface can point at it
   * instead of telling the user to "edit the configuration". */
  configPath: string;
  models: ModelInfo[];
  skills: SkillInfo[];
  plugins: PluginInfo[];
  tools: ToolInfo[];
  conversations: ConversationInfo[];
  conversationGroups?: GroupLayout;
  tasks: TaskInfo[];
  strategies: SavedStrategy[];
  workflows: TaskSummary[];
  operations: OperationInfo[];
  resources: ResourceInfo[];
  diagnostics: DiagnosticInfo[];
  notifications: NotificationInfo[];
  permission: PermissionState;
  bridge: {
    simulated?: boolean;
    enabled: boolean;
    connected: boolean;
    baseUrl: string;
    error?: string;
  };
}

export interface BridgeMethod {
  methodId: string;
  displayName: string;
  group: string;
  summary: string;
  effect: string;
  callable: boolean;
  unavailableReason?: string;
  catalogVersion?: string;
  executionMode?: string;
  requiresConfirmation?: boolean;
  whenToUse?: string[];
  sideEffects?: string[];
  parameters?: Array<{
    name: string;
    type: string;
    required: boolean;
    description: string;
  }>;
}
export interface BridgeGuide {
  title: string;
  purpose: string;
  whenToUse: string[];
  preconditions: string[];
  sideEffects: string[];
  resultMeaning: string;
  verification: string;
  rollback: string;
  examples: unknown[];
  documentationSource: string;
  sourceReference?: string;
}
export interface BridgeMethodDetail extends BridgeMethod {
  guide?: BridgeGuide;
  inputSchema: {
    type?: string;
    properties?: Record<
      string,
      { type?: string; description?: string; enum?: unknown[] }
    >;
    required?: string[];
  };
  outputSchema?: unknown;
  errors?: string[];
}
export interface BridgeCatalog {
  total: number;
  items?: BridgeMethod[];
  methods?: BridgeMethod[];
  offset?: number;
  nextOffset?: number | null;
  groups?: Array<{ id: string; count: number }>;
  catalogVersion?: string;
}

export interface RecoveryRecord {
  changeId: string;
  recordVersion?: string;
  currentVersion?: string;
  state?: string;
  createdAt?: string;
  operation?: string;
  configPath?: string;
  paths: string[];
  canRestore: boolean;
  reason?: string;
}
