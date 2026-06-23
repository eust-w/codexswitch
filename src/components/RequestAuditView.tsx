import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  CheckCircle2,
  Clipboard,
  FileSearch,
  RefreshCw,
  Search,
  XCircle
} from 'lucide-react';
import { api } from '../lib/api';
import type { Locale } from '../locales';
import type { RequestTrace, TelemetrySummary } from '../types';

interface RequestAuditViewProps {
  lang: Locale;
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

export default function RequestAuditView({ lang }: RequestAuditViewProps) {
  const [summary, setSummary] = useState<TelemetrySummary>(emptySummary);
  const [traces, setTraces] = useState<RequestTrace[]>([]);
  const [selectedProjectId, setSelectedProjectId] = useState('');
  const [selectedTraceId, setSelectedTraceId] = useState('');
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<'request' | 'response' | null>(null);

  const labels = useMemo(
    () =>
      lang === 'zh'
        ? {
            title: '请求审计',
            subtitle: '按项目顺序查看每一组 request / response',
            allProjects: '全部项目',
            search: '搜索项目、模型、路由',
            refresh: '刷新',
            noRows: '暂无请求记录',
            noPreview: '未保存正文预览',
            request: '请求',
            response: '响应',
            sequence: '顺序',
            status: '状态',
            model: '模型',
            route: '路由',
            provider: '服务商',
            latency: '耗时',
            bytes: '大小',
            project: '项目',
            capture: '采集',
            copy: '复制',
            copied: '已复制',
            error: '错误'
          }
        : {
            title: 'Request Audit',
            subtitle: 'Inspect each ordered request / response pair by project',
            allProjects: 'All projects',
            search: 'Search project, model, route',
            refresh: 'Refresh',
            noRows: 'No request records',
            noPreview: 'Body preview was not captured',
            request: 'Request',
            response: 'Response',
            sequence: 'Seq',
            status: 'Status',
            model: 'Model',
            route: 'Route',
            provider: 'Provider',
            latency: 'Latency',
            bytes: 'Size',
            project: 'Project',
            capture: 'Capture',
            copy: 'Copy',
            copied: 'Copied',
            error: 'Error'
          },
    [lang]
  );

  const loadAudit = useCallback(async () => {
    setLoading(true);
    try {
      const [nextSummary, nextTraces] = await Promise.all([
        api.getTelemetrySummary(),
        api.getTelemetryTraces(500, selectedProjectId || undefined, selectedProjectId ? 'asc' : 'desc')
      ]);
      setSummary(nextSummary);
      setTraces(nextTraces);
      setError(null);
      setSelectedTraceId((current) => {
        if (current && nextTraces.some((trace) => trace.id === current)) return current;
        return nextTraces[0]?.id || '';
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [selectedProjectId]);

  useEffect(() => {
    loadAudit();
  }, [loadAudit]);

  const filteredTraces = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return traces;
    return traces.filter((trace) =>
      [
        trace.projectName,
        trace.projectId,
        trace.routePath,
        trace.requestModel,
        trace.providerName,
        trace.statusCode?.toString()
      ]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(needle))
    );
  }, [query, traces]);

  const selectedTrace = filteredTraces.find((trace) => trace.id === selectedTraceId) || filteredTraces[0];
  const projectOptions = summary.byProject;

  const copyPreview = async (kind: 'request' | 'response', value?: string) => {
    if (!value) return;
    await navigator.clipboard.writeText(prettyPreview(value));
    setCopied(kind);
    window.setTimeout(() => setCopied(null), 1200);
  };

  return (
    <div id="request-audit-tab-panel" className="h-[calc(100vh-112px)] min-h-[620px] flex flex-col gap-4 text-slate-100 font-sans">
      <section className="bg-[#0F172A] border border-[#1E293B] rounded-2xl p-4 md:p-5 shadow-sm shrink-0">
        <div className="flex flex-col xl:flex-row xl:items-center xl:justify-between gap-4">
          <div className="flex min-w-0 items-center gap-2.5">
            <div className="p-2 bg-indigo-600/10 border border-indigo-500/20 rounded-lg text-indigo-400">
              <FileSearch className="h-4.5 w-4.5" />
            </div>
            <div className="min-w-0">
              <h2 className="text-sm font-bold text-slate-100">{labels.title}</h2>
              <p className="text-[11px] text-slate-500 font-mono mt-0.5">{labels.subtitle}</p>
            </div>
          </div>

          <div className="flex flex-col sm:flex-row sm:items-center gap-2">
            <select
              id="audit-project-select"
              value={selectedProjectId}
              onChange={(event) => {
                setSelectedProjectId(event.target.value);
                setSelectedTraceId('');
              }}
              className="h-9 min-w-[220px] bg-[#090D16] text-slate-200 border border-[#1E293B] rounded-lg px-3 text-xs font-mono focus:outline-none"
            >
              <option value="">{labels.allProjects}</option>
              {projectOptions.map((project) => (
                <option key={project.projectId} value={project.projectId}>
                  {project.projectName} ({project.requests})
                </option>
              ))}
            </select>

            <div className="relative min-w-[220px]">
              <Search className="absolute left-3 top-2.5 w-3.5 h-3.5 text-slate-500" />
              <input
                id="audit-search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={labels.search}
                className="h-9 w-full bg-[#090D16] text-slate-200 border border-[#1E293B] rounded-lg pl-9 pr-3 text-xs font-mono focus:outline-none placeholder-slate-600"
              />
            </div>

            <button
              id="audit-refresh"
              type="button"
              onClick={loadAudit}
              className="h-9 flex items-center justify-center gap-1.5 bg-slate-850 hover:bg-slate-800 text-slate-300 border border-[#1E293B] rounded-lg px-3 text-xs font-semibold font-mono transition-colors cursor-pointer"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
              <span>{labels.refresh}</span>
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

      <section className="grid grid-cols-1 xl:grid-cols-[minmax(360px,0.95fr)_minmax(0,1.55fr)] gap-4 flex-1 min-h-0">
        <div className="bg-[#0F172A] border border-[#1E293B] rounded-2xl shadow-sm min-h-0 overflow-hidden flex flex-col">
          <div className="px-4 py-3 border-b border-[#1E293B] flex items-center justify-between">
            <span className="text-xs font-bold text-slate-100">{labels.request}</span>
            <span className="text-[10px] font-mono text-slate-500">{filteredTraces.length}</span>
          </div>
          <div className="flex-1 min-h-0 overflow-y-auto">
            {filteredTraces.length === 0 ? (
              <div className="h-full flex items-center justify-center text-xs font-mono text-slate-500 p-6 text-center">
                {labels.noRows}
              </div>
            ) : (
              filteredTraces.map((trace) => {
                const active = trace.id === selectedTrace?.id;
                return (
                  <button
                    id={`audit-row-${trace.id}`}
                    key={trace.id}
                    type="button"
                    onClick={() => setSelectedTraceId(trace.id)}
                    className={`w-full text-left px-4 py-3 border-b border-[#1E293B]/80 transition-colors cursor-pointer ${
                      active ? 'bg-indigo-600/12' : 'hover:bg-slate-800/35'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-[10px] font-mono font-bold text-indigo-300">#{trace.projectSequence}</span>
                          <span className="text-xs font-semibold text-slate-100 truncate">{trace.projectName}</span>
                        </div>
                        <div className="mt-1 text-[10px] font-mono text-slate-500 truncate">{trace.routePath}</div>
                        <div className="mt-1 text-[10px] font-mono text-slate-400 truncate">{trace.requestModel || '-'}</div>
                      </div>
                      <StatusBadge trace={trace} />
                    </div>
                  </button>
                );
              })
            )}
          </div>
        </div>

        <div className="bg-[#0F172A] border border-[#1E293B] rounded-2xl shadow-sm min-h-0 overflow-hidden flex flex-col">
          {selectedTrace ? (
            <>
              <div className="px-4 py-3 border-b border-[#1E293B] grid grid-cols-2 md:grid-cols-4 gap-3 text-[10px] font-mono">
                <MetaItem label={labels.sequence} value={`#${selectedTrace.projectSequence}`} />
                <MetaItem label={labels.status} value={String(selectedTrace.statusCode ?? 'ERR')} />
                <MetaItem label={labels.latency} value={formatMs(selectedTrace.latencyMs)} />
                <MetaItem label={labels.bytes} value={formatBytes(selectedTrace.requestBytes + (selectedTrace.responseBytes ?? 0))} />
                <MetaItem label={labels.project} value={selectedTrace.projectName} />
                <MetaItem label={labels.model} value={selectedTrace.requestModel || '-'} />
                <MetaItem label={labels.provider} value={selectedTrace.providerName} />
                <MetaItem label={labels.capture} value={selectedTrace.captureLevel} />
              </div>

              <div className="grid grid-cols-1 lg:grid-cols-2 gap-0 flex-1 min-h-0">
                <PreviewPanel
                  title={labels.request}
                  value={selectedTrace.requestBodyPreview}
                  emptyText={labels.noPreview}
                  copyLabel={copied === 'request' ? labels.copied : labels.copy}
                  onCopy={() => copyPreview('request', selectedTrace.requestBodyPreview)}
                />
                <PreviewPanel
                  title={labels.response}
                  value={selectedTrace.responseBodyPreview}
                  emptyText={labels.noPreview}
                  copyLabel={copied === 'response' ? labels.copied : labels.copy}
                  onCopy={() => copyPreview('response', selectedTrace.responseBodyPreview)}
                />
              </div>
            </>
          ) : (
            <div className="h-full flex items-center justify-center text-xs font-mono text-slate-500 p-6 text-center">
              {labels.noRows}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

function StatusBadge({ trace }: { trace: RequestTrace }) {
  return (
    <span className={`shrink-0 inline-flex items-center gap-1 px-2 py-1 rounded-md border text-[10px] font-mono font-bold ${
      trace.success
        ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
        : 'bg-rose-500/10 text-rose-400 border-rose-500/20'
    }`}>
      {trace.success ? <CheckCircle2 className="h-3 w-3" /> : <XCircle className="h-3 w-3" />}
      {trace.statusCode ?? 'ERR'}
    </span>
  );
}

function MetaItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <div className="text-slate-500 uppercase">{label}</div>
      <div className="mt-0.5 text-slate-200 truncate">{value}</div>
    </div>
  );
}

function PreviewPanel({
  title,
  value,
  emptyText,
  copyLabel,
  onCopy
}: {
  title: string;
  value?: string;
  emptyText: string;
  copyLabel: string;
  onCopy: () => void;
}) {
  return (
    <div className="min-h-[280px] lg:min-h-0 flex flex-col border-t lg:border-t-0 lg:border-l first:lg:border-l-0 border-[#1E293B]">
      <div className="h-11 px-4 border-b border-[#1E293B] flex items-center justify-between gap-3">
        <h3 className="text-xs font-bold text-slate-100">{title}</h3>
        <button
          type="button"
          disabled={!value}
          onClick={onCopy}
          className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border border-[#1E293B] bg-[#090D16] hover:bg-slate-800 text-[10px] font-mono text-slate-300 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
        >
          <Clipboard className="h-3.5 w-3.5" />
          <span>{copyLabel}</span>
        </button>
      </div>
      <pre className="flex-1 min-h-0 overflow-auto bg-[#090D16] p-4 text-[11px] leading-relaxed font-mono text-slate-300 whitespace-pre-wrap break-words">
        {value ? prettyPreview(value) : emptyText}
      </pre>
    </div>
  );
}

function prettyPreview(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

function formatMs(value: number) {
  return `${Math.round(value)}ms`;
}

function formatBytes(value: number) {
  if (value < 1024) return `${value}B`;
  if (value < 1024 * 1024) return `${Math.round(value / 10.24) / 100}KB`;
  return `${Math.round(value / 10485.76) / 100}MB`;
}
