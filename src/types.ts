export type AgentStatus = 'idle' | 'running' | 'error';
export type ProviderType = 'openai' | 'anthropic' | 'gemini' | 'openrouter' | 'custom';
export type AppType = 'claude' | 'codex' | 'gemini';
export type ServiceType = 'claude' | 'openai' | 'responses' | 'gemini';
export type ProviderStatus = 'connected' | 'disconnected' | 'testing';
export type McpStatus = 'active' | 'inactive' | 'error';
export type McpHealth = 'healthy' | 'unhealthy' | 'checking';
export type LogLevel = 'info' | 'warn' | 'error' | 'debug';

export interface Agent {
  id: string;
  name: string;
  desc: string;
  providerId: string;
  modelId: string;
  status: AgentStatus;
  lastRun?: string;
  metrics: {
    tokensSpent: number;
    avgLatency: string;
    callsCount: number;
  };
  // Extended configuration options
  allowedProviders?: string[]; // Multiple selected model providers
  contextLengthLimit?: number; // Context length limit (in K tokens)
  maxOutputLimit?: number; // Max output limit (tokens)
  temperature?: number; // Temperature (0.0 to 2.0)
  autoFormatConvert?: boolean; // Automatically convert to the format required by the AI coding agent
}

export interface Provider {
  id: string;
  appType?: AppType;
  name: string;
  type: ProviderType;
  serviceType?: ServiceType;
  apiKey: string;
  apiKeys?: string[];
  baseUrl: string;
  baseUrls?: string[];
  status: ProviderStatus;
  latency?: string;
  
  // Advanced local gateway configuration options
  description?: string;
  skipTls?: boolean;
  lowQuality?: boolean;
  autoBlacklist?: boolean;
  codexPassthrough?: boolean;
  codexToolsCompat?: boolean;
  normalizeUserId?: boolean;
  normalizeChatRoles?: boolean;
  reasoningStyle?: string;
  customHeaders?: { name: string; value: string }[];
  proxyUrl?: string;
  routePrefix?: string;
  modelMapping?: Record<string, string>;
  priority?: number;
  isCurrent?: boolean;
  failoverEnabled?: boolean;
  health?: ProviderHealth;
}

export interface ProviderHealth {
  providerId: string;
  isHealthy: boolean;
  consecutiveFailures: number;
  lastSuccessAt?: string;
  lastFailureAt?: string;
  lastError?: string;
  circuitOpenUntil?: string;
}

export interface Model {
  id: string;
  name: string;
  providerId: string;
  tags: string[];
  isDefault: boolean;
  contextWindow: string;
  latency: string;
  maxOutput: string;
  supportsImages?: boolean;
}

export interface UpstreamModel {
  id: string;
  name: string;
  ownedBy?: string;
  contextWindow?: string;
  maxOutput?: string;
  supportsImages?: boolean;
}

export interface ProviderModelSyncResult {
  success: boolean;
  upstreamCount: number;
  imported: Model[];
}

export interface McpServer {
  id: string;
  name: string;
  type: 'stdio' | 'sse';
  command?: string;
  args?: string;
  url?: string;
  status: McpStatus;
  health: McpHealth;
  toolsCount: number;
  resourcesCount: number;
  errorMsg?: string;
}

export interface LogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  source: string;
  message: string;
}

export interface GatewayState {
  status: 'running' | 'stopped';
  uptimeSeconds: number;
  cpuUsage: number;
  memoryUsage: number;
  port: number;
}

export interface TelemetryConfig {
  enabled: boolean;
  captureBodies: boolean;
  retentionDays: number;
  maxBodyPreviewBytes: number;
}

export interface RequestTrace {
  id: string;
  timestamp: string;
  projectSequence: number;
  projectId: string;
  projectName: string;
  workspacePath?: string;
  gitRemote?: string;
  sessionId?: string;
  protocol: string;
  routePath: string;
  providerId: string;
  providerName: string;
  serviceType: string;
  requestModel?: string;
  upstreamUrl?: string;
  statusCode?: number;
  success: boolean;
  latencyMs: number;
  requestBytes: number;
  responseBytes?: number;
  error?: string;
  gatewayUserId?: string;
  gatewayUserName?: string;
  gatewayProjectId?: string;
  gatewayProjectName?: string;
  gatewayKeyId?: string;
  gatewayKeyName?: string;
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
  estimatedCostUsd?: number;
  captureLevel: 'metadata' | 'body';
  requestBodyPreview?: string;
  responseBodyPreview?: string;
}

export interface GatewayUser {
  id: string;
  displayName: string;
  email?: string;
  status: string;
  createdAt: string;
}

export interface GatewayProject {
  id: string;
  userId: string;
  userName: string;
  name: string;
  slug: string;
  status: string;
  createdAt: string;
}

export interface GatewayKey {
  id: string;
  keyPrefix: string;
  name: string;
  userId: string;
  userName: string;
  projectId: string;
  projectName: string;
  status: string;
  quotaRequests: number;
  quotaTokens: number;
  usedRequests: number;
  usedTokens: number;
  expiresAt?: string;
  lastUsedAt?: string;
  createdAt: string;
}

export interface GatewayUsageBucket {
  id: string;
  name: string;
  userId?: string;
  userName?: string;
  projectId?: string;
  projectName?: string;
  requests: number;
  tokens: number;
  estimatedCostUsd: number;
}

export interface GatewayKeyUsageBucket {
  keyId: string;
  keyName: string;
  userId: string;
  userName: string;
  projectId: string;
  projectName: string;
  usedRequests: number;
  usedTokens: number;
  quotaRequests: number;
  quotaTokens: number;
  estimatedCostUsd: number;
}

export interface GatewayUsageSummary {
  totalRequests: number;
  totalTokens: number;
  estimatedCostUsd: number;
  byUser: GatewayUsageBucket[];
  byProject: GatewayUsageBucket[];
  byKey: GatewayKeyUsageBucket[];
}

export interface DistillationJob {
  id: string;
  name: string;
  status: string;
  projectId?: string;
  userId?: string;
  traceCount: number;
  exportPath?: string;
  error?: string;
  filters: Record<string, unknown>;
  createdAt: string;
  completedAt?: string;
}

export interface TelemetryBucketSummary {
  requests: number;
  successRate: number;
  avgLatencyMs: number;
}

export interface TelemetryProjectSummary extends TelemetryBucketSummary {
  projectId: string;
  projectName: string;
}

export interface TelemetryProviderSummary extends TelemetryBucketSummary {
  providerId: string;
  providerName: string;
}

export interface TelemetryModelSummary extends TelemetryBucketSummary {
  model: string;
}

export interface TelemetrySummary {
  totalRequests: number;
  totalProjects: number;
  successRate: number;
  avgLatencyMs: number;
  captureBodies: boolean;
  retentionDays: number;
  byProject: TelemetryProjectSummary[];
  byProvider: TelemetryProviderSummary[];
  byModel: TelemetryModelSummary[];
  recentErrors: RequestTrace[];
  insights: string[];
}
