import { 
  LayoutDashboard, 
  Bot, 
  Server, 
  Terminal, 
  Power, 
  RefreshCw,
  Cpu,
  Globe,
  MessageSquare,
  Sun,
  Moon,
  BarChart3,
  FileSearch,
  KeyRound
} from 'lucide-react';
import { GatewayState } from '../types';
import { Locale } from '../locales';

interface SidebarProps {
  activeTab: string;
  setActiveTab: (tab: string) => void;
  gateway: GatewayState;
  onToggleGateway: () => void;
  onRestartGateway: () => void;
  agentsCount: number;
  lang: Locale;
  setLang: (lang: Locale) => void;
  t: (key: any) => string;
  theme: 'light' | 'dark';
  setTheme: (theme: 'light' | 'dark') => void;
}

export default function Sidebar({
  activeTab,
  setActiveTab,
  gateway,
  onToggleGateway,
  onRestartGateway,
  agentsCount,
  lang,
  setLang,
  t,
  theme,
  setTheme
}: SidebarProps) {
  const menuItems = [
    { id: 'dashboard', label: lang === 'zh' ? '主控制中心' : 'Orchestrator Cockpit', icon: LayoutDashboard },
    { id: 'providers', label: lang === 'zh' ? 'AI 通道管理' : 'API Channels', icon: Server },
    { id: 'agents', label: lang === 'zh' ? 'AI 客户端代理' : 'Coding Clients', icon: Bot, count: agentsCount },
    { id: 'chat', label: lang === 'zh' ? '沙盒连通测试' : 'Sandbox Testing', icon: MessageSquare },
    { id: 'telemetry', label: lang === 'zh' ? '项目遥测' : 'Project Telemetry', icon: BarChart3 },
    { id: 'admin', label: lang === 'zh' ? '租户与密钥' : 'Tenants & Keys', icon: KeyRound },
    { id: 'audit', label: lang === 'zh' ? '请求审计' : 'Request Audit', icon: FileSearch },
    { id: 'logs', label: lang === 'zh' ? '网关日志分析' : 'Gateway Log Analysis', icon: Terminal }
  ];

  return (
    <aside id="sidebar-container" className="w-full md:w-68 bg-[#0D1321] border-b md:border-b-0 md:border-r border-[#1E293B] flex flex-col h-auto md:h-full select-none shrink-0 font-sans">
      {/* Product Title */}
      <div id="sidebar-header" className="p-5 border-b border-[#1E293B] space-y-3.5 bg-[#0F172A]/40">
        <div className="flex items-center gap-3">
          <div className="p-2.5 bg-indigo-600/10 text-indigo-400 border border-indigo-500/20 rounded-lg shadow-sm">
            <Cpu className="h-5 w-5" />
          </div>
          <div>
            <h1 className="text-xs font-bold tracking-wider text-slate-100 uppercase font-sans">
              codexswitch
            </h1>
          </div>
        </div>

        {/* Global Control Bar for Lang & Theme */}
        <div className="space-y-2">
          {/* Global Language Switcher */}
          <div className="flex items-center gap-2 p-1 bg-[#131B2E] border border-[#1E293B] rounded-lg">
            <Globe className="h-3.5 w-3.5 text-slate-400 pl-1" />
            <div className="flex-1 grid grid-cols-2 gap-1 text-[9px] font-mono leading-none">
              <button
                id="lang-toggle-en"
                type="button"
                onClick={() => setLang('en')}
                className={`py-1 text-center font-bold rounded transition-all cursor-pointer ${
                  lang === 'en'
                    ? 'bg-indigo-600 text-white'
                    : 'text-slate-400 hover:text-slate-200'
                }`}
              >
                EN
              </button>
              <button
                id="lang-toggle-zh"
                type="button"
                onClick={() => setLang('zh')}
                className={`py-1 text-center font-bold rounded transition-all cursor-pointer ${
                  lang === 'zh'
                    ? 'bg-indigo-600 text-white'
                    : 'text-slate-400 hover:text-slate-200'
                }`}
              >
                ZH
              </button>
            </div>
          </div>

          {/* Global Theme Switcher */}
          <div className="flex items-center justify-between p-1 bg-[#131B2E] border border-[#1E293B] rounded-lg">
            <div className="flex items-center gap-1.5 pl-1.5">
              {theme === 'dark' ? (
                <Moon className="h-3.5 w-3.5 text-indigo-400" />
              ) : (
                <Sun className="h-3.5 w-3.5 text-amber-500" />
              )}
              <span className="text-[9px] font-mono font-bold text-slate-450 uppercase">
                {lang === 'zh' ? '外观主题' : 'THEME'}
              </span>
            </div>
            <div className="w-24 grid grid-cols-2 gap-0.5 text-[9px] font-mono leading-none">
              <button
                id="theme-toggle-light"
                type="button"
                onClick={() => setTheme('light')}
                className={`py-1 text-center font-bold rounded transition-all cursor-pointer ${
                  theme === 'light'
                    ? 'bg-indigo-600 text-white'
                    : 'text-slate-400 hover:text-slate-200'
                }`}
              >
                {lang === 'zh' ? '浅色' : 'LIGHT'}
              </button>
              <button
                id="theme-toggle-dark"
                type="button"
                onClick={() => setTheme('dark')}
                className={`py-1 text-center font-bold rounded transition-all cursor-pointer ${
                  theme === 'dark'
                    ? 'bg-indigo-600 text-white'
                    : 'text-slate-400 hover:text-slate-200'
                }`}
              >
                {lang === 'zh' ? '深色' : 'DARK'}
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Primary Navigation */}
      <nav id="sidebar-navigation" className="grid grid-cols-2 sm:grid-cols-3 md:block md:flex-1 px-3 py-4 gap-1.5 md:space-y-1.5 overflow-visible md:overflow-y-auto">
        {menuItems.map((item) => {
          const IconComponent = item.icon;
          const isActive = activeTab === item.id;
          return (
            <button
              id={`nav-item-${item.id}`}
              key={item.id}
              onClick={() => setActiveTab(item.id)}
              className={`w-full flex items-center justify-between px-3.5 py-2.5 text-xs font-semibold rounded-lg transition-all cursor-pointer ${
                isActive
                  ? 'bg-indigo-600 text-white shadow-sm'
                  : 'text-slate-350 hover:text-slate-100 hover:bg-slate-800/40'
              }`}
            >
              <div className="flex items-center gap-2.5">
                <IconComponent className="h-4.5 w-4.5 shrink-0" />
                <span>{item.label}</span>
              </div>
              {item.count !== undefined && (
                <span className={`text-[9px] font-mono font-bold px-1.5 py-0.5 rounded ${
                  isActive ? 'bg-indigo-700 text-slate-100' : 'bg-slate-800 text-slate-300'
                }`}>
                  {item.count}
                </span>
              )}
            </button>
          );
        })}
      </nav>

      {/* Gateway Status Segment */}
      <div id="sidebar-gateway" className="p-4 bg-[#0F172A]/40 border-t border-[#1E293B]">
        <div className="p-3.5 bg-[#131B2E] border border-[#1E293B] rounded-xl shadow-sm">
          <div className="flex items-center justify-between mb-3 border-b border-[#1E293B] pb-2">
            <span className="text-[9px] text-slate-400 font-mono font-bold uppercase">
              {lang === 'zh' ? '本地网关服务' : 'DAEMON'}
            </span>
            <div className="flex items-center gap-1.5">
              <span className={`h-2 w-2 rounded-full ${gateway.status === 'running' ? 'bg-emerald-500 animate-pulse' : 'bg-rose-500'}`} />
              <span className="text-[9px] font-mono text-slate-300 font-bold uppercase">
                {gateway.status === 'running' ? 'ONLINE' : 'STOPPED'}
              </span>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2">
            <button
              id="btn-gate-toggle"
              onClick={onToggleGateway}
              className={`flex items-center justify-center gap-1.5 py-1.5 px-2 text-[10px] font-mono font-bold rounded-lg cursor-pointer transition-all ${
                gateway.status === 'running'
                  ? 'bg-rose-950/40 text-rose-300 border border-rose-800/40 hover:bg-rose-900/30'
                  : 'bg-emerald-950/40 text-emerald-300 border border-emerald-800/40 hover:bg-emerald-900/30'
              }`}
            >
              <Power className="h-3 w-3 shrink-0" />
              <span>{gateway.status === 'running' ? 'STOP' : 'START'}</span>
            </button>

            <button
              id="btn-gate-restart"
              onClick={onRestartGateway}
              disabled={gateway.status === 'stopped'}
              className="flex items-center justify-center gap-1.5 py-1.5 px-2 text-[10px] font-mono font-bold rounded-lg bg-slate-800 text-slate-205 border border-slate-700/60 cursor-pointer hover:bg-slate-700/60 disabled:opacity-40 disabled:cursor-not-allowed transition-all"
            >
              <RefreshCw className="h-3 w-3 shrink-0" />
              <span>RESTART</span>
            </button>
          </div>

          {gateway.status === 'running' && (
            <div className="mt-3 pt-2.5 border-t border-dashed border-[#1E293B] grid grid-cols-2 gap-y-1 text-[9px] text-slate-400 font-mono">
              <div>PORT: <span className="text-slate-200 font-bold">{gateway.port}</span></div>
              <div className="text-right">CPU: <span className="text-indigo-400 font-bold">{gateway.cpuUsage}%</span></div>
              <div className="truncate">UP: <span className="text-slate-200 font-bold">{gateway.uptimeSeconds}s</span></div>
              <div className="text-right">MEM: <span className="text-purple-400 font-bold">{gateway.memoryUsage}M</span></div>
            </div>
          )}
        </div>
      </div>
    </aside>
  );
}
