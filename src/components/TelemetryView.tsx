import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Activity,
  AlertTriangle,
  BarChart3,
  Clock,
  Database,
  Eye,
  EyeOff,
  RefreshCw,
  Server,
  Trash2
} from 'lucide-react';
import { api } from '../lib/api';
import type {
  RequestTrace,
  TelemetryConfig,
  TelemetrySummary
} from '../types';
import type { Locale } from '../locales';

interface TelemetryViewProps {
  lang: Locale;
}

interface RankedRow {
  id: string;
  name: string;
  meta: string;
  requests: number;
  successRate: number;
  avgLatencyMs: number;
}

const emptySummary: TelemetrySummary = {
  totalRequests: 0,
  totalProjects: 0,
  successRate: 0,
  avgLatencyMs: 0,
  captureBodies: false,
  retentionDays: 30,
  byProject: [],
  byProvider: [],
  byModel: [],
  recentErrors: [],
  insights: []
};

export default function TelemetryView({ lang }: TelemetryViewProps) {
  const [summary, setSummary] = useState<TelemetrySummary>(emptySummary);
  const [config, setConfig] = useState<TelemetryConfig | null>(null);
  const [traces, setTraces] = useState<RequestTrace[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const labels = useMemo(
    () =>
      lang === 'zh'
        ? {
            title: '项目遥测',
            subtitle: '本地请求记录、项目统计与脱敏样本',
            refresh: '刷新',
            clear: '清空',
            capture: '正文预览',
            metadataOnly: '仅元数据',
            requests: '请求总数',
            projects: '项目数',
            success: '成功率',
            latency: '平均耗时',
            projectRank: '项目排行',
            providerRank: '服务商排行',
            modelRank: '模型排行',
            insights: '摘要',
            recent: '最近请求',
            sequence: '顺序',
            project: '项目',
            route: '路由',
            model: '模型',
            provider: '服务商',
            status: '状态',
            bytes: '大小',
            captureLevel: '采集',
            noData: '暂无遥测数据',
            error: '错误',
            confirmClear: '清空所有本地请求遥测记录？',
            retention: '保留',
            days: '天'
          }
        : {
            title: 'Project Telemetry',
            subtitle: 'Local request records, project summaries, and redacted samples',
            refresh: 'Refresh',
            clear: 'Clear',
            capture: 'Body preview',
            metadataOnly: 'Metadata only',
            requests: 'Requests',
            projects: 'Projects',
            success: 'Success',
            latency: 'Avg latency',
            projectRank: 'Projects',
            providerRank: 'Providers',
            modelRank: 'Models',
            insights: 'Summary',
            recent: 'Recent requests',
            sequence: 'Seq',
            project: 'Project',
            route: 'Route',
            model: 'Model',
            provider: 'Provider',
            status: 'Status',
            bytes: 'Size',
            captureLevel: 'Capture',
            noData: 'No telemetry yet',
            error: 'Error',
            confirmClear: 'Clear all local request telemetry?',
            retention: 'Keep',
            days: 'days'
          },
    [lang]
  );

  const loadTelemetry = useCallback(async () => {
    try {
      const [nextSummary, nextConfig, nextTraces] = await Promise.all([
        api.getTelemetrySummary(),
        api.getTelemetryConfig(),
        api.getTelemetryTraces(100)
      ]);
      setSummary(nextSummary);
      setConfig(nextConfig);
      setTraces(nextTraces);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadTelemetry();
    const id = window.setInterval(loadTelemetry, 3000);
    return () => window.clearInterval(id);
  }, [loadTelemetry]);

  const updateConfig = async (updates: Partial<TelemetryConfig>) => {
    setSaving(true);
    try {
      const nextConfig = await api.updateTelemetryConfig(updates);
      setConfig(nextConfig);
      await loadTelemetry();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const clearTelemetry = async () => {
    if (!window.confirm(labels.confirmClear)) return;
    setSaving(true);
    try {
      await api.clearTelemetryTraces();
      await loadTelemetry();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const projectRows = summary.byProject.slice(0, 8).map((item) => ({
    id: item.projectId,
    name: item.projectName,
    meta: item.projectId,
    requests: item.requests,
    successRate: item.successRate,
    avgLatencyMs: item.avgLatencyMs
  }));
  const providerRows = summary.byProvider.slice(0, 8).map((item) => ({
    id: item.providerId,
    name: item.providerName,
    meta: item.providerId,
    requests: item.requests,
    successRate: item.successRate,
    avgLatencyMs: item.avgLatencyMs
  }));
  const modelRows = summary.byModel.slice(0, 8).map((item) => ({
    id: item.model,
    name: item.model,
    meta: 'model',
    requests: item.requests,
    successRate: item.successRate,
    avgLatencyMs: item.avgLatencyMs
  }));

  return (
    <div id="telemetry-tab-panel" className="space-y-5 text-slate-100 font-sans">
      <section className="bg-[#0F172A] border border-[#1E293B] rounded-2xl p-4 md:p-5 shadow-sm">
        <div className="flex flex-col xl:flex-row xl:items-center xl:justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2.5">
              <div className="p-2 bg-indigo-600/10 border border-indigo-500/20 rounded-lg text-indigo-400">
                <BarChart3 className="h-4.5 w-4.5" />
              </div>
              <div>
                <h2 className="text-sm font-bold text-slate-100">{labels.title}</h2>
                <p className="text-[11px] text-slate-500 font-mono mt-0.5">{labels.subtitle}</p>
              </div>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <div className="flex items-center gap-2 bg-[#090D16] border border-[#1E293B] rounded-lg p-1">
              <button
                id="telemetry-capture-toggle"
                type="button"
                disabled={!config || saving}
                onClick={() => updateConfig({ captureBodies: !config?.captureBodies })}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[11px] font-mono font-bold transition-all cursor-pointer disabled:opacity-50 ${
                  config?.captureBodies
                    ? 'bg-amber-500/10 text-amber-300 border border-amber-500/25'
                    : 'bg-slate-800 text-slate-300 border border-slate-700'
                }`}
              >
                {config?.captureBodies ? <Eye className="h-3.5 w-3.5" /> : <EyeOff className="h-3.5 w-3.5" />}
                <span>{config?.captureBodies ? labels.capture : labels.metadataOnly}</span>
              </button>

              <label className="flex items-center gap-1.5 px-2 text-[11px] font-mono text-slate-400">
                <span>{labels.retention}</span>
                <input
                  id="telemetry-retention-days"
                  type="number"
                  min={1}
                  max={3650}
                  value={config?.retentionDays ?? summary.retentionDays}
                  disabled={!config || saving}
                  onChange={(event) => updateConfig({ retentionDays: Number(event.target.value) || 30 })}
                  className="w-16 bg-[#0F172A] border border-[#1E293B] rounded-md px-2 py-1 text-slate-100 text-[11px] focus:outline-none"
                />
                <span>{labels.days}</span>
              </label>
            </div>

            <button
              id="telemetry-refresh"
              type="button"
              onClick={loadTelemetry}
              disabled={saving}
              className="flex items-center gap-1.5 bg-slate-850 hover:bg-slate-800 text-slate-300 border border-[#1E293B] rounded-lg px-3 py-2 text-xs font-semibold font-mono transition-colors cursor-pointer disabled:opacity-50"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
              <span>{labels.refresh}</span>
            </button>

            <button
              id="telemetry-clear"
              type="button"
              onClick={clearTelemetry}
              disabled={saving || traces.length === 0}
              className="p-2 hover:bg-rose-950/40 border border-[#1E293B] text-slate-400 hover:text-rose-400 rounded-lg bg-slate-805 transition-colors cursor-pointer disabled:opacity-40"
              title={labels.clear}
            >
              <Trash2 className="w-4 h-4 text-rose-450" />
            </button>
          </div>
        </div>

        {error && (
          <div className="mt-4 flex items-start gap-2 rounded-lg border border-rose-500/20 bg-rose-500/10 p-3 text-xs text-rose-300">
            <AlertTriangle className="h-4 w-4 shrink-0" />
            <span className="break-all">{labels.error}: {error}</span>
          </div>
        )}
      </section>

      <section className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-3">
        <MetricCard icon={Database} label={labels.requests} value={String(summary.totalRequests)} />
        <MetricCard icon={Activity} label={labels.projects} value={String(summary.totalProjects)} />
        <MetricCard icon={Server} label={labels.success} value={formatRate(summary.successRate)} />
        <MetricCard icon={Clock} label={labels.latency} value={formatMs(summary.avgLatencyMs)} />
      </section>

      <section className="grid grid-cols-1 xl:grid-cols-3 gap-4">
        <SummaryList title={labels.projectRank} rows={projectRows} />
        <SummaryList title={labels.providerRank} rows={providerRows} />
        <SummaryList title={labels.modelRank} rows={modelRows} />
      </section>

      <section className="grid grid-cols-1 xl:grid-cols-3 gap-4">
        <div className="xl:col-span-1 bg-[#0F172A] border border-[#1E293B] rounded-2xl p-4 shadow-sm">
          <h3 className="text-xs font-bold text-slate-100 mb-3 flex items-center gap-2">
            <BarChart3 className="h-4 w-4 text-indigo-400" />
            {labels.insights}
          </h3>
          <div className="space-y-2">
            {summary.insights.map((item, index) => (
              <div key={`${item}-${index}`} className="text-xs leading-relaxed text-slate-300 bg-[#090D16] border border-[#1E293B] rounded-lg p-3">
                {item}
              </div>
            ))}
          </div>
        </div>

        <div className="xl:col-span-2 bg-[#0F172A] border border-[#1E293B] rounded-2xl p-4 shadow-sm min-w-0">
          <div className="flex items-center justify-between gap-3 mb-3">
            <h3 className="text-xs font-bold text-slate-100 flex items-center gap-2">
              <Activity className="h-4 w-4 text-indigo-400" />
              {labels.recent}
            </h3>
            <span className="text-[10px] font-mono text-slate-500">{traces.length}</span>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full min-w-[820px] text-left text-[11px]">
              <thead className="text-slate-500 font-mono uppercase border-b border-[#1E293B]">
                <tr>
                  <th className="py-2 pr-3">{labels.project}</th>
                  <th className="py-2 pr-3">{labels.sequence}</th>
                  <th className="py-2 pr-3">{labels.route}</th>
                  <th className="py-2 pr-3">{labels.model}</th>
                  <th className="py-2 pr-3">{labels.provider}</th>
                  <th className="py-2 pr-3">{labels.status}</th>
                  <th className="py-2 pr-3">{labels.latency}</th>
                  <th className="py-2 pr-3">{labels.bytes}</th>
                  <th className="py-2">{labels.captureLevel}</th>
                </tr>
              </thead>
              <tbody>
                {traces.length === 0 ? (
                  <tr>
                    <td colSpan={9} className="py-8 text-center text-slate-500 font-mono">
                      {labels.noData}
                    </td>
                  </tr>
                ) : (
                  traces.map((trace) => (
                    renderTraceRow(trace)
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      </section>
    </div>
  );
}

function MetricCard({
  icon: Icon,
  label,
  value
}: {
  icon: typeof Database;
  label: string;
  value: string;
}) {
  return (
    <div className="bg-[#0F172A] border border-[#1E293B] rounded-2xl p-4 shadow-sm">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-[10px] font-mono text-slate-500 uppercase">{label}</div>
          <div className="text-2xl font-bold text-slate-100 mt-1">{value}</div>
        </div>
        <div className="p-2 bg-indigo-600/10 border border-indigo-500/20 rounded-lg text-indigo-400">
          <Icon className="h-4.5 w-4.5" />
        </div>
      </div>
    </div>
  );
}

function SummaryList({
  title,
  rows
}: {
  title: string;
  rows: RankedRow[];
}) {
  return (
    <div className="bg-[#0F172A] border border-[#1E293B] rounded-2xl p-4 shadow-sm min-w-0">
      <h3 className="text-xs font-bold text-slate-100 mb-3">{title}</h3>
      <div className="space-y-2">
        {rows.length === 0 ? (
          <div className="h-24 flex items-center justify-center text-[11px] text-slate-500 font-mono">EMPTY</div>
        ) : (
          rows.map((item) => (
            <div key={`${title}-${item.id}`} className="bg-[#090D16] border border-[#1E293B] rounded-lg p-3">
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-xs font-semibold text-slate-200 truncate">{item.name}</div>
                  <div className="text-[10px] font-mono text-slate-500 truncate mt-0.5">{item.meta}</div>
                </div>
                <div className="text-right shrink-0">
                  <div className="text-xs font-bold text-slate-100">{item.requests}</div>
                  <div className="text-[10px] font-mono text-emerald-400">{formatRate(item.successRate)}</div>
                </div>
              </div>
              <div className="mt-2 h-1.5 bg-slate-800 rounded-full overflow-hidden">
                <div className="h-full bg-indigo-500" style={{ width: `${Math.max(4, item.successRate * 100)}%` }} />
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function renderTraceRow(trace: RequestTrace) {
  return (
    <tr key={trace.id} className="border-b border-[#1E293B]/70 text-slate-300 hover:bg-slate-800/30">
      <td className="py-2.5 pr-3 max-w-[150px]">
        <div className="font-semibold truncate">{trace.projectName}</div>
        <div className="text-[10px] text-slate-500 font-mono truncate">{shortTime(trace.timestamp)}</div>
      </td>
      <td className="py-2.5 pr-3 font-mono text-slate-400">#{trace.projectSequence}</td>
      <td className="py-2.5 pr-3 font-mono text-slate-400 max-w-[170px] truncate">{trace.routePath}</td>
      <td className="py-2.5 pr-3 font-mono text-slate-300 max-w-[140px] truncate">{trace.requestModel || '-'}</td>
      <td className="py-2.5 pr-3 max-w-[140px] truncate">{trace.providerName}</td>
      <td className="py-2.5 pr-3">
        <span className={`px-2 py-1 rounded-md border font-mono text-[10px] font-bold ${trace.success ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20' : 'bg-rose-500/10 text-rose-400 border-rose-500/20'}`}>
          {trace.statusCode ?? 'ERR'}
        </span>
      </td>
      <td className="py-2.5 pr-3 font-mono">{formatMs(trace.latencyMs)}</td>
      <td className="py-2.5 pr-3 font-mono">{formatBytes(trace.requestBytes + (trace.responseBytes ?? 0))}</td>
      <td className="py-2.5 font-mono">
        <span className={trace.captureLevel === 'body' ? 'text-amber-300' : 'text-slate-500'}>
          {trace.captureLevel}
        </span>
      </td>
    </tr>
  );
}

function formatRate(value: number) {
  return `${Math.round(value * 1000) / 10}%`;
}

function formatMs(value: number) {
  return `${Math.round(value)}ms`;
}

function formatBytes(value: number) {
  if (value < 1024) return `${value}B`;
  if (value < 1024 * 1024) return `${Math.round(value / 10.24) / 100}KB`;
  return `${Math.round(value / 10485.76) / 100}MB`;
}

function shortTime(value: string) {
  return value.replace('T', ' ').slice(5, 19);
}
