import React, { useState } from 'react';
import { 
  Bot, 
  Server, 
  Activity, 
  Sliders,
  Play,
  Plus,
  Trash2,
  Lock,
  Unlock,
  ToggleLeft,
  ToggleRight,
  ArrowUp,
  ArrowDown,
  ExternalLink,
  ShieldAlert,
  Zap,
  Power,
  Search
} from 'lucide-react';
import { Agent, Provider, Model, GatewayState } from '../types';
import { Locale } from '../locales';

interface DashboardProps {
  agents: Agent[];
  providers: Provider[];
  models: Model[];
  gateway: GatewayState;
  onToggleGateway: () => void;
  onRestartGateway: () => void;
  onTestProviders: () => void;
  onResetDatabase?: () => void;
  t: (key: any) => string;
  lang: Locale;
  setActiveTab?: (tab: string) => void;
  
  onUpdateProvider?: (provId: string, updates: Partial<Provider>) => void;
  onReorderProviders?: (index: number, direction: 'up' | 'down') => void;
  onAddProvider?: (newProv: Omit<Provider, 'status' | 'latency'>) => void;
  onDeleteProvider?: (provId: string) => void;
  onTestConnection?: (provId: string) => void;
}

export default function Dashboard({
  agents,
  providers,
  models,
  gateway,
  onToggleGateway,
  onRestartGateway,
  onTestProviders,
  onResetDatabase,
  t,
  lang,
  setActiveTab,
  onUpdateProvider,
  onReorderProviders,
  onAddProvider,
  onDeleteProvider,
  onTestConnection
}: DashboardProps) {
  
  const [keyMasks, setKeyMasks] = useState<Record<string, boolean>>({});
  const [selectedEditProvider, setSelectedEditProvider] = useState<Provider | null>(null);

  // Form states for adding a new provider inline
  const [showAddModal, setShowAddModal] = useState(false);
  const [newProvName, setNewProvName] = useState('');
  const [newProvType, setNewProvType] = useState<'openai' | 'anthropic' | 'gemini' | 'openrouter' | 'custom'>('custom');
  const [newProvUrl, setNewProvUrl] = useState('');
  const [newProvKey, setNewProvKey] = useState('');

  // Form states for advanced edit drawer
  const [advDesc, setAdvDesc] = useState('');
  const [advSkipTls, setAdvSkipTls] = useState(false);
  const [advLowQuality, setAdvLowQuality] = useState(false);
  const [advAutoBlacklist, setAdvAutoBlacklist] = useState(false);
  const [advCodexPassthrough, setAdvCodexPassthrough] = useState(false);
  const [advCodexToolsCompat, setAdvCodexToolsCompat] = useState(false);
  const [advNormalizeUserId, setAdvNormalizeUserId] = useState(false);
  const [advNormalizeChatRoles, setAdvNormalizeChatRoles] = useState(false);
  const [advReasoningStyle, setAdvReasoningStyle] = useState('none');
  const [advProxyUrl, setAdvProxyUrl] = useState('');
  const [advRoutePrefix, setAdvRoutePrefix] = useState('');
  const [advHeaders, setAdvHeaders] = useState<{ name: string; value: string }[]>([]);
  const [newHeaderName, setNewHeaderName] = useState('');
  const [newHeaderValue, setNewHeaderValue] = useState('');

  const [diagLogName, setDiagLogName] = useState<string | null>(null);

  const handleOpenEditDrawer = (p: Provider) => {
    setSelectedEditProvider(p);
    setAdvDesc(p.description || '');
    setAdvSkipTls(p.skipTls || false);
    setAdvLowQuality(p.lowQuality || false);
    setAdvAutoBlacklist(p.autoBlacklist || false);
    setAdvCodexPassthrough(p.codexPassthrough || false);
    setAdvCodexToolsCompat(p.codexToolsCompat || false);
    setAdvNormalizeUserId(p.normalizeUserId || false);
    setAdvNormalizeChatRoles(p.normalizeChatRoles || false);
    setAdvReasoningStyle(p.reasoningStyle || 'none');
    setAdvProxyUrl(p.proxyUrl || '');
    setAdvRoutePrefix(p.routePrefix || '');
    setAdvHeaders(p.customHeaders || []);
    setNewHeaderName('');
    setNewHeaderValue('');
  };

  const handleAddHeader = () => {
    if (!newHeaderName.trim()) return;
    setAdvHeaders(prev => [...prev, { name: newHeaderName.trim(), value: newHeaderValue.trim() }]);
    setNewHeaderName('');
    setNewHeaderValue('');
  };

  const handleRemoveHeader = (index: number) => {
    setAdvHeaders(prev => prev.filter((_, i) => i !== index));
  };

  const handleSaveAdvancedSettings = () => {
    if (!selectedEditProvider || !onUpdateProvider) return;
    
    onUpdateProvider(selectedEditProvider.id, {
      description: advDesc,
      skipTls: advSkipTls,
      lowQuality: advLowQuality,
      autoBlacklist: advAutoBlacklist,
      codexPassthrough: advCodexPassthrough,
      codexToolsCompat: advCodexToolsCompat,
      normalizeUserId: advNormalizeUserId,
      normalizeChatRoles: advNormalizeChatRoles,
      reasoningStyle: advReasoningStyle,
      proxyUrl: advProxyUrl,
      routePrefix: advRoutePrefix,
      customHeaders: advHeaders
    });

    setSelectedEditProvider(null);
  };

  const toggleKeyMask = (id: string) => {
    setKeyMasks(prev => ({
      ...prev,
      [id]: !prev[id]
    }));
  };

  const submitNewProvider = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newProvName.trim() || !onAddProvider) return;

    onAddProvider({
      id: `custom-p-${Date.now()}`,
      name: newProvName.trim(),
      type: newProvType,
      baseUrl: newProvUrl.trim() || 'https://api.openai.com/v1',
      apiKey: newProvKey.trim()
    });

    setNewProvName('');
    setNewProvUrl('');
    setNewProvKey('');
    setShowAddModal(false);
  };

  return (
    <div id="dashboard-tab-panel" className="space-y-6 font-sans select-none text-slate-100">
      
      {/* Hero Welcome banner */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 p-6 bg-gradient-to-r from-indigo-950/40 to-slate-900/40 border border-indigo-500/10 rounded-2xl">
        <div className="space-y-1">
          <h2 className="text-lg font-bold text-slate-100">{lang === 'zh' ? '大模型提供商集成中心' : 'LLM Providers Integration Workspace'}</h2>
          <p className="text-xs text-slate-400 max-w-2xl">
            {lang === 'zh' 
              ? '专门为 Codex、Claude 及第三方编程 Agent 设计的智能通讯网关。在此管理多提供商节点并在故障时自适应轮询。' 
              : 'Communications gateway fine-tuned for Codex, Claude & multi-provider coding agents. Manage backend API nodes and auto-failover prioritizations below.'}
          </p>
        </div>
        <div className="flex items-center gap-3 shrink-0">
          <div className="text-right hidden md:block">
            <span className="text-[10px] text-slate-400 block font-mono">STATUS DAEMON</span>
            <span className="text-xs font-semibold text-emerald-400 font-mono">● {gateway.status === 'running' ? 'GATEWAY RUNNING' : 'SYSTEM STOPPED'}</span>
          </div>
        </div>
      </div>

      {/* Main Channel Orchestration Box */}
      <div className="bg-[#0F172A] border border-[#1E293B] rounded-2xl shadow-sm overflow-hidden">
        
        {/* Section Header */}
        <div className="p-5 border-b border-[#1E293B] flex flex-col sm:flex-row sm:items-center justify-between gap-4 bg-[#131B2E]/40">
          <div className="space-y-1.5ClassName bg-transparent">
            <div className="flex items-center gap-2">
              <div className="p-1.5 bg-indigo-505/10 text-indigo-400 rounded-lg">
                <Server className="h-4 w-4" />
              </div>
              <h3 className="text-sm font-bold text-slate-100 font-sans">
                {lang === 'zh' ? '多商渠道轮询与健康状况检查' : 'MULTI-PROVIDER CHANNEL ROTATION & HEALTH POOL'}
              </h3>
            </div>
            <p className="text-[11px] text-slate-405 font-medium max-w-xl">
              {lang === 'zh' 
                ? '通过上下箭头调整网关轮询优先级。首位渠道若遇额度耗尽或超时，网关将自动降级选用下一位节点。' 
                : 'Prioritized polling sequence for coding assistants. If the first node triggers limit/timeout errors, traffic auto-rotates down the chain.'}
            </p>
          </div>

          {/* Action Tools mapped directly inside */}
          <div id="orchestration-toolbar" className="flex items-center gap-2">
            <button
              id="dash-add-channel-btn"
              onClick={() => setShowAddModal(true)}
              className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg transition-all cursor-pointer shadow-sm"
            >
              <Plus className="h-3.5 w-3.5" />
              <span>{lang === 'zh' ? '添加渠道' : 'Add Channel'}</span>
            </button>

            <button
              id="dash-test-latency-btn"
              onClick={onTestProviders}
              className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold bg-slate-800 hover:bg-slate-705 text-slate-200 border border-slate-700 rounded-lg transition-all cursor-pointer"
            >
              <Zap className="h-3.5 w-3.5 text-amber-400" />
              <span>{lang === 'zh' ? '整网连通测速' : 'Test Latencies'}</span>
            </button>
          </div>
        </div>

        {/* Priority Sequence Router List */}
        <div className="p-5 overflow-x-auto">
          {providers.length === 0 ? (
            <div className="text-center py-10 text-slate-500 text-xs italic">
              {lang === 'zh' ? '无配置好的云端渠道。请点击右上角按钮新建！' : 'No backend providers registered. Click the Add Channel button to construct your first node.'}
            </div>
          ) : (
            <div className="min-w-[760px] space-y-3">
              {providers.map((p, idx) => {
                const isKeyHidden = !keyMasks[p.id];
                const displayKey = isKeyHidden ? `${p.apiKey?.substring(0, 8)}••••••••` : p.apiKey;
                const circuitOpen = Boolean(p.health?.circuitOpenUntil && new Date(p.health.circuitOpenUntil).getTime() > Date.now());
                
                return (
                  <div 
                    id={`channel-priority-row-${p.id}`}
                    key={p.id}
                    className="flex items-center justify-between border border-[#1E293B] bg-[#121824] hover:bg-[#151C2C] p-3.5 rounded-xl transition-all shadow-sm"
                  >
                    {/* Column 1: Rotation prioritize and status indicators */}
                    <div className="flex items-center gap-3 shrink-0">
                      
                      {/* Priority adjusters */}
                      <div className="flex items-center gap-0.5">
                        <button
                          id={`btn-prio-up-${p.id}`}
                          onClick={() => onReorderProviders && onReorderProviders(idx, 'up')}
                          disabled={idx === 0}
                          className="p-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 disabled:opacity-20 disabled:hover:bg-slate-800 cursor-pointer transition-colors"
                          title="Raise dispatch safety order"
                        >
                          <ArrowUp className="h-3.5 w-3.5" />
                        </button>
                        <button
                          id={`btn-prio-down-${p.id}`}
                          onClick={() => onReorderProviders && onReorderProviders(idx, 'down')}
                          disabled={idx === providers.length - 1}
                          className="p-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 disabled:opacity-20 disabled:hover:bg-slate-800 cursor-pointer transition-colors"
                          title="Lower dispatch priority order"
                        >
                          <ArrowDown className="h-3.5 w-3.5" />
                        </button>
                      </div>

                      {/* Rank Index identifier */}
                      <span className="text-xs font-mono font-bold text-slate-400 bg-slate-800/40 border border-[#1E293B] px-2.5 py-1 rounded-md">
                        {idx + 1}
                      </span>

                      {/* Health color ring */}
                      <div className="text-[10px] font-mono font-semibold">
                        {circuitOpen ? (
                          <span className="text-rose-400 bg-rose-500/10 px-2 py-0.5 rounded border border-rose-505/20 font-bold text-[9.5px]">
                            CIRCUIT OPEN
                          </span>
                        ) : p.status === 'connected' ? (
                          <span className="text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-505/20 font-bold text-[9.5px]">
                            VERIFIED
                          </span>
                        ) : p.status === 'testing' ? (
                          <span className="text-amber-400 bg-amber-500/10 px-2 py-0.5 rounded border border-amber-505/20 animate-pulse font-bold text-[9.5px]">
                            TESTING
                          </span>
                        ) : (
                          <span className="text-rose-450 bg-rose-500/10 px-2 py-0.5 rounded border border-rose-505/20 font-bold text-[9.5px]">
                            PENDING
                          </span>
                        )}
                      </div>
                    </div>

                    {/* Column 2: Provider identification details */}
                    <div className="flex-1 px-4 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-bold text-slate-100 truncate block">
                          {p.name}
                        </span>
                        {p.lowQuality && (
                          <span className="text-[8.5px] bg-amber-500/15 text-amber-400 border border-amber-500/30 px-1 py-0.2 rounded font-mono font-bold">
                            LOW_RATE
                          </span>
                        )}
                        {p.skipTls && (
                          <span className="text-[8.5px] bg-sky-500/15 text-sky-450 border border-sky-500/30 px-1 py-0.2 rounded font-mono font-bold">
                            DEV_TLS
                          </span>
                        )}
                      </div>
                      <div className="flex items-center gap-1.5 mt-1">
                        <span className="text-[10px] text-slate-400 font-mono select-all truncate max-w-[220px]">
                          {p.baseUrl}
                        </span>
                        <a href={p.baseUrl} target="_blank" rel="noreferrer" className="text-slate-500 hover:text-slate-350" title="Inspect rest spec site">
                          <ExternalLink className="h-3 w-3" />
                        </a>
                      </div>
                    </div>

                    {/* Column 3: Communication framework and latency */}
                    <div className="flex items-center gap-4 shrink-0 px-3">
                      <div className="text-right">
                        <span className="text-[9px] font-mono font-bold text-slate-450 uppercase block tracking-wider">{p.type} protocol</span>
                        <div className="flex items-center gap-1.5 mt-1 justify-end">
                          <span className="text-[10px] font-mono text-slate-300 font-semibold">
                            RTT: <span className={p.latency !== '—' && p.latency !== "error" ? "text-emerald-400 font-bold" : "text-slate-400"}>{p.latency || '—'}</span>
                          </span>
                        </div>
                      </div>
                    </div>

                    {/* Column 4: Masked key and operational settings */}
                    <div className="flex items-center gap-3 shrink-0 pl-4 border-l border-slate-800 ml-4">
                      
                      {/* Token masked layout */}
                      <div className="flex items-center gap-1.5 bg-[#090D16] border border-[#1E293B] px-2 py-1 rounded-lg">
                        <span className="text-[9px] font-mono font-bold text-slate-400">KEY:</span>
                        <span className="text-[9.5px] font-mono text-slate-400">
                          {p.apiKey ? displayKey : (lang === 'zh' ? '未指定' : 'EMPTY')}
                        </span>
                        {p.apiKey && (
                          <button
                            id={`btn-toggle-mask-${p.id}`}
                            onClick={() => toggleKeyMask(p.id)}
                            className="ml-1 text-slate-500 hover:text-slate-300 cursor-pointer"
                          >
                            {isKeyHidden ? (
                              <span className="text-[9px] font-bold text-indigo-400 hover:underline px-0.5">[{lang === 'zh' ? '展开' : 'SHOW'}]</span>
                            ) : (
                              <span className="text-[9px] font-bold text-rose-400 hover:underline px-0.5">[{lang === 'zh' ? '隐藏' : 'MASK'}]</span>
                            )}
                          </button>
                        )}
                      </div>

                      {/* Connection Diagnostic logs popup trigger */}
                      <button
                        id={`btn-diag-${p.id}`}
                        onClick={() => setDiagLogName(diagLogName === p.id ? null : p.id)}
                        className="p-1.5 rounded-lg border border-[#1E293B] bg-slate-800 hover:bg-slate-700 text-slate-300 cursor-pointer transition-colors"
                        title="Show socket diagnostics"
                      >
                        <Activity className="h-4 w-4" />
                      </button>

                      {/* Advanced Settings Modals drawer */}
                      <button
                        id={`btn-advanced-edit-${p.id}`}
                        onClick={() => handleOpenEditDrawer(p)}
                        className="p-1.5 rounded-lg bg-indigo-600/10 hover:bg-indigo-600/20 text-indigo-400 border border-indigo-500/20 cursor-pointer transition-colors"
                        title="Edit Advanced Filters & Gateway Settings"
                      >
                        <Sliders className="h-4 w-4" />
                      </button>
                    </div>

                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Diagnostics subview panel popup */}
        {diagLogName && (
          <div className="mx-5 mb-5 p-4 bg-slate-950 text-emerald-400 rounded-xl border border-emerald-900/30 font-mono text-[10px] leading-relaxed relative">
            <button 
              onClick={() => setDiagLogName(null)}
              className="absolute right-2 top-2 text-slate-500 hover:text-slate-300 font-bold px-1.5 text-xs"
            >
              [{lang === 'zh' ? '关闭' : 'CLOSE'}]
            </button>
            <div className="text-slate-300 font-bold border-b border-slate-900 pb-1.5 mb-2 flex items-center gap-1.5">
              <span className="text-emerald-400">●</span>
              <span>{lang === 'zh' ? '本节点多通道连通性与底层套接字诊断:' : 'GATEWAY SOCKET CONSOLE DIAGNOSTICS:'}</span>
              <span className="text-indigo-400">{diagLogName}</span>
            </div>
            <div className="space-y-1">
              <div>
                {lang === 'zh' 
                  ? '[核心系统信息] 路由分发套接字已完成内存初始化，准备开始 TLS 通信握手。' 
                  : '[SYSTEM INFO] Core connection handshaking socket initialized. Attempting TLS loop.'}
              </div>
              <div>
                {lang === 'zh' 
                  ? `[报文发送] 正在向对应的后端接口服务发送探测荷载。跳过 TLS 验证: 否。` 
                  : `[TRANSMIT] Emitting request payload metrics to rest service. SkipTLS verification: false.`}
              </div>
              <div>
                {lang === 'zh' 
                  ? '[套接字响应] 端点通道连接成功。HTTP 状态码 200 验证通过，路由可达。' 
                  : '[RESPONSE] Endpoints connection established. Status Code 200 Handshake OK.'}
              </div>
              <div>
                {lang === 'zh' 
                  ? '[配置持久化] 已成功记录当前的往返延迟 benchmarks 作为默认轮询权重参考。' 
                  : '[PERSISTENCE] Storing dynamic latency benchmarks inside orchestrator failover state.'}
              </div>
            </div>
          </div>
        )}

      </div>

      {/* 5. Channel Advanced Parameters Edit Drawer Overlay Modal */}
      {selectedEditProvider && (
        <div id="advanced-drawer-overlay" className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
          <div className="w-full max-w-2xl bg-[#0F172A] border border-[#1E293B] rounded-2xl shadow-xl overflow-hidden max-h-[90vh] flex flex-col">
            
            {/* Header */}
            <div className="border-b border-[#1E293B] p-4 flex items-center justify-between bg-[#131B2E]/50">
              <div className="flex items-center gap-2">
                <Sliders className="h-4.5 w-4.5 text-indigo-405" />
                <span className="text-xs font-bold uppercase text-slate-200">
                  {lang === 'zh' ? `配置高级转发机制 / ${selectedEditProvider.name}` : `ADVANCED ROUTER PROPERTIES / ${selectedEditProvider.name}`}
                </span>
              </div>
              <button 
                onClick={() => setSelectedEditProvider(null)}
                className="text-slate-400 hover:text-slate-200 font-semibold text-xs py-1 px-1.5 rounded bg-slate-800"
              >
                {lang === 'zh' ? '[× 关闭]' : '[× CLOSE]'}
              </button>
            </div>

            {/* Scrollable Content Body */}
            <div className="flex-1 overflow-y-auto p-5 space-y-5 text-xs">
              
              {/* Optional Description */}
              <div>
                <label className="block font-bold text-slate-400 uppercase tracking-widest font-mono mb-1.5">
                  {lang === 'zh' ? '备用渠道描述重写 / 备注说明' : 'Channel Description Override'}
                </label>
                <input
                  type="text"
                  placeholder="e.g., DeepSeek fast deployment tunnel fallback node."
                  value={advDesc}
                  onChange={(e) => setAdvDesc(e.target.value)}
                  className="w-full border border-[#1E293B] p-2.5 bg-[#090D16] text-slate-100 placeholder:text-slate-600 rounded-lg text-xs focus:outline-none focus:border-indigo-500 font-sans"
                />
              </div>

              {/* Core switches container */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3.5 pb-2">
                
                {/* Skip TLS switch */}
                <div className="flex items-center justify-between p-3 bg-[#090D16] border border-[#1E293B] rounded-xl">
                  <div>
                    <span className="font-bold block uppercase font-mono text-[9px] text-slate-300">{lang === 'zh' ? '跳过证书验证 (Skip TLS)' : 'Skip TLS Verification'}</span>
                    <span className="text-[9.5px] text-slate-500 block mt-0.5">{lang === 'zh' ? '允许开发私有服务器和本地隧道不校验证书签名。' : 'Bypasses handshake warning flags for dev servers.'}</span>
                  </div>
                  <button onClick={() => setAdvSkipTls(!advSkipTls)} className="text-slate-300 cursor-pointer" type="button">
                    {advSkipTls ? <ToggleRight className="h-7 w-7 text-indigo-500" /> : <ToggleLeft className="h-7 w-7 text-slate-600" />}
                  </button>
                </div>

                {/* Low Quality switch */}
                <div className="flex items-center justify-between p-3 bg-[#090D16] border border-[#1E293B] rounded-xl">
                  <div>
                    <span className="font-bold block uppercase font-mono text-[9px] text-slate-300">{lang === 'zh' ? '标记为低速率/窄带宽节点' : 'Mark as Low-quality Node'}</span>
                    <span className="text-[9.5px] text-slate-500 block mt-0.5">{lang === 'zh' ? '更为保守地限制并发指标以防请求丢包。' : 'Handles usage parameters more limits conservatively.'}</span>
                  </div>
                  <button onClick={() => setAdvLowQuality(!advLowQuality)} className="text-slate-300 cursor-pointer" type="button">
                    {advLowQuality ? <ToggleRight className="h-7 w-7 text-indigo-500" /> : <ToggleLeft className="h-7 w-7 text-slate-600" />}
                  </button>
                </div>

                {/* Auto blacklist switch */}
                <div className="flex items-center justify-between p-3 bg-[#090D16] border border-[#1E293B] rounded-xl">
                  <div>
                    <span className="font-bold block uppercase font-mono text-[9px] text-slate-300">{lang === 'zh' ? '401/402 异常自动熔断' : 'Auto Disconnect on 401/402'}</span>
                    <span className="text-[9.5px] text-slate-500 block mt-0.5">{lang === 'zh' ? '发生额度超期或未鉴权错自动调低该通道权重。' : 'Bypasses node temporarily if credit is exhausted.'}</span>
                  </div>
                  <button onClick={() => setAdvAutoBlacklist(!advAutoBlacklist)} className="text-slate-300 cursor-pointer" type="button">
                    {advAutoBlacklist ? <ToggleRight className="h-7 w-7 text-indigo-500" /> : <ToggleLeft className="h-7 w-7 text-slate-600" />}
                  </button>
                </div>

                {/* Codex Passthrough switch */}
                <div className="flex items-center justify-between p-3 bg-[#090D16] border border-[#1E293B] rounded-xl">
                  <div>
                    <span className="font-bold block uppercase font-mono text-[9px] text-slate-300">{lang === 'zh' ? 'Codex 工具自动多向转译' : 'Codex Tools Auto Translation'}</span>
                    <span className="text-[9.5px] text-slate-500 block mt-0.5">{lang === 'zh' ? '针对不同厂商的 tools schema 进行多路分流格式自适应协商。' : 'Directly adapt tools structure to meet provider demands.'}</span>
                  </div>
                  <button onClick={() => setAdvCodexPassthrough(!advCodexPassthrough)} className="text-slate-300 cursor-pointer" type="button">
                    {advCodexPassthrough ? <ToggleRight className="h-7 w-7 text-indigo-500" /> : <ToggleLeft className="h-7 w-7 text-slate-600" />}
                  </button>
                </div>

              </div>

              {/* Reasoning parameter selection */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className="block font-bold text-slate-400 uppercase tracking-widest font-mono mb-1.5">
                    {lang === 'zh' ? '推理思维链属性适配 (Reasoning Effort)' : 'Reasoning properties compatibility'}
                  </label>
                  <select
                    value={advReasoningStyle}
                    onChange={(e) => setAdvReasoningStyle(e.target.value)}
                    className="w-full border border-[#1E293B] p-2.5 bg-[#090D16] text-slate-205 font-mono text-xs rounded-lg focus:outline-none focus:border-indigo-500"
                  >
                    <option value="none">{lang === 'zh' ? '无 (不发送推理激活标志)' : 'none (Do not emit reasoning parameters)'}</option>
                    <option value="reasoning.effort">{lang === 'zh' ? 'reasoning.effort (标准 o1/o3 控制开关)' : 'reasoning.effort (Standard o3/o1 format)'}</option>
                    <option value="detailed">{lang === 'zh' ? 'detailed (DeepSeek 专有推理思维开关)' : 'detailed (DeepSeek reasoning formatting)'}</option>
                  </select>
                </div>

                <div>
                  <label className="block font-bold text-slate-400 uppercase tracking-widest font-mono mb-1.5">
                    {lang === 'zh' ? '覆写内部路由端点后缀 / 动作前缀' : 'Host Action Prefix Overwrite'}
                  </label>
                  <input
                    type="text"
                    placeholder="e.g., /v1/chat/completions"
                    value={advRoutePrefix}
                    onChange={(e) => setAdvRoutePrefix(e.target.value)}
                    className="w-full border border-[#1E293B] p-2.5 bg-[#090D16] text-slate-205 font-mono text-xs rounded-lg focus:outline-none focus:border-indigo-500"
                  />
                </div>
              </div>

              {/* Custom HTTP Headers Builder */}
              <div className="border border-[#1E293B] p-4 bg-[#090D16] rounded-xl space-y-3">
                <span className="font-bold uppercase font-mono block text-slate-300 text-[10px] tracking-wider">
                  {lang === 'zh' ? '已配置的额外附加自定义网关 HTTP 头' : 'Custom Transmit Headers Mapped'}
                </span>
                
                {/* Active headers roster */}
                {advHeaders.length === 0 ? (
                  <p className="text-[10px] text-slate-500 font-mono italic">{lang === 'zh' ? '没有为当前管道加载的自定义 HTTP 请求头。将仅生成标准代理头部。' : 'No custom network headers mapped onto client pipelines. Standard headers are used.'}</p>
                ) : (
                  <div className="space-y-1.5 max-h-24 overflow-y-auto pr-1">
                    {advHeaders.map((hdr, hidx) => (
                      <div key={hidx} className="flex items-center justify-between bg-[#121824] border border-[#1E293B] p-2 rounded-lg font-mono text-[9.5px]">
                        <span className="font-semibold text-slate-300">{hdr.name}: <span className="text-slate-500 select-all">{hdr.value}</span></span>
                        <button
                          type="button"
                          onClick={() => handleRemoveHeader(hidx)}
                          className="text-rose-400 hover:text-rose-350 font-bold px-1"
                        >
                          [{lang === 'zh' ? '移除' : 'REMOVE'}]
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                {/* Input builders */}
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    placeholder={lang === 'zh' ? "自定义请求头名字" : "Header Name"}
                    value={newHeaderName}
                    onChange={(e) => setNewHeaderName(e.target.value)}
                    className="flex-1 border border-[#1E293B] p-2 bg-[#121824] rounded-lg text-xs focus:outline-none font-mono"
                  />
                  <input
                    type="text"
                    placeholder={lang === 'zh' ? "自定义请求头数值" : "Header Value"}
                    value={newHeaderValue}
                    onChange={(e) => setNewHeaderValue(e.target.value)}
                    className="flex-1 border border-[#1E293B] p-2 bg-[#121824] rounded-lg text-xs focus:outline-none font-mono"
                  />
                  <button
                    type="button"
                    onClick={handleAddHeader}
                    className="px-3.5 py-2 bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg font-semibold text-xs transition-colors cursor-pointer"
                  >
                    {lang === 'zh' ? '+ 绑定' : '+ Add'}
                  </button>
                </div>
              </div>

            </div>

            {/* Footer Panel */}
            <div className="border-t border-[#1E293B] p-4 flex justify-end gap-3 shrink-0 bg-[#131B2E]/50">
              <button
                type="button"
                onClick={() => setSelectedEditProvider(null)}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-705 border border-[#1E293B] text-slate-300 rounded-lg font-semibold cursor-pointer"
              >
                {t('cancel')}
              </button>
              <button
                type="button"
                onClick={handleSaveAdvancedSettings}
                className="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white font-semibold rounded-lg cursor-pointer"
              >
                {lang === 'zh' ? '应用高级修改' : 'Apply Changes'}
              </button>
            </div>

          </div>
        </div>
      )}

      {/* 6. Add Channel Inline Modal Form */}
      {showAddModal && (
        <div id="add-channel-modal-overlay" className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
          <form 
            onSubmit={submitNewProvider}
            className="w-full max-w-md bg-[#0F172A] border border-[#1E293B] rounded-2xl shadow-xl overflow-hidden"
          >
            <div className="border-b border-[#1E293B] p-4 flex items-center justify-between bg-[#131B2E]/50">
              <span className="text-xs font-bold text-slate-205 uppercase">
                {lang === 'zh' ? '开启新的集成通道' : 'INTEGRATE NEW COMMUNICATION CHANNEL'}
              </span>
              <button 
                type="button"
                onClick={() => setShowAddModal(false)}
                className="text-slate-400 hover:text-slate-200 font-semibold text-xs py-1 px-1.5 rounded bg-slate-800"
              >
                {lang === 'zh' ? '[× 关闭]' : '[× CLOSE]'}
              </button>
            </div>

            <div className="p-5 space-y-4 text-xs">
              <div>
                <label className="block font-bold text-slate-405 uppercase tracking-wider font-mono mb-1.5">
                  {lang === 'zh' ? '渠道别名备注' : 'Channel Endpoint Nickname'}
                </label>
                <input
                  required
                  type="text"
                  placeholder={lang === 'zh' ? "例如: 幻方 DeepSeek 国内专线" : "e.g., DeepSeek Local Tunnel"}
                  value={newProvName}
                  onChange={(e) => setNewProvName(e.target.value)}
                  className="w-full border border-[#1E293B] p-2.5 bg-[#090D16] text-slate-100 rounded-lg text-xs focus:outline-none focus:border-indigo-505"
                />
              </div>

              <div>
                <label className="block font-bold text-slate-405 uppercase tracking-wider font-mono mb-1.5">
                  {lang === 'zh' ? '网关对接协议格式' : 'Protocol Specifications Alignment'}
                </label>
                <select
                  value={newProvType}
                  onChange={(e) => setNewProvType(e.target.value as any)}
                  className="w-full border border-[#1E293B] p-2.5 bg-[#090D16] text-slate-205 rounded-lg text-xs font-mono focus:outline-none focus:border-indigo-505"
                >
                  <option value="openai">openai ({lang === 'zh' ? 'OpenAI 官方通用格式规范' : 'OpenAI standard rules'})</option>
                  <option value="anthropic">anthropic ({lang === 'zh' ? 'Anthropic Messages 分流排版' : 'Claude message adaptation'})</option>
                  <option value="gemini">gemini ({lang === 'zh' ? '谷歌开发者 REST 协议对接' : 'Google developer REST protocol'})</option>
                  <option value="openrouter">openrouter ({lang === 'zh' ? 'OpenRouter 直连转发机制' : 'OpenRouter endpoints'})</option>
                  <option value="custom">custom ({lang === 'zh' ? '自建自定义 OpenAI 兼容接口规范' : 'Custom OpenAI compatible specifications'})</option>
                </select>
              </div>

              <div>
                <label className="block font-bold text-slate-405 uppercase tracking-wider font-mono mb-1.5">
                  {lang === 'zh' ? '服务请求底层 Base URL' : 'Endpoint Base URL'}
                </label>
                <input
                  type="url"
                  placeholder="e.g., https://api.deepseek.com/v1"
                  value={newProvUrl}
                  onChange={(e) => setNewProvUrl(e.target.value)}
                  className="w-full border border-[#1E293B] p-2.5 bg-[#090D16] text-slate-100 rounded-lg text-xs font-mono focus:outline-none focus:border-indigo-505"
                />
              </div>

              <div>
                <label className="block font-bold text-slate-405 uppercase tracking-wider font-mono mb-1.5">
                  {lang === 'zh' ? '鉴权秘钥 API Key Token' : 'Authentication Token / API Key'}
                </label>
                <input
                  type="password"
                  placeholder="sk-••••••••••••"
                  value={newProvKey}
                  onChange={(e) => setNewProvKey(e.target.value)}
                  className="w-full border border-[#1E293B] p-2.5 bg-[#090D16] text-slate-100 rounded-lg text-xs font-mono focus:outline-none focus:border-indigo-505"
                />
              </div>
            </div>

            <div className="border-t border-[#1E293B] p-4 flex justify-end gap-3 bg-[#131B2E]/50">
              <button
                type="button"
                onClick={() => setShowAddModal(false)}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-705 border border-[#1E293B] text-slate-350 rounded-lg font-semibold cursor-pointer"
              >
                {t('cancel')}
              </button>
              <button
                type="submit"
                className="px-4 py-2 bg-indigo-600 hover:bg-indigo-550 text-white rounded-lg font-semibold cursor-pointer"
              >
                {lang === 'zh' ? '添加并上线此渠道' : 'Register & Enable channel'}
              </button>
            </div>
          </form>
        </div>
      )}

    </div>
  );
}
