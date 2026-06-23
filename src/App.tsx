import { useState, useEffect } from 'react';
import { Agent, Provider, Model, McpServer, LogEntry, GatewayState } from './types';
import Sidebar from './components/Sidebar';
import Dashboard from './components/Dashboard';
import AgentsView from './components/AgentsView';
import ProvidersView from './components/ProvidersView';
import ChatView from './components/ChatView';
import LogsView from './components/LogsView';
import TelemetryView from './components/TelemetryView';
import RequestAuditView from './components/RequestAuditView';
import AdminGatewayView from './components/AdminGatewayView';
import { Sun, Moon } from 'lucide-react';
import { translations, Locale } from './locales';
import { api, apiRequest } from './lib/api';

export default function App() {
  const [lang, setLang] = useState<Locale>(() => {
    const saved = localStorage.getItem('ah_lang');
    return (saved as Locale) || 'en';
  });

  const [theme, setTheme] = useState<'light' | 'dark'>(() => {
    const saved = localStorage.getItem('ah_theme');
    return (saved as 'light' | 'dark') || 'dark';
  });

  const t = (key: keyof typeof translations['en']): string => {
    return translations[lang]?.[key] || translations['en'][key] || String(key);
  };

  const [activeTab, setActiveTab] = useState<string>('dashboard');

  const activeTabTitle = () => {
    const titles = {
      zh: {
        dashboard: '主控制中心',
        providers: 'AI 通道管理',
        agents: 'AI 客户端代理',
        chat: '沙盒连通测试',
        telemetry: '项目遥测',
        admin: '租户与密钥',
        audit: '请求审计',
        logs: '网关日志分析'
      },
      en: {
        dashboard: 'Orchestrator Cockpit',
        providers: 'API Channels',
        agents: 'Coding Clients',
        chat: 'Sandbox Testing',
        telemetry: 'Project Telemetry',
        admin: 'Tenants & Keys',
        audit: 'Request Audit',
        logs: 'Gateway Log Analysis'
      }
    } as const;
    return titles[lang][activeTab as keyof typeof titles['en']] || titles[lang].dashboard;
  };

  useEffect(() => {
    localStorage.setItem('ah_lang', lang);
  }, [lang]);

  useEffect(() => {
    localStorage.setItem('ah_theme', theme);
    const root = window.document.documentElement;
    if (theme === 'light') {
      root.classList.add('light');
      root.classList.remove('dark');
    } else {
      root.classList.add('dark');
      root.classList.remove('light');
    }
  }, [theme]);

  const [providers, setProviders] = useState<Provider[]>([]);
  const [models, setModels] = useState<Model[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [apiConnected, setApiConnected] = useState<boolean>(true);
  const [gateway, setGateway] = useState<GatewayState>({
    status: 'stopped',
    uptimeSeconds: 0,
    cpuUsage: 0,
    memoryUsage: 0,
    port: 3900
  });

  // Fetch current backend database state
  const fetchState = async () => {
    try {
      const data = await api.getState();
      const healthByProvider = new Map((data.providerHealth || []).map((health) => [health.providerId, health]));
      setProviders((data.providers || []).map((provider) => ({
        ...provider,
        health: healthByProvider.get(provider.id),
      })));
      setModels(data.models || []);
      setAgents(data.agents || []);
      setMcpServers((data.mcpServers || []) as McpServer[]);
      setGateway(data.gateway);
      setLogs(data.logs || []);
      setApiConnected(true);
    } catch (err) {
      // Gracefully handle transient network states during backend dev-server boots
      setApiConnected(false);
    }
  };

  // Poll backend database state periodically to show real-time changes
  useEffect(() => {
    fetchState();
    const interval = setInterval(fetchState, 1500);
    return () => clearInterval(interval);
  }, []);

  // Utility to push log to backend
  const addLog = async (level: 'info' | 'warn' | 'error' | 'debug', source: string, message: string) => {
    try {
      // Create artificial client log trigger or we can just let it render or send to server
      // Let's keep a local temporary log push if needed or trigger a server log
      // In this system all logs are loaded cleanly from backend. Let's just do a fetchState to read background logs
      await fetchState();
    } catch (e) {
      console.error(e);
    }
  };

  // Actions: Toggle Gateway on the backend
  const handleToggleGateway = async () => {
    try {
      const res = await apiRequest('/api/gateway/toggle', { method: 'POST' });
      if (res.ok) {
        fetchState();
      }
    } catch (err) {
      console.error(err);
    }
  };

  // Actions: Restart Gateway sequence on the backend
  const handleRestartGateway = async () => {
    try {
      const res = await apiRequest('/api/gateway/restart', { method: 'POST' });
      if (res.ok) {
        fetchState();
      }
    } catch (err) {
      console.error(err);
    }
  };

  // Actions: Trigger local diagnostic task sequence
  const handleTriggerQuickTask = async () => {
    try {
      await apiRequest('/api/gateway/quick-task', { method: 'POST' });
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  // Actions: Ping all configured connection providers
  const handleTestProviders = async () => {
    try {
      await apiRequest('/api/providers/test-all', { method: 'POST' });
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  // Actions: Reset databases to factory presets
  const handleResetDatabase = async () => {
    if (confirm('Revert all configured workspace profiles, MCP bindings, and custom providers back to defaults?')) {
      try {
        await apiRequest('/api/database/reset', { method: 'POST' });
        fetchState();
      } catch (err) {
        console.error(err);
      }
    }
  };

  // Target Agents Methods
  const handleUpdateAgent = async (agentId: string, updates: Partial<Agent>) => {
    try {
      await api.put(`/api/agents/${agentId}`, updates);
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleTriggerAgentInvocation = async (agentId: string) => {
    try {
      const result = await api.post(`/api/agents/${agentId}/invoke`) as {
        success?: boolean;
        invocation?: {
          response?: string;
          stderr?: string;
          stdout?: string;
          exitCode?: number;
          isError?: boolean;
          durationMs?: number;
        };
      };
      fetchState();
      const invocation = result.invocation;
      if (result.success && invocation) {
        alert(
          invocation.response ||
          invocation.stdout ||
          (lang === 'zh' ? 'Agent 真实进程已执行完成。' : 'Agent process completed.')
        );
      } else if (invocation) {
        alert(invocation.response || invocation.stderr || `${lang === 'zh' ? 'Agent 进程失败' : 'Agent process failed'}: ${invocation.exitCode}`);
      }
    } catch (err) {
      console.error(err);
      alert(err instanceof Error ? err.message : String(err));
    }
  };

  // Providers handler profiles
  const handleAddProvider = async (newProv: Omit<Provider, 'status' | 'latency'>, initialModelsList?: Model[]) => {
    try {
      await api.post('/api/providers', newProv);
      if (initialModelsList && initialModelsList.length > 0) {
        for (const m of initialModelsList) {
          await api.post('/api/models', m);
        }
      }
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleUpdateProvider = async (provId: string, updates: Partial<Provider>) => {
    try {
      await api.put(`/api/providers/${provId}`, updates);
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleDeleteProvider = async (provId: string) => {
    try {
      await api.delete(`/api/providers/${provId}`);
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleTestConnection = async (provId: string) => {
    try {
      await api.post(`/api/providers/${provId}/test`);
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleSwitchProvider = async (provId: string) => {
    try {
      await api.post(`/api/providers/${provId}/switch`);
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleReorderProviders = async (currentIndex: number, direction: 'up' | 'down') => {
    try {
      await api.post('/api/providers/reorder', { currentIndex, direction, lang });
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleClearLogs = async () => {
    try {
      await api.post('/api/logs/clear');
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleAddModel = async (m: Model) => {
    try {
      await api.post('/api/models', m);
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleUpdateModel = async (modelId: string, updates: Partial<Model>) => {
    try {
      await api.put(`/api/models/${modelId}`, updates);
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleDeleteModel = async (modelId: string) => {
    try {
      await api.delete(`/api/models/${modelId}`);
      fetchState();
    } catch (err) {
      console.error(err);
    }
  };

  const handleSyncProviderModels = async (providerId: string) => {
    try {
      await api.syncProviderModels(providerId);
      await fetchState();
    } catch (err) {
      console.error(err);
      alert(
        lang === 'zh'
          ? `获取模型列表失败：${err instanceof Error ? err.message : String(err)}`
          : `Failed to fetch model list: ${err instanceof Error ? err.message : String(err)}`
      );
      throw err;
    }
  };

  return (
    <div id="agenthub-local-workspace" className="flex flex-col md:flex-row min-h-screen md:h-screen bg-[#090D16] text-[#F1F5F9] font-sans overflow-y-auto md:overflow-hidden">
      
      {/* Primary Left Navigation Area */}
      <Sidebar 
        activeTab={activeTab} 
        setActiveTab={setActiveTab} 
        gateway={gateway}
        onToggleGateway={handleToggleGateway}
        onRestartGateway={handleRestartGateway}
        agentsCount={agents.length}
        lang={lang}
        setLang={setLang}
        t={t}
        theme={theme}
        setTheme={setTheme}
      />

      {/* Main Orchestration workspace area */}
      <main id="main-content-window" className="w-full flex-1 flex flex-col min-w-0 bg-[#090D16] min-h-[640px] md:h-full md:min-h-0">
        {/* Top bar header with high-contrast sleek modern cockpit references */}
        <header className="min-h-14 md:h-14 border-b border-[#1E293B] px-4 md:px-6 py-3 md:py-0 flex flex-wrap sm:flex-nowrap items-center justify-between gap-3 shrink-0 bg-[#0D1321]/80 backdrop-blur">
          <div className="flex min-w-0 flex-1 items-center gap-2 text-[11px] font-mono leading-tight tracking-tight text-slate-400 select-none">
            <span className="shrink-0">{t('station')}</span>
            <span className="text-slate-600">/</span>
            <span className="min-w-0 text-[11px] sm:text-xs font-bold text-slate-100 break-words">
              {activeTabTitle()}
            </span>
          </div>

          <div className="flex items-center gap-2 md:gap-4 shrink-0">
            {/* Quick Theme Toggle Button */}
            <button
              id="header-theme-toggle-btn"
              onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
              className="p-1.5 bg-[#131B2E] border border-[#1E293B] hover:bg-[#1E293B] hover:text-slate-100 rounded-lg transition-all cursor-pointer flex items-center justify-center shadow-sm"
              title={lang === 'zh' ? '切换外观主题' : 'Toggle theme'}
            >
              {theme === 'dark' ? (
                <Sun className="h-3.5 w-3.5 text-amber-400" />
              ) : (
                <Moon className="h-3.5 w-3.5 text-indigo-400" />
              )}
            </button>
            <div className="h-3 w-px bg-[#1E293B]" />

            <span className="text-[10px] text-slate-400 font-mono hidden md:inline font-semibold">
              UPTIME: <span className="text-indigo-400">{gateway.uptimeSeconds}s</span>
            </span>
            <div className="h-3 w-px bg-[#1E293B]" />
            {apiConnected ? (
              <span className="text-[10px] bg-emerald-500/10 text-emerald-400 font-mono font-bold border border-emerald-500/30 px-2 py-0.5 rounded-md">
                ● REALTIME
              </span>
            ) : (
              <span className="text-[10px] bg-amber-500/10 text-amber-400 font-mono font-bold border border-amber-500/30 px-2 py-0.5 rounded-md animate-pulse">
                ● DAEMON RECONNECTING
              </span>
            )}
          </div>
        </header>

        {/* Dynamic Inner Tab Viewport */}
        <div className="flex-1 overflow-y-auto p-4 md:p-6 relative bg-[#090D16]">
          
          {/* Main conditional module components */}
          {activeTab === 'dashboard' && (
            <Dashboard
              agents={agents}
              providers={providers}
              models={models}
              gateway={gateway}
              onToggleGateway={handleToggleGateway}
              onRestartGateway={handleRestartGateway}
              onTestProviders={handleTestProviders}
              onResetDatabase={handleResetDatabase}
              t={t}
              lang={lang}
              setActiveTab={setActiveTab}
              onUpdateProvider={handleUpdateProvider}
              onReorderProviders={handleReorderProviders}
              onAddProvider={handleAddProvider}
              onDeleteProvider={handleDeleteProvider}
              onTestConnection={handleTestConnection}
            />
          )}

          {activeTab === 'agents' && (
            <AgentsView
              agents={agents}
              providers={providers}
              models={models}
              onUpdateAgent={handleUpdateAgent}
              onTriggerAgentInvocation={handleTriggerAgentInvocation}
              t={t}
              lang={lang}
            />
          )}

          {activeTab === 'providers' && (
            <ProvidersView
              providers={providers}
              onAddProvider={handleAddProvider}
              onUpdateProvider={handleUpdateProvider}
              onDeleteProvider={handleDeleteProvider}
              onTestConnection={handleTestConnection}
              t={t}
              lang={lang}
              models={models}
              onAddModel={handleAddModel}
              onUpdateModel={handleUpdateModel}
              onDeleteModel={handleDeleteModel}
              onSyncProviderModels={handleSyncProviderModels}
              onSwitchProvider={handleSwitchProvider}
            />
          )}

          {activeTab === 'chat' && (
            <ChatView
              providers={providers}
              models={models}
              lang={lang}
              t={t}
              onAddLog={addLog}
            />
          )}

          {activeTab === 'telemetry' && (
            <TelemetryView lang={lang} />
          )}

          {activeTab === 'admin' && (
            <AdminGatewayView lang={lang} />
          )}

          {activeTab === 'audit' && (
            <RequestAuditView lang={lang} />
          )}

          {activeTab === 'logs' && (
            <LogsView
              logs={logs}
              onClearLogs={handleClearLogs}
              onAddLog={addLog}
              t={t}
              lang={lang}
            />
          )}

        </div>
      </main>

    </div>
  );
}
