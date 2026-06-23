import React, { useState } from 'react';
import { 
  Bot, 
  Settings2, 
  Activity, 
  TrendingUp, 
  Cpu, 
  HelpCircle, 
  Sparkles, 
  CheckCircle2, 
  AlertCircle,
  Play,
  Square,
  Layers,
  Sliders,
  ShieldCheck,
  Binary,
  Compass
} from 'lucide-react';
import { Agent, Provider, Model } from '../types';
import { Locale } from '../locales';

interface AgentsViewProps {
  agents: Agent[];
  providers: Provider[];
  models: Model[];
  onUpdateAgent: (agentId: string, updates: Partial<Agent>) => void;
  onTriggerAgentInvocation: (agentId: string) => void;
  t: (key: any) => string;
  lang: Locale;
}

export default function AgentsView({
  agents,
  providers,
  models,
  onUpdateAgent,
  onTriggerAgentInvocation,
  t,
  lang
}: AgentsViewProps) {
  const [selectedAgentId, setSelectedAgentId] = useState<string>(agents[0]?.id || '');
  const [showTooltip, setShowTooltip] = useState<boolean>(false);

  const activeAgent = agents.find(a => a.id === selectedAgentId) || agents[0];

  // Get models available for the selected agent's current provider
  const availableModelsForProvider = models.filter(
    m => m.providerId === activeAgent?.providerId
  );

  const handleProviderChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const nextProvId = e.target.value;
    const matchingModel = models.find(m => m.providerId === nextProvId);
    onUpdateAgent(activeAgent.id, {
      providerId: nextProvId,
      modelId: matchingModel ? matchingModel.id : ''
    });
  };

  const handleModelChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    onUpdateAgent(activeAgent.id, {
      modelId: e.target.value
    });
  };

  const toggleAgentStatus = () => {
    const nextStatus = activeAgent.status === 'running' ? 'idle' : 'running';
    onUpdateAgent(activeAgent.id, { status: nextStatus });
  };

  // Toggle provider in multi-provider selection list
  const handleToggleAllowedProvider = (pId: string) => {
    const currentList = activeAgent.allowedProviders || [];
    let updatedList: string[];
    if (currentList.includes(pId)) {
      if (currentList.length <= 1) return;
      updatedList = currentList.filter(id => id !== pId);
    } else {
      updatedList = [...currentList, pId];
    }
    onUpdateAgent(activeAgent.id, { allowedProviders: updatedList });
  };

  return (
    <div id="agents-tab-panel" className="grid grid-cols-1 lg:grid-cols-3 gap-6 h-full items-start text-slate-150 font-sans">
      
      {/* Left Column: List of Agents */}
      <div className="lg:col-span-1 space-y-4">
        <div className="flex items-center justify-between pb-3 border-b border-[#1E293B]">
          <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider font-mono">
            {lang === 'zh' ? '编码 Agent 宿主列表' : 'AGENT DAEMONS'}
          </h3>
          <span className="text-[10px] bg-slate-800 text-slate-300 px-2 py-0.5 rounded font-mono font-bold border border-[#1E293B]">
            {agents.length} {lang === 'zh' ? '个配置文件' : 'PROFILES'}
          </span>
        </div>

        <div className="space-y-3">
          {agents.map((agent) => {
            const isSelected = agent.id === selectedAgentId;
            const currentProvider = providers.find(p => p.id === agent.providerId);
            const activeAllowedCount = agent.allowedProviders?.length || 1;
            
            return (
              <div
                id={`agent-card-${agent.id}`}
                key={agent.id}
                onClick={() => setSelectedAgentId(agent.id)}
                className={`p-4 border rounded-xl cursor-pointer transition-all ${
                  isSelected
                    ? 'bg-indigo-605/10 border-indigo-500/40 text-slate-100 shadow-sm'
                    : 'bg-[#0F172A] border-[#1E293B] text-slate-350 hover:bg-[#131B2E] hover:border-slate-700'
                }`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2.5">
                    <div className="p-1 rounded bg-[#090D16] border border-[#1E293B]">
                      <Bot className="h-4.5 w-4.5 text-indigo-400" />
                    </div>
                    <span className="text-xs font-bold font-sans">{agent.name}</span>
                  </div>

                  {/* Status pills */}
                  <span className={`inline-flex items-center gap-1 text-[9.5px] font-mono font-bold px-2 py-0.5 rounded border ${
                    agent.status === 'running'
                      ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
                      : agent.status === 'error'
                      ? 'bg-rose-500/10 text-rose-400 border-rose-500/20'
                      : 'bg-[#090D16] text-slate-400 border-[#1E293B]'
                  }`}>
                    {agent.status === 'running' ? 'RUNNING' : agent.status.toUpperCase()}
                  </span>
                </div>

                <p className="text-[11px] text-slate-400 line-clamp-2 mt-2 leading-relaxed">
                  {agent.desc}
                </p>

                <div className="mt-3.5 pt-2 border-t border-dashed border-[#1E293B] flex items-center justify-between text-[10px] font-mono font-semibold text-slate-400 uppercase">
                  <span className="bg-[#090D16] border border-[#1E293B] px-1.5 py-0.5 text-[9px] rounded font-bold">
                    {activeAllowedCount} {lang === 'zh' ? '个渠道已集成' : 'CHANNELS POOLED'}
                  </span>
                  <span className="truncate max-w-[125px] text-right text-slate-400">{agent.modelId}</span>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Right Column: Expanded Workspace Control Panel */}
      <div className="lg:col-span-2 space-y-5">
        {activeAgent ? (
          <div className="p-6 bg-[#0F172A] border border-[#1E293B] rounded-2xl shadow-sm space-y-5">
            
            {/* Header section with status triggers */}
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-4 border-b border-[#1E293B]">
              <div className="space-y-1.5">
                <div className="flex items-center gap-2">
                  <span className="text-[10px] bg-indigo-605/10 text-indigo-400 border border-indigo-505/25 px-2 py-0.5 rounded font-mono font-bold uppercase">
                    {t('agentConfigCenter')}
                  </span>
                  {activeAgent.lastRun && (
                    <span className="text-[10px] text-slate-400 font-mono font-bold">{t('lastRun')}: {activeAgent.lastRun}</span>
                  )}
                </div>
                <h3 className="text-sm font-bold text-slate-100">{activeAgent.name} {t('station')}</h3>
              </div>

              {/* Status Action Button */}
              <div className="flex items-center gap-2 shrink-0">
                <button
                  id="btn-trigger-agent-state"
                  onClick={toggleAgentStatus}
                  className={`flex items-center gap-1.5 px-3 py-1.5 border rounded-lg text-xs font-semibold transition-all cursor-pointer ${
                    activeAgent.status === 'running'
                      ? 'bg-rose-950/30 text-rose-300 border-rose-800/40 hover:bg-rose-900/30'
                      : 'bg-slate-800 border-slate-700 hover:bg-slate-700 text-slate-200'
                  }`}
                >
                  {activeAgent.status === 'running' ? (
                    <>
                      <Square className="h-3.5 w-3.5 fill-rose-550 text-rose-450 animate-pulse" />
                      <span>{t('stopDaemon')}</span>
                    </>
                  ) : (
                    <>
                      <Play className="h-3.5 w-3.5 fill-emerald-550 text-emerald-450" />
                      <span>{t('startDaemon')}</span>
                    </>
                  )}
                </button>

                <button
                  id="btn-test-invoke-agent"
                  onClick={() => onTriggerAgentInvocation(activeAgent.id)}
                  className="flex items-center gap-1.5 bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs px-3.5 py-1.5 rounded-lg cursor-pointer transition-colors shadow-sm"
                >
                  <Sparkles className="h-3.5 w-3.5" />
                  <span>{t('invokeAgent')}</span>
                </button>
              </div>
            </div>

            {/* 1. Allowed Channel checklist with unified translation switch */}
            <div className="p-4.5 bg-[#090D16] border border-[#1E293B] rounded-xl space-y-4">
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div className="space-y-0.5">
                  <h4 className="text-xs font-bold text-slate-250 flex items-center gap-1.5 font-sans">
                    <ShieldCheck className="h-4.5 w-4.5 text-indigo-400" />
                    <span>{lang === 'zh' ? '多商协议自适应绑定' : 'Adaptive Multi-Provider Sync'}</span>
                  </h4>
                  <p className="text-[10.5px] text-slate-450">{lang === 'zh' ? '允许当前 Agent 调度哪些后端通道，网关在请求时自动对齐输入规范差异。' : 'Allowed backup LLM providers. Adapters automatically map prompt formats.'}</p>
                </div>
                
                {/* Adaptive translation toggler */}
                <div className="flex items-center gap-2 bg-[#121824] border border-[#1E293B] px-3 py-1.5 rounded-lg shrink-0">
                  <span className="text-[10px] font-bold text-slate-300 font-mono">{lang === 'zh' ? '格式转译' : 'Auto Format'}</span>
                  <input
                    id="auto-format-toggle"
                    type="checkbox"
                    checked={activeAgent.autoFormatConvert ?? true}
                    onChange={(e) => onUpdateAgent(activeAgent.id, { autoFormatConvert: e.target.checked })}
                    className="w-7 h-4 bg-slate-800 rounded-full appearance-none cursor-pointer relative checked:bg-indigo-600 before:content-[''] before:absolute before:w-3 before:h-3 before:bg-slate-500 before:rounded-full before:top-[1px] before:left-[1px] before:transition-all checked:before:translate-x-3 checked:before:bg-white border border-[#1E293B]"
                  />
                </div>
              </div>

              {/* Roster of multiple provider tags */}
              <div className="grid grid-cols-2 sm:grid-cols-3 gap-2.5 pt-2 border-t border-[#1E293B]">
                {providers.map((p) => {
                  const isChecked = (activeAgent.allowedProviders || []).includes(p.id) || p.id === activeAgent.providerId;
                  const isPrimary = p.id === activeAgent.providerId;
                  
                  return (
                    <button
                      id={`cb-provider-allowed-${p.id}`}
                      key={p.id}
                      type="button"
                      onClick={() => handleToggleAllowedProvider(p.id)}
                      disabled={isPrimary}
                      className={`flex items-center justify-between p-2.5 border rounded-lg text-left transition-all ${
                        isChecked 
                          ? 'bg-indigo-605/10 text-slate-100 border-indigo-450/40'
                          : 'bg-[#121824] text-slate-400 border-[#1E293B] hover:border-slate-700'
                      }`}
                    >
                      <div className="space-y-0.5 truncate pr-1">
                        <span className="text-[10.5px] font-bold font-mono block truncate">{p.name}</span>
                        <span className="text-[9px] text-[#818CF8] font-sans block">
                          {isPrimary ? (lang === 'zh' ? '🌟 首选主通道' : '🌟 Main Channel') : p.type.toUpperCase()}
                        </span>
                      </div>
                      <input
                        type="checkbox"
                        checked={isChecked}
                        readOnly
                        className="w-3.5 h-3.5 rounded bg-[#090D16] accent-indigo-500 cursor-pointer pointer-events-none border-[#1E293B]"
                      />
                    </button>
                  );
                })}
              </div>

              {/* Tips for format converting */}
              {activeAgent.autoFormatConvert !== false && (
                <div className="p-3 bg-indigo-500/5 border border-indigo-500/10 rounded-lg text-slate-400 flex gap-2.5 text-[10px] leading-relaxed">
                  <Binary className="h-4.5 w-4.5 text-indigo-400 flex-shrink-0 mt-0.5" />
                  <div>
                    <span className="text-indigo-450 font-bold">{lang === 'zh' ? '协议自适应：' : 'Unified Protocol active: '}</span>
                    {lang === 'zh' ? '智能网关将接收到的 system prompts、tools schema 语法自动转译为目标通道的兼容标准，确保全流程无缝穿梭。' 
                    : 'System automatically syncs system coding blocks and nested payload schemas into destination specifications, guaranteeing multi-brand portability.'}
                  </div>
                </div>
              )}
            </div>

            {/* 2. Main target channel routing */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div className="space-y-1.5 p-4.5 bg-[#090D16] border border-[#1E293B] rounded-xl">
                <label className="text-[10px] font-bold text-slate-400 uppercase tracking-widest block font-mono">{t('apiProvider')}</label>
                <select
                  id="agent-provider-select"
                  value={activeAgent.providerId}
                  onChange={handleProviderChange}
                  className="w-full bg-[#121824] text-slate-100 border border-[#1E293B] rounded-lg p-2 text-xs font-mono font-bold focus:outline-none"
                >
                  {providers.map(p => (
                    <option key={p.id} value={p.id}>
                      {p.name} ({p.status === 'connected' ? t('connected') : p.status.toUpperCase()})
                    </option>
                  ))}
                </select>
                <span className="text-[9.5px] text-slate-450 block">{t('apiProviderDesc')}</span>
              </div>

              <div className="space-y-1.5 p-4.5 bg-[#090D16] border border-[#1E293B] rounded-xl">
                <label className="text-[10px] font-bold text-slate-400 uppercase tracking-widest block font-mono">{t('targetModel')}</label>
                <select
                  id="agent-model-select"
                  value={activeAgent.modelId}
                  onChange={handleModelChange}
                  className="w-full bg-[#121824] text-slate-100 border border-[#1E293B] rounded-lg p-2 text-xs font-mono font-bold focus:outline-none"
                >
                  {availableModelsForProvider.length > 0 ? (
                    availableModelsForProvider.map(m => (
                      <option key={m.id} value={m.id}>
                        {m.name} ({m.contextWindow})
                      </option>
                    ))
                  ) : (
                    <option value="">{lang === 'zh' ? '该提供商无可用模型' : 'No model available for provider'}</option>
                  )}
                </select>
                <span className="text-[9.5px] text-slate-450 block">{t('targetModelDesc')}</span>
              </div>
            </div>

            {/* 3. Parameter limiting parameters boundaries */}
            <div className="p-4.5 bg-[#090D16] border border-[#1E293B] rounded-xl space-y-4">
              <h4 className="text-xs font-bold text-slate-200 flex items-center gap-1.5 font-sans">
                <Sliders className="h-4.5 w-4.5 text-indigo-400" />
                <span>{lang === 'zh' ? '运行约束指标限制' : 'Operational Limiting Parameters'}</span>
              </h4>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-4 pt-1">
                {/* Context boundaries */}
                <div className="space-y-2">
                  <div className="flex items-center justify-between font-mono text-[10.5px]">
                    <span className="text-slate-400 font-sans font-bold">{lang === 'zh' ? '单次最大上下文限制' : 'Context limit bounds'}</span>
                    <span className="text-indigo-400 font-extrabold font-mono">
                      {(activeAgent.contextLengthLimit ?? 128) >= 2048 ? '2M Unlimited' : `${activeAgent.contextLengthLimit ?? 128}K tokens`}
                    </span>
                  </div>
                  <input
                    id="context-limit-slider"
                    type="range"
                    min="8"
                    max="2048"
                    step="8"
                    value={activeAgent.contextLengthLimit ?? 128}
                    onChange={(e) => onUpdateAgent(activeAgent.id, { contextLengthLimit: parseInt(e.target.value) })}
                    className="w-full h-1 bg-slate-800 rounded appearance-none cursor-pointer accent-indigo-500"
                  />
                  <div className="flex justify-between text-[8px] text-slate-500 font-mono font-semibold">
                    <span>8K</span>
                    <span>{lang === 'zh' ? '128K 默认' : '128K DEFAULT'}</span>
                    <span>{lang === 'zh' ? '1M 旗舰配置' : '1M ULTRA'}</span>
                    <span>{lang === 'zh' ? '2M 最大限制' : '2M MAX'}</span>
                  </div>
                </div>

                {/* Output Boundaries */}
                <div className="space-y-2">
                  <div className="flex items-center justify-between font-mono text-[10.5px]">
                    <span className="text-slate-400 font-sans font-bold">{lang === 'zh' ? '最大吐字参数限制' : 'Max Outflow Limits'}</span>
                    <span className="text-indigo-400 font-extrabold font-mono">{(activeAgent.maxOutputLimit ?? 4096).toLocaleString()} tokens</span>
                  </div>
                  <input
                    id="output-limit-slider"
                    type="range"
                    min="1024"
                    max="16384"
                    step="1024"
                    value={activeAgent.maxOutputLimit ?? 4096}
                    onChange={(e) => onUpdateAgent(activeAgent.id, { maxOutputLimit: parseInt(e.target.value) })}
                    className="w-full h-1 bg-slate-800 rounded appearance-none cursor-pointer accent-indigo-500"
                  />
                  <div className="flex justify-between text-[8px] text-slate-500 font-mono font-semibold">
                    <span>1,024</span>
                    <span>{lang === 'zh' ? '4,096 标准' : '4,096 STANDARD'}</span>
                    <span>{lang === 'zh' ? '8,192 密集生成' : '8,192 HEAVY'}</span>
                    <span>{lang === 'zh' ? '16,384 最大' : '16,384 MAX'}</span>
                  </div>
                </div>

                {/* Temperature details */}
                <div className="space-y-2 md:col-span-2 pt-2 border-t border-[#1E293B]">
                  <div className="flex items-center justify-between font-mono text-[10.5px]">
                    <span className="text-slate-400 font-sans font-bold">{lang === 'zh' ? '确定度与创造温度限制' : 'Inference Temperature control'}</span>
                    <span className="text-indigo-400 font-extrabold font-mono">{(activeAgent.temperature ?? 0.7).toFixed(1)}</span>
                  </div>
                  <input
                    id="temperature-slider"
                    type="range"
                    min="0.0"
                    max="2.0"
                    step="0.1"
                    value={activeAgent.temperature ?? 0.7}
                    onChange={(e) => onUpdateAgent(activeAgent.id, { temperature: parseFloat(e.target.value) })}
                    className="w-full h-1 bg-slate-800 rounded appearance-none cursor-pointer accent-indigo-500"
                  />
                  <div className="flex justify-between text-[8px] text-slate-500 font-mono font-semibold">
                    <span>0.0 {lang === 'zh' ? '(精准/确定)' : '(DETERMINISTIC)'}</span>
                    <span>0.7 {lang === 'zh' ? '(代码生成建议)' : '(CODING SUGGEST)'}</span>
                    <span>1.2 {lang === 'zh' ? '(创造力/丰富)' : '(CREATIVE)'}</span>
                    <span>2.0 {lang === 'zh' ? '(随机/混沌)' : '(CHAOTIC)'}</span>
                  </div>
                </div>
              </div>
            </div>

            {/* 4. Live references checklist cabinet */}
            <div className="p-4.5 bg-[#090D16] border border-[#1E293B] rounded-xl space-y-3">
              <div className="flex items-center justify-between">
                <h4 className="text-[9.5px] uppercase font-bold tracking-widest text-slate-405 font-mono flex items-center gap-1.5">
                  <Layers className="h-4 w-4 text-slate-400" />
                  <span>{lang === 'zh' ? '已绑定通道所承载大模型一览' : 'Mapped Provider Models Inventory'}</span>
                </h4>
                <div className="text-[10px] text-slate-400 font-mono flex items-center gap-1.5">
                  <Activity className="h-3 w-3 text-emerald-450 animate-pulse" />
                  <span>{lang === 'zh' ? '接口映射：正常' : 'API mapped: OK'}</span>
                </div>
              </div>

              <div className="overflow-x-auto rounded-lg border border-[#1E293B] font-mono text-xs">
                <table className="w-full text-left">
                  <thead>
                    <tr className="bg-[#121824] text-slate-400 text-[10px] border-b border-[#1E293B]">
                      <th className="p-2.5 font-bold font-sans">{lang === 'zh' ? '模型实例名称' : 'Model Name'}</th>
                      <th className="p-2.5 font-bold font-sans">{lang === 'zh' ? '所属服务商' : 'Native Org'}</th>
                      <th className="p-2.5 font-bold font-sans">{lang === 'zh' ? '上下文硬吞吐上限' : 'Raw Context Window'}</th>
                      <th className="p-2.5 font-bold font-sans text-right">{lang === 'zh' ? 'Rtt 延迟' : 'RTT Latency'}</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-[#1E293B] text-slate-300 text-[10.5px]">
                    {models
                      .filter(m => (activeAgent.allowedProviders || [activeAgent.providerId]).includes(m.providerId))
                      .map(m => {
                        const pv = providers.find(p => p.id === m.providerId);
                        const isMainModel = m.id === activeAgent.modelId;
                        return (
                          <tr key={m.id} className={`hover:bg-[#121824] ${isMainModel ? 'text-indigo-400 bg-indigo-500/5' : ''}`}>
                            <td className="p-2.5 font-semibold">
                              <span className="flex items-center gap-1.5">
                                {isMainModel && <span className="h-1.5 w-1.5 rounded-full bg-indigo-400 animate-pulse" />}
                                <span>{m.name}</span>
                              </span>
                            </td>
                            <td className="p-2.5 text-slate-450 uppercase font-semibold">{pv?.name?.split(' ')[0] || m.providerId.toUpperCase()}</td>
                            <td className="p-2.5 text-slate-500 font-medium">{m.contextWindow}</td>
                            <td className="p-2.5 text-right font-medium text-slate-500">{m.latency}</td>
                          </tr>
                        );
                      })}
                  </tbody>
                </table>
              </div>
            </div>

            {/* Profiler statistics */}
            <div className="space-y-3.5 pt-1.5">
              <h4 className="text-xs font-bold text-slate-400 uppercase tracking-widest font-mono">{t('agentProfiler')}</h4>
              
              <div className="grid grid-cols-3 gap-3.5 text-center font-mono">
                <div className="p-3 bg-[#090D16] border border-[#1E293B] rounded-lg">
                  <span className="text-[10px] text-slate-500">{t('cumulativeCalls')}</span>
                  <div className="mt-1 text-xs font-bold text-[#F1F5F9]">{activeAgent.metrics.callsCount}</div>
                </div>

                <div className="p-3 bg-[#090D16] border border-[#1E293B] rounded-lg">
                  <span className="text-[10px] text-slate-500">{t('avgLatency')}</span>
                  <div className="mt-1 text-xs font-bold text-[#F1F5F9]">{activeAgent.metrics.avgLatency}</div>
                </div>

                <div className="p-3 bg-[#090D16] border border-[#1E293B] rounded-lg">
                  <span className="text-[10px] text-slate-500">{t('tokensProcessed')}</span>
                  <div className="mt-1 text-xs font-bold text-[#F1F5F9]">
                    {activeAgent.metrics.tokensSpent >= 1_000_000 
                      ? `${(activeAgent.metrics.tokensSpent / 1_000_000).toFixed(1)}M` 
                      : activeAgent.metrics.tokensSpent.toLocaleString()}
                  </div>
                </div>
              </div>
            </div>

          </div>
        ) : (
          <div className="p-8 text-center text-slate-500 bg-[#0F172A] border border-[#1E293B] rounded-2xl italic font-bold">
            {lang === 'zh' ? '请配置或选择一个活跃的 Agent 宿主' : 'Please configure or select an active agent profile.'}
          </div>
        )}
      </div>

    </div>
  );
}
