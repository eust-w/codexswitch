import type {
  Agent,
  DistillationJob,
  GatewayKey,
  GatewayProject,
  GatewayState,
  GatewayUsageSummary,
  GatewayUser,
  LogEntry,
  Model,
  Provider,
  ProviderHealth,
  ProviderModelSyncResult,
  RequestTrace,
  TelemetryConfig,
  TelemetrySummary,
  UpstreamModel
} from '../types';

interface CodexswitchWindow extends Window {
  __TAURI_INTERNALS__?: unknown;
}

export interface CoreState {
  providers: Provider[];
  providerHealth?: ProviderHealth[];
  models: Model[];
  agents: Agent[];
  mcpServers: unknown[];
  logs: LogEntry[];
  gateway: GatewayState;
}

const env = (import.meta as unknown as { env?: Record<string, string | undefined> }).env || {};

export const API_BASE =
  env.VITE_CODEXSWITCH_API_BASE ||
  (((window as CodexswitchWindow).__TAURI_INTERNALS__ ? 'http://127.0.0.1:3900' : ''));

export function apiUrl(path: string): string {
  return `${API_BASE}${path}`;
}

export async function apiRequest(path: string, init?: RequestInit): Promise<Response> {
  return fetch(apiUrl(path), init);
}

async function jsonRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await apiRequest(path, init);
  if (!res.ok) {
    const body = await res.text();
    throw new Error(body || `Request failed: ${res.status}`);
  }
  return res.json() as Promise<T>;
}

export const api = {
  getState: () => jsonRequest<CoreState>('/api/state'),
  fetchProviderModels: (providerId: string) =>
    jsonRequest<UpstreamModel[]>(`/api/providers/${encodeURIComponent(providerId)}/models`),
  syncProviderModels: (providerId: string) =>
    jsonRequest<ProviderModelSyncResult>(
      `/api/providers/${encodeURIComponent(providerId)}/models/sync`,
      { method: 'POST' }
    ),
  getTelemetryConfig: () => jsonRequest<TelemetryConfig>('/api/telemetry/config'),
  updateTelemetryConfig: (body: Partial<TelemetryConfig>) =>
    jsonRequest<TelemetryConfig>('/api/telemetry/config', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  getTelemetrySummary: () => jsonRequest<TelemetrySummary>('/api/telemetry/summary'),
  getTelemetryTraces: (limit = 100, projectId?: string, order?: 'asc' | 'desc') => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (projectId) params.set('projectId', projectId);
    if (order) params.set('order', order);
    return jsonRequest<RequestTrace[]>(`/api/telemetry/traces?${params.toString()}`);
  },
  clearTelemetryTraces: () =>
    jsonRequest<{ success: boolean }>('/api/telemetry/traces', { method: 'DELETE' }),
  getGatewayUsers: () => jsonRequest<GatewayUser[]>('/api/admin/users'),
  createGatewayUser: (body: { displayName: string; email?: string }) =>
    jsonRequest<{ success: boolean; user: GatewayUser }>('/api/admin/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  updateGatewayUser: (id: string, body: Partial<GatewayUser>) =>
    jsonRequest<{ success: boolean; user: GatewayUser }>(`/api/admin/users/${encodeURIComponent(id)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  getGatewayProjects: () => jsonRequest<GatewayProject[]>('/api/admin/projects'),
  createGatewayProject: (body: { userId: string; name: string; slug?: string }) =>
    jsonRequest<{ success: boolean; project: GatewayProject }>('/api/admin/projects', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  updateGatewayProject: (id: string, body: Partial<GatewayProject>) =>
    jsonRequest<{ success: boolean; project: GatewayProject }>(`/api/admin/projects/${encodeURIComponent(id)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  getGatewayKeys: () => jsonRequest<GatewayKey[]>('/api/admin/keys'),
  createGatewayKey: (body: {
    userId: string;
    projectId: string;
    name: string;
    quotaRequests?: number;
    quotaTokens?: number;
    expiresAt?: string;
  }) =>
    jsonRequest<{ success: boolean; key: GatewayKey; rawKey: string }>('/api/admin/keys', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  updateGatewayKey: (id: string, body: Partial<GatewayKey>) =>
    jsonRequest<{ success: boolean; key: GatewayKey }>(`/api/admin/keys/${encodeURIComponent(id)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  revokeGatewayKey: (id: string) =>
    jsonRequest<{ success: boolean; key: GatewayKey }>(`/api/admin/keys/${encodeURIComponent(id)}/revoke`, {
      method: 'POST',
    }),
  getGatewayUsageSummary: () => jsonRequest<GatewayUsageSummary>('/api/admin/usage/summary'),
  getDistillationJobs: () => jsonRequest<DistillationJob[]>('/api/admin/distillation/jobs'),
  createDistillationJob: (body: {
    name: string;
    projectId?: string;
    userId?: string;
    limit?: number;
    includeFailures?: boolean;
  }) =>
    jsonRequest<{ success: boolean; job: DistillationJob }>('/api/admin/distillation/jobs', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  getDistillationJob: (id: string) =>
    jsonRequest<DistillationJob>(`/api/admin/distillation/jobs/${encodeURIComponent(id)}`),
  post: (path: string, body?: unknown) =>
    jsonRequest(path, {
      method: 'POST',
      headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
  put: (path: string, body: unknown) =>
    jsonRequest(path, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  delete: (path: string) =>
    jsonRequest(path, {
      method: 'DELETE',
    }),
};
