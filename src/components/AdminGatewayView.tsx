import { useEffect, useMemo, useState } from 'react';
import type { HTMLAttributes, ReactNode } from 'react';
import {
  Ban,
  Copy,
  Database,
  FileJson,
  FolderKanban,
  KeyRound,
  Play,
  RefreshCw,
  ShieldCheck,
  Users,
} from 'lucide-react';
import type {
  DistillationJob,
  GatewayKey,
  GatewayKeyUsageBucket,
  GatewayProject,
  GatewayUsageBucket,
  GatewayUsageSummary,
  GatewayUser,
} from '../types';
import { Locale } from '../locales';
import { api } from '../lib/api';

interface AdminGatewayViewProps {
  lang: Locale;
}

interface KeyFormState {
  userId: string;
  projectId: string;
  name: string;
  quotaRequests: string;
  quotaTokens: string;
}

interface DistillationFormState {
  userId: string;
  projectId: string;
  name: string;
  limit: string;
  includeFailures: boolean;
}

const emptyUsage: GatewayUsageSummary = {
  totalRequests: 0,
  totalTokens: 0,
  estimatedCostUsd: 0,
  byUser: [],
  byProject: [],
  byKey: [],
};

const numberFormatter = new Intl.NumberFormat('en-US');
const costFormatter = new Intl.NumberFormat('en-US', {
  minimumFractionDigits: 4,
  maximumFractionDigits: 6,
});

export default function AdminGatewayView({ lang }: AdminGatewayViewProps) {
  const copyText = lang === 'zh' ? '复制' : 'Copy';
  const [users, setUsers] = useState<GatewayUser[]>([]);
  const [projects, setProjects] = useState<GatewayProject[]>([]);
  const [keys, setKeys] = useState<GatewayKey[]>([]);
  const [usage, setUsage] = useState<GatewayUsageSummary>(emptyUsage);
  const [jobs, setJobs] = useState<DistillationJob[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string>('');
  const [latestRawKey, setLatestRawKey] = useState<{ name: string; value: string } | null>(null);

  const [userName, setUserName] = useState('');
  const [userEmail, setUserEmail] = useState('');
  const [projectUserId, setProjectUserId] = useState('');
  const [projectName, setProjectName] = useState('');
  const [projectSlug, setProjectSlug] = useState('');
  const [keyForm, setKeyForm] = useState<KeyFormState>({
    userId: '',
    projectId: '',
    name: '',
    quotaRequests: '0',
    quotaTokens: '0',
  });
  const [distillForm, setDistillForm] = useState<DistillationFormState>({
    userId: '',
    projectId: '',
    name: 'Distillation Export',
    limit: '1000',
    includeFailures: false,
  });

  const text = {
    title: lang === 'zh' ? '租户与密钥管理' : 'Tenant Key Management',
    user: lang === 'zh' ? '使用者' : 'User',
    project: lang === 'zh' ? '项目' : 'Project',
    key: lang === 'zh' ? '访问 Key' : 'Access Key',
    activeKeys: lang === 'zh' ? '有效 Key' : 'Active Keys',
    requests: lang === 'zh' ? '请求' : 'Requests',
    tokens: lang === 'zh' ? 'Tokens' : 'Tokens',
    cost: lang === 'zh' ? '成本' : 'Cost',
    createUser: lang === 'zh' ? '新增使用者' : 'Create User',
    createProject: lang === 'zh' ? '新增项目' : 'Create Project',
    issueKey: lang === 'zh' ? '签发 Key' : 'Issue Key',
    distill: lang === 'zh' ? '蒸馏导出' : 'Distillation Export',
    usageByUser: lang === 'zh' ? '按使用者统计' : 'Usage by User',
    usageByProject: lang === 'zh' ? '按项目统计' : 'Usage by Project',
    usageByKey: lang === 'zh' ? '按 Key 统计' : 'Usage by Key',
    refresh: lang === 'zh' ? '刷新' : 'Refresh',
    revoke: lang === 'zh' ? '撤销' : 'Revoke',
    run: lang === 'zh' ? '运行' : 'Run',
    empty: lang === 'zh' ? '暂无数据' : 'No data',
  };

  const activeKeyCount = keys.filter((key) => key.status === 'active').length;
  const projectsForProjectForm = users.length > 0 ? users : [];
  const selectedKeyProjects = useMemo(
    () => projects.filter((project) => !keyForm.userId || project.userId === keyForm.userId),
    [keyForm.userId, projects],
  );
  const selectedDistillProjects = useMemo(
    () => projects.filter((project) => !distillForm.userId || project.userId === distillForm.userId),
    [distillForm.userId, projects],
  );

  const load = async () => {
    setLoading(true);
    try {
      const [nextUsers, nextProjects, nextKeys, nextUsage, nextJobs] = await Promise.all([
        api.getGatewayUsers(),
        api.getGatewayProjects(),
        api.getGatewayKeys(),
        api.getGatewayUsageSummary(),
        api.getDistillationJobs(),
      ]);
      setUsers(nextUsers);
      setProjects(nextProjects);
      setKeys(nextKeys);
      setUsage(nextUsage);
      setJobs(nextJobs);

      const firstUserId = nextUsers[0]?.id || '';
      const firstProjectId = nextProjects.find((project) => project.userId === firstUserId)?.id || nextProjects[0]?.id || '';
      setProjectUserId((current) => current || firstUserId);
      setKeyForm((current) => ({
        ...current,
        userId: current.userId || firstUserId,
        projectId: current.projectId || firstProjectId,
      }));
      setDistillForm((current) => ({
        ...current,
        userId: current.userId || firstUserId,
        projectId: current.projectId || firstProjectId,
      }));
      setStatus('');
    } catch (err) {
      setStatus(errorText(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const createUser = async () => {
    const displayName = userName.trim();
    if (!displayName) return setStatus(lang === 'zh' ? '使用者名称必填' : 'User name is required');
    setBusy(true);
    try {
      const response = await api.createGatewayUser({
        displayName,
        email: userEmail.trim() || undefined,
      });
      setUserName('');
      setUserEmail('');
      setProjectUserId(response.user.id);
      setKeyForm((current) => ({ ...current, userId: response.user.id, projectId: '' }));
      setDistillForm((current) => ({ ...current, userId: response.user.id, projectId: '' }));
      setStatus(lang === 'zh' ? '使用者已创建' : 'User created');
      await load();
    } catch (err) {
      setStatus(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const createProject = async () => {
    const name = projectName.trim();
    const userId = projectUserId || users[0]?.id;
    if (!userId) return setStatus(lang === 'zh' ? '请先创建使用者' : 'Create a user first');
    if (!name) return setStatus(lang === 'zh' ? '项目名称必填' : 'Project name is required');
    setBusy(true);
    try {
      const response = await api.createGatewayProject({
        userId,
        name,
        slug: projectSlug.trim() || undefined,
      });
      setProjectName('');
      setProjectSlug('');
      setKeyForm((current) => ({
        ...current,
        userId,
        projectId: response.project.id,
      }));
      setDistillForm((current) => ({
        ...current,
        userId,
        projectId: response.project.id,
      }));
      setStatus(lang === 'zh' ? '项目已创建' : 'Project created');
      await load();
    } catch (err) {
      setStatus(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const issueKey = async () => {
    const userId = keyForm.userId || users[0]?.id;
    const projectId = keyForm.projectId || selectedKeyProjects[0]?.id;
    const name = keyForm.name.trim();
    if (!userId) return setStatus(lang === 'zh' ? '请先创建使用者' : 'Create a user first');
    if (!projectId) return setStatus(lang === 'zh' ? '请先创建项目' : 'Create a project first');
    if (!name) return setStatus(lang === 'zh' ? 'Key 名称必填' : 'Key name is required');
    setBusy(true);
    try {
      const response = await api.createGatewayKey({
        userId,
        projectId,
        name,
        quotaRequests: parsePositiveInteger(keyForm.quotaRequests),
        quotaTokens: parsePositiveInteger(keyForm.quotaTokens),
      });
      setLatestRawKey({ name: response.key.name, value: response.rawKey });
      setKeyForm((current) => ({ ...current, name: '' }));
      setStatus(lang === 'zh' ? 'Key 已签发' : 'Key issued');
      await load();
    } catch (err) {
      setStatus(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const revokeKey = async (keyId: string) => {
    setBusy(true);
    try {
      await api.revokeGatewayKey(keyId);
      setLatestRawKey(null);
      setStatus(lang === 'zh' ? 'Key 已撤销' : 'Key revoked');
      await load();
    } catch (err) {
      setStatus(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const runDistillation = async () => {
    const name = distillForm.name.trim() || 'Distillation Export';
    setBusy(true);
    try {
      const selectedProject = projects.find((project) => project.id === distillForm.projectId);
      const response = await api.createDistillationJob({
        name,
        userId: distillForm.userId || selectedProject?.userId || undefined,
        projectId: distillForm.projectId || undefined,
        limit: parsePositiveInteger(distillForm.limit) || 1000,
        includeFailures: distillForm.includeFailures,
      });
      setStatus(
        response.job.exportPath
          ? `${lang === 'zh' ? '导出完成' : 'Export completed'}: ${response.job.exportPath}`
          : lang === 'zh'
            ? '导出任务已创建'
            : 'Export job created',
      );
      await load();
    } catch (err) {
      setStatus(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const copy = async (value: string) => {
    await navigator.clipboard.writeText(value);
    setStatus(lang === 'zh' ? '已复制' : 'Copied');
  };

  return (
    <div className="space-y-4 md:space-y-5">
      <section className="bg-[#0F172A] border border-[#1E293B] rounded-2xl p-4 md:p-5 shadow-sm">
        <div className="flex flex-col lg:flex-row lg:items-start justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-indigo-600/10 border border-indigo-500/20 rounded-lg text-indigo-400">
              <ShieldCheck className="h-5 w-5" />
            </div>
            <div>
              <h2 className="text-base font-bold text-slate-100">{text.title}</h2>
              <div className="mt-1 flex flex-wrap items-center gap-2 text-[10px] font-mono text-slate-400">
                <span>{users.length} {text.user}</span>
                <span className="text-slate-600">/</span>
                <span>{projects.length} {text.project}</span>
                <span className="text-slate-600">/</span>
                <span>{activeKeyCount} {text.activeKeys}</span>
              </div>
            </div>
          </div>
          <button
            type="button"
            onClick={load}
            disabled={loading || busy}
            className="inline-flex items-center justify-center gap-1.5 h-9 px-3 rounded-lg bg-slate-850 hover:bg-slate-800 border border-[#1E293B] text-slate-300 text-xs font-mono font-bold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
            {text.refresh}
          </button>
        </div>

        <div className="mt-5 grid grid-cols-2 lg:grid-cols-4 gap-3">
          <Metric label={text.requests} value={numberFormatter.format(usage.totalRequests)} icon={Database} />
          <Metric label={text.tokens} value={numberFormatter.format(usage.totalTokens)} icon={FileJson} />
          <Metric label={text.cost} value={formatCost(usage.estimatedCostUsd)} icon={ShieldCheck} />
          <Metric label={text.key} value={numberFormatter.format(keys.length)} icon={KeyRound} />
        </div>

        {status && (
          <div className="mt-4 border border-[#1E293B] bg-[#090D16] text-slate-300 rounded-lg px-3 py-2 text-xs font-mono break-words">
            {status}
          </div>
        )}

        {latestRawKey && (
          <div className="mt-4 border border-amber-500/30 bg-amber-500/10 rounded-xl p-3">
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
              <div className="min-w-0">
                <div className="text-[10px] uppercase font-mono font-bold text-amber-300">{latestRawKey.name}</div>
                <div className="mt-1 font-mono text-xs text-amber-100 break-all">{latestRawKey.value}</div>
              </div>
              <button
                type="button"
                onClick={() => copy(latestRawKey.value)}
                className="inline-flex items-center justify-center gap-1.5 h-8 px-3 rounded-lg bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 text-amber-200 text-xs font-mono font-bold cursor-pointer"
              >
                <Copy className="h-3.5 w-3.5" />
                {copyText}
              </button>
            </div>
          </div>
        )}
      </section>

      <div className="grid grid-cols-1 xl:grid-cols-3 gap-4">
        <Panel icon={Users} title={text.createUser}>
          <div className="space-y-3">
            <TextInput
              label={lang === 'zh' ? '名称' : 'Name'}
              value={userName}
              onChange={setUserName}
              placeholder={lang === 'zh' ? '例如：张三' : 'Ada Lovelace'}
            />
            <TextInput
              label="Email"
              value={userEmail}
              onChange={setUserEmail}
              placeholder="name@example.com"
            />
            <ActionButton onClick={createUser} disabled={busy} icon={Users}>
              {text.createUser}
            </ActionButton>
          </div>
        </Panel>

        <Panel icon={FolderKanban} title={text.createProject}>
          <div className="space-y-3">
            <SelectInput
              label={text.user}
              value={projectUserId}
              onChange={setProjectUserId}
              options={projectsForProjectForm.map((user) => ({ value: user.id, label: user.displayName }))}
            />
            <TextInput
              label={lang === 'zh' ? '项目名' : 'Project Name'}
              value={projectName}
              onChange={(value) => {
                setProjectName(value);
                if (!projectSlug) setProjectSlug(slugify(value));
              }}
              placeholder={lang === 'zh' ? '例如：论文检测产品' : 'Research Portal'}
            />
            <TextInput
              label="Slug"
              value={projectSlug}
              onChange={setProjectSlug}
              placeholder="research-portal"
            />
            <ActionButton onClick={createProject} disabled={busy || users.length === 0} icon={FolderKanban}>
              {text.createProject}
            </ActionButton>
          </div>
        </Panel>

        <Panel icon={KeyRound} title={text.issueKey}>
          <div className="space-y-3">
            <SelectInput
              label={text.user}
              value={keyForm.userId}
              onChange={(value) => {
                const nextProjectId = projects.find((project) => project.userId === value)?.id || '';
                setKeyForm((current) => ({ ...current, userId: value, projectId: nextProjectId }));
              }}
              options={users.map((user) => ({ value: user.id, label: user.displayName }))}
            />
            <SelectInput
              label={text.project}
              value={keyForm.projectId}
              onChange={(value) => setKeyForm((current) => ({ ...current, projectId: value }))}
              options={selectedKeyProjects.map((project) => ({ value: project.id, label: project.name }))}
            />
            <TextInput
              label={lang === 'zh' ? 'Key 名称' : 'Key Name'}
              value={keyForm.name}
              onChange={(value) => setKeyForm((current) => ({ ...current, name: value }))}
              placeholder={lang === 'zh' ? 'Codex 工作站' : 'Codex Workstation'}
            />
            <div className="grid grid-cols-2 gap-3">
              <TextInput
                label={lang === 'zh' ? '请求额度' : 'Request Quota'}
                value={keyForm.quotaRequests}
                onChange={(value) => setKeyForm((current) => ({ ...current, quotaRequests: value }))}
                inputMode="numeric"
              />
              <TextInput
                label={lang === 'zh' ? 'Token 额度' : 'Token Quota'}
                value={keyForm.quotaTokens}
                onChange={(value) => setKeyForm((current) => ({ ...current, quotaTokens: value }))}
                inputMode="numeric"
              />
            </div>
            <ActionButton onClick={issueKey} disabled={busy || users.length === 0 || projects.length === 0} icon={KeyRound}>
              {text.issueKey}
            </ActionButton>
          </div>
        </Panel>
      </div>

      <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
        <Panel icon={KeyRound} title={text.activeKeys}>
          <div className="space-y-2 max-h-[360px] overflow-auto pr-1">
            {keys.length === 0 ? (
              <EmptyState label={text.empty} />
            ) : (
              keys.map((key) => (
                <div key={key.id} className="border border-[#1E293B] bg-[#090D16] rounded-lg p-3">
                  <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-semibold text-slate-100 text-sm">{key.name}</span>
                        <StatusBadge status={key.status} />
                      </div>
                      <div className="mt-1 text-[10px] font-mono text-slate-500 break-all">
                        {key.keyPrefix}... / {key.userName} / {key.projectName}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() => revokeKey(key.id)}
                      disabled={busy || key.status !== 'active'}
                      className="inline-flex items-center justify-center gap-1.5 h-8 px-2.5 rounded-lg bg-rose-950/30 hover:bg-rose-900/40 border border-rose-800/40 text-rose-300 text-[10px] font-mono font-bold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                    >
                      <Ban className="h-3.5 w-3.5" />
                      {text.revoke}
                    </button>
                  </div>
                  <div className="mt-3 grid grid-cols-2 md:grid-cols-4 gap-2 text-[10px] font-mono">
                    <UsageCell label={text.requests} value={`${key.usedRequests}/${quotaText(key.quotaRequests)}`} />
                    <UsageCell label={text.tokens} value={`${key.usedTokens}/${quotaText(key.quotaTokens)}`} />
                    <UsageCell label="Last" value={shortDate(key.lastUsedAt)} />
                    <UsageCell label="Created" value={shortDate(key.createdAt)} />
                  </div>
                </div>
              ))
            )}
          </div>
        </Panel>

        <Panel icon={FileJson} title={text.distill}>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
            <div className="space-y-3">
              <TextInput
                label={lang === 'zh' ? '任务名' : 'Job Name'}
                value={distillForm.name}
                onChange={(value) => setDistillForm((current) => ({ ...current, name: value }))}
              />
              <SelectInput
                label={text.user}
                value={distillForm.userId}
                onChange={(value) => {
                  const nextProjectId = projects.find((project) => project.userId === value)?.id || '';
                  setDistillForm((current) => ({ ...current, userId: value, projectId: nextProjectId }));
                }}
                options={[{ value: '', label: lang === 'zh' ? '全部使用者' : 'All Users' }].concat(
                  users.map((user) => ({ value: user.id, label: user.displayName })),
                )}
              />
              <SelectInput
                label={text.project}
                value={distillForm.projectId}
                onChange={(value) => setDistillForm((current) => ({ ...current, projectId: value }))}
                options={[{ value: '', label: lang === 'zh' ? '全部项目' : 'All Projects' }].concat(
                  selectedDistillProjects.map((project) => ({ value: project.id, label: project.name })),
                )}
              />
              <div className="grid grid-cols-[1fr_auto] gap-3 items-end">
                <TextInput
                  label="Limit"
                  value={distillForm.limit}
                  onChange={(value) => setDistillForm((current) => ({ ...current, limit: value }))}
                  inputMode="numeric"
                />
                <label className="h-9 inline-flex items-center gap-2 px-3 rounded-lg border border-[#1E293B] bg-[#090D16] text-[10px] font-mono text-slate-300 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={distillForm.includeFailures}
                    onChange={(event) => setDistillForm((current) => ({ ...current, includeFailures: event.target.checked }))}
                    className="accent-indigo-500"
                  />
                  {lang === 'zh' ? '失败项' : 'Failures'}
                </label>
              </div>
              <ActionButton onClick={runDistillation} disabled={busy} icon={Play}>
                {text.run}
              </ActionButton>
            </div>

            <div className="space-y-2 max-h-[300px] overflow-auto pr-1">
              {jobs.length === 0 ? (
                <EmptyState label={text.empty} />
              ) : (
                jobs.map((job) => (
                  <div key={job.id} className="border border-[#1E293B] bg-[#090D16] rounded-lg p-3">
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <div className="font-semibold text-slate-100 text-sm truncate">{job.name}</div>
                        <div className="mt-1 text-[10px] font-mono text-slate-500">
                          {job.traceCount} traces / {shortDate(job.completedAt || job.createdAt)}
                        </div>
                      </div>
                      <StatusBadge status={job.status} />
                    </div>
                    {job.exportPath && (
                      <div className="mt-2 flex items-center gap-2 text-[10px] font-mono text-slate-400">
                        <span className="min-w-0 break-all">{job.exportPath}</span>
                        <button
                          type="button"
                          onClick={() => copy(job.exportPath || '')}
                          className="shrink-0 p-1 rounded-md border border-[#1E293B] bg-slate-850 text-slate-300 hover:bg-slate-800 cursor-pointer"
                          title={copyText}
                        >
                          <Copy className="h-3 w-3" />
                        </button>
                      </div>
                    )}
                    {job.error && <div className="mt-2 text-[10px] text-rose-300 break-words">{job.error}</div>}
                  </div>
                ))
              )}
            </div>
          </div>
        </Panel>
      </div>

      <div className="grid grid-cols-1 xl:grid-cols-3 gap-4">
        <UsagePanel title={text.usageByUser} items={usage.byUser} empty={text.empty} />
        <UsagePanel title={text.usageByProject} items={usage.byProject} empty={text.empty} />
        <KeyUsagePanel title={text.usageByKey} items={usage.byKey} empty={text.empty} />
      </div>
    </div>
  );
}

function Panel({
  icon: Icon,
  title,
  children,
}: {
  icon: typeof Users;
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="bg-[#0F172A] border border-[#1E293B] rounded-2xl p-4 shadow-sm min-w-0">
      <div className="flex items-center gap-2 mb-4">
        <div className="p-1.5 bg-indigo-600/10 border border-indigo-500/20 rounded-lg text-indigo-400">
          <Icon className="h-4 w-4" />
        </div>
        <h3 className="font-bold text-slate-100 text-sm">{title}</h3>
      </div>
      {children}
    </section>
  );
}

function Metric({
  label,
  value,
  icon: Icon,
}: {
  label: string;
  value: string;
  icon: typeof Users;
}) {
  return (
    <div className="bg-[#090D16] border border-[#1E293B] rounded-xl p-3">
      <div className="flex items-center justify-between gap-3">
        <div className="text-[10px] uppercase font-mono font-bold text-slate-500">{label}</div>
        <Icon className="h-4 w-4 text-indigo-400" />
      </div>
      <div className="mt-2 text-lg font-mono font-bold text-slate-100 break-words">{value}</div>
    </div>
  );
}

function TextInput({
  label,
  value,
  onChange,
  placeholder,
  inputMode,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  inputMode?: HTMLAttributes<HTMLInputElement>['inputMode'];
}) {
  return (
    <label className="block">
      <span className="block mb-1.5 text-[10px] uppercase font-mono font-bold text-slate-500">{label}</span>
      <input
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        inputMode={inputMode}
        className="w-full h-9 bg-[#090D16] border border-[#1E293B] rounded-lg px-3 text-xs text-slate-100 placeholder:text-slate-600 focus:outline-none focus:border-indigo-500"
      />
    </label>
  );
}

function SelectInput({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string }[];
}) {
  return (
    <label className="block">
      <span className="block mb-1.5 text-[10px] uppercase font-mono font-bold text-slate-500">{label}</span>
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="w-full h-9 bg-[#090D16] border border-[#1E293B] rounded-lg px-3 text-xs text-slate-100 focus:outline-none focus:border-indigo-500"
      >
        {options.length === 0 ? (
          <option value="">None</option>
        ) : (
          options.map((option) => (
            <option key={`${label}-${option.value || 'all'}`} value={option.value}>
              {option.label}
            </option>
          ))
        )}
      </select>
    </label>
  );
}

function ActionButton({
  children,
  onClick,
  disabled,
  icon: Icon,
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  icon: typeof Users;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="w-full inline-flex items-center justify-center gap-1.5 h-9 px-3 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-mono font-bold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
    >
      <Icon className="h-3.5 w-3.5" />
      {children}
    </button>
  );
}

function UsageCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-[#1E293B] bg-[#111827] px-2 py-1.5 min-w-0">
      <div className="text-slate-500 uppercase">{label}</div>
      <div className="text-slate-200 font-bold truncate">{value}</div>
    </div>
  );
}

function UsagePanel({
  title,
  items,
  empty,
}: {
  title: string;
  items: GatewayUsageBucket[];
  empty: string;
}) {
  return (
    <Panel icon={Database} title={title}>
      <div className="space-y-2 max-h-[320px] overflow-auto pr-1">
        {items.length === 0 ? (
          <EmptyState label={empty} />
        ) : (
          items.map((item) => (
            <div key={item.id}>
              <UsageRow
                name={item.name}
                requests={item.requests}
                tokens={item.tokens}
                cost={item.estimatedCostUsd}
              />
            </div>
          ))
        )}
      </div>
    </Panel>
  );
}

function KeyUsagePanel({
  title,
  items,
  empty,
}: {
  title: string;
  items: GatewayKeyUsageBucket[];
  empty: string;
}) {
  return (
    <Panel icon={KeyRound} title={title}>
      <div className="space-y-2 max-h-[320px] overflow-auto pr-1">
        {items.length === 0 ? (
          <EmptyState label={empty} />
        ) : (
          items.map((item) => (
            <div key={item.keyId}>
              <UsageRow
                name={item.keyName}
                subtitle={`${item.userName} / ${item.projectName}`}
                requests={item.usedRequests}
                tokens={item.usedTokens}
                cost={item.estimatedCostUsd}
                requestQuota={item.quotaRequests}
                tokenQuota={item.quotaTokens}
              />
            </div>
          ))
        )}
      </div>
    </Panel>
  );
}

function UsageRow({
  name,
  subtitle,
  requests,
  tokens,
  cost,
  requestQuota,
  tokenQuota,
}: {
  name: string;
  subtitle?: string;
  requests: number;
  tokens: number;
  cost: number;
  requestQuota?: number;
  tokenQuota?: number;
}) {
  return (
    <div className="border border-[#1E293B] bg-[#090D16] rounded-lg p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-semibold text-slate-100 truncate">{name}</div>
          {subtitle && <div className="mt-1 text-[10px] font-mono text-slate-500 truncate">{subtitle}</div>}
        </div>
        <div className="shrink-0 text-right text-[10px] font-mono text-slate-400">
          <div>{numberFormatter.format(requests)} req</div>
          <div>{numberFormatter.format(tokens)} tok</div>
        </div>
      </div>
      <div className="mt-2 flex items-center justify-between gap-2 text-[10px] font-mono text-slate-500">
        <span>{formatCost(cost)}</span>
        {(requestQuota !== undefined || tokenQuota !== undefined) && (
          <span>
            {quotaText(requestQuota || 0)} req / {quotaText(tokenQuota || 0)} tok
          </span>
        )}
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const isActive = status === 'active' || status === 'completed';
  const isRunning = status === 'running';
  return (
    <span
      className={`inline-flex items-center px-2 py-0.5 rounded-md border text-[9px] font-mono font-bold uppercase ${
        isActive
          ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
          : isRunning
            ? 'bg-amber-500/10 text-amber-300 border-amber-500/20'
            : 'bg-rose-500/10 text-rose-300 border-rose-500/20'
      }`}
    >
      {status}
    </span>
  );
}

function EmptyState({ label }: { label: string }) {
  return (
    <div className="border border-dashed border-[#1E293B] bg-[#090D16] rounded-lg px-3 py-8 text-center text-xs font-mono text-slate-500">
      {label}
    </div>
  );
}

function parsePositiveInteger(value: string): number {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
}

function quotaText(value: number): string {
  return value > 0 ? numberFormatter.format(value) : '∞';
}

function formatCost(value: number): string {
  const normalized = Object.is(value, -0) ? 0 : value;
  return `$${costFormatter.format(normalized)}`;
}

function shortDate(value?: string): string {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString(undefined, { month: '2-digit', day: '2-digit' });
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}
