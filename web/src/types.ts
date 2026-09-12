export interface ModelInfo {
  id: string;
  name: string;
  protocol: string;
  model: string;
  baseUrl: string;
  active: boolean;
  timeoutMs?: number;
}

export interface SkillInfo {
  name: string;
  description: string;
  source: string;
  tags: string[];
  enabled?: boolean;
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
}
export interface TaskInfo {
  id: string;
  conversationId: string;
  prompt: string;
  state:
    | "answered"
    | "queued"
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
  source?:
    | { kind: "agent" }
    | { kind: "savedStrategy"; strategyId: string }
    | { kind: "savedWorkflow"; workflowId: string; workflowRevision: number };
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
  tasks: TaskInfo[];
  strategies: SavedStrategy[];
  workflows: WorkflowInfo[];
  operations: OperationInfo[];
  resources: ResourceInfo[];
  diagnostics: DiagnosticInfo[];
  notifications: NotificationInfo[];
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
