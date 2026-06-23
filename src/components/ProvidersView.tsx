import React, { useState } from 'react';
import { 
  Server, 
  Plus, 
  Trash2, 
  CheckCircle2, 
  Play, 
  Radio, 
  Key, 
  Globe, 
  Activity, 
  RefreshCw,
  Search,
  ChevronRight,
  Image,
  Layers
} from 'lucide-react';
import { Provider, ProviderType, Model } from '../types';
import { Locale } from '../locales';

interface ProvidersViewProps {
  providers: Provider[];
  onAddProvider: (provider: Omit<Provider, 'status' | 'latency'>, initialModelsList?: Model[]) => void;
  onUpdateProvider: (providerId: string, updates: Partial<Provider>) => void;
  onDeleteProvider: (providerId: string) => void;
  onTestConnection: (providerId: string) => void;
  onSwitchProvider: (providerId: string) => void;
  t: (key: any) => string;
  lang: Locale;
  models: Model[];
  onAddModel: (model: Model) => void;
  onUpdateModel: (modelId: string, updates: Partial<Model>) => void;
  onDeleteModel: (modelId: string) => void;
  onSyncProviderModels: (providerId: string) => Promise<void> | void;
}

export default function ProvidersView({
  providers,
  onAddProvider,
  onUpdateProvider,
  onDeleteProvider,
  onTestConnection,
  onSwitchProvider,
  t,
  lang,
  models,
  onAddModel,
  onUpdateModel,
  onDeleteModel,
  onSyncProviderModels
}: ProvidersViewProps) {
  const [selectedId, setSelectedId] = useState<string>(providers[0]?.id || '');
  const [isAdding, setIsAdding] = useState(false);
  const [syncingModelsFor, setSyncingModelsFor] = useState<string | null>(null);

  // Form states for new custom provider
  const [newName, setNewName] = useState('');
  const [newType, setNewType] = useState<ProviderType>('custom');
  const [newApiKey, setNewApiKey] = useState('');
  const [newBaseUrl, setNewBaseUrl] = useState('');

  // Form states for custom models being added during channel creation
  const [tempModels, setTempModels] = useState<Omit<Model, 'providerId' | 'isDefault' | 'latency'>[]>([
    { id: 'custom-gpt-4o', name: 'gpt-4o', tags: ['Standard'], contextWindow: '128K tokens', maxOutput: '4,096 tokens', supportsImages: true }
  ]);

  // Form states for adding a single new model
  const [addingModelName, setAddingModelName] = useState('');
  const [addingModelContext, setAddingModelContext] = useState('128K tokens');
  const [addingModelMaxOutput, setAddingModelMaxOutput] = useState('4,096 tokens');
  const [addingModelSupportsImages, setAddingModelSupportsImages] = useState(true);

  const activeProvider = providers.find(p => p.id === selectedId);

  const handleStartAdd = () => {
    setIsAdding(true);
    setNewName('');
    setNewType('custom');
    setNewApiKey('');
    setNewBaseUrl('https://api.yourprovider.v1');
    setTempModels([
      { id: 'custom-gpt-4o', name: 'gpt-4o', tags: ['Standard'], contextWindow: '128K tokens', maxOutput: '4,096 tokens', supportsImages: true }
    ]);
    setAddingModelName('');
  };

  const handleCreateSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim()) return alert(lang === 'zh' ? '请输入提供商标签或名称。' : 'Please enter a provider label/name.');
    
    const newId = `custom-${Date.now()}`;
    
    // Prepare initial models map
    const mappedModels: Model[] = tempModels.map((tm, idx) => ({
      id: `${newId}-${tm.id}`,
      name: tm.name,
      providerId: newId,
      tags: tm.tags,
      isDefault: idx === 0,
      contextWindow: tm.contextWindow,
      latency: '—',
      maxOutput: tm.maxOutput,
      supportsImages: tm.supportsImages
    }));

    onAddProvider({
      id: newId,
      name: newName,
      type: newType,
      apiKey: newApiKey,
      baseUrl: newBaseUrl
    }, mappedModels);

    setSelectedId(newId);
    setIsAdding(false);
  };

  const handleFieldChange = (field: keyof Provider, val: string) => {
    if (!activeProvider) return;
    onUpdateProvider(activeProvider.id, { [field]: val });
  };

  const handleSyncModels = async () => {
    if (!activeProvider || syncingModelsFor) return;
    setSyncingModelsFor(activeProvider.id);
    try {
      await onSyncProviderModels(activeProvider.id);
    } finally {
      setSyncingModelsFor(null);
    }
  };

  // Filter models belonging to active provider
  const providerModels = activeProvider ? models.filter(m => m.providerId === activeProvider.id) : [];

  return (
    <div id="providers-tab-panel" className="grid grid-cols-1 lg:grid-cols-3 gap-6 items-start h-full text-slate-150 font-sans">
      
      {/* Left Column: Configured Provider list */}
      <div className="lg:col-span-1 space-y-4">
        <div className="flex items-center justify-between pb-3 border-b border-[#1E293B]">
          <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider font-mono">
            {lang === 'zh' ? '配置好的通信通道' : 'CONNECTED INLET CHANNELS'}
          </h3>
          <button
            id="btn-new-provider-form"
            onClick={handleStartAdd}
            className="flex items-center gap-1.5 bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-[11px] px-3 py-1.5 rounded-lg transition-all cursor-pointer shadow-sm"
          >
            <Plus className="h-3.5 w-3.5" />
            <span>{lang === 'zh' ? '添加渠道' : 'ADD NEW'}</span>
          </button>
        </div>

        <div className="space-y-3">
          {providers.map((p) => {
            const isSelected = p.id === selectedId && !isAdding;
            const circuitOpen = Boolean(p.health?.circuitOpenUntil && new Date(p.health.circuitOpenUntil).getTime() > Date.now());
            return (
              <div
                id={`provider-list-card-${p.id}`}
                key={p.id}
                onClick={() => {
                  setSelectedId(p.id);
                  setIsAdding(false);
                }}
                className={`p-4 border rounded-xl cursor-pointer transition-all ${
                  isSelected
                    ? 'bg-indigo-600/10 border-indigo-500/40 text-slate-100 shadow-sm'
                    : 'bg-[#0F172A] border-[#1E293B] text-slate-350 hover:bg-[#131B2E] hover:border-slate-700'
                }`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2.5">
                    <Server className={`h-4.5 w-4.5 ${isSelected ? 'text-indigo-400' : 'text-slate-500'}`} />
                    <span className="text-xs font-bold font-sans">{p.name}</span>
                  </div>
                  <span className={`h-2 w-2 rounded-full ${
                    circuitOpen
                      ? 'bg-rose-500'
                      : p.status === 'connected'
                      ? 'bg-emerald-450 animate-pulse' 
                      : p.status === 'testing' 
                      ? 'bg-amber-400 animate-ping' 
                      : 'bg-slate-600'
                  }`} />
                </div>
                
                <div className="mt-3.5 pt-2 border-t border-dashed border-[#1E293B] flex items-center justify-between text-[9.5px] font-mono text-slate-400 uppercase font-semibold">
                  <span>{lang === 'zh' ? `协议格式: ${p.type}` : `FORMAT: ${p.type}`}</span>
                  <span className={`font-semibold ${circuitOpen ? 'text-rose-400' : 'text-slate-205'}`}>
                    {circuitOpen
                      ? (lang === 'zh' ? '熔断中' : 'Circuit open')
                      : p.health?.consecutiveFailures
                      ? `${lang === 'zh' ? '失败' : 'fail'}: ${p.health.consecutiveFailures}`
                      : p.latency !== '—'
                      ? (`${lang === 'zh' ? '实测延迟' : 'latency'}: ${p.latency}`)
                      : (lang === 'zh' ? '未测速' : 'Not Pinged')}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Right Column: Configuration & Form Settings */}
      <div className="lg:col-span-2">
        {isAdding ? (
          /* Create New Provider Form */
          <form 
            id="new-provider-detail-form" 
            onSubmit={handleCreateSubmit} 
            className="p-6 bg-[#0F172A] border border-[#1E293B] rounded-2xl shadow-sm space-y-6"
          >
            <h3 className="text-xs font-bold text-slate-200 flex items-center gap-2 uppercase tracking-wide font-mono border-b border-[#1E293B] pb-3">
              <Plus className="text-indigo-405 h-4 w-4" />
              <span>{lang === 'zh' ? '添加新的自定义大模型提供商 / API 渠道' : 'REGISTER NEW CUSTOM LLM INLET'}</span>
            </h3>
            
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div className="space-y-1.5">
                <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest font-mono">{lang === 'zh' ? '厂商标识(别名)' : 'PROVIDER LABEL'}</label>
                <input
                  id="new-prov-name"
                  type="text"
                  placeholder="e.g. DeepSeek / Localhost Tunnel"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-500 rounded-lg p-2.5 text-xs focus:outline-none font-sans"
                  required
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest font-mono">{lang === 'zh' ? '协议类型' : 'PROTOCOL ALIGNMENT'}</label>
                <select
                  id="new-prov-type"
                  value={newType}
                  onChange={(e) => setNewType(e.target.value as ProviderType)}
                  className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-505 rounded-lg p-2.5 text-xs font-mono font-bold focus:outline-none"
                >
                  <option value="custom">{lang === 'zh' ? '通用自定义 Webhook' : 'Generic Custom Webhook'}</option>
                  <option value="openai">{lang === 'zh' ? 'OpenAI 兼容接口规范' : 'OpenAI Compatible Format'}</option>
                  <option value="anthropic">{lang === 'zh' ? 'Anthropic Messages 接口' : 'Anthropic API'}</option>
                  <option value="gemini">{lang === 'zh' ? '谷歌 Gemini 官方 REST 接口' : 'Google Gemini Developer REST'}</option>
                </select>
              </div>
            </div>

            <div className="space-y-1.5">
              <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest font-mono">{lang === 'zh' ? '服务端点 Base URL' : 'ENDPOINT BASE URL'}</label>
              <input
                id="new-prov-url"
                type="text"
                placeholder="https://api.yourprovider.v1/v1"
                value={newBaseUrl}
                onChange={(e) => setNewBaseUrl(e.target.value)}
                className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-500 rounded-lg p-2.5 text-xs font-mono focus:outline-none"
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest font-mono">{lang === 'zh' ? '大模型密钥 API Key' : 'SECRET TOKEN / CREDIT CHECK'}</label>
              <input
                id="new-prov-key"
                type="password"
                placeholder="sk-••••••••"
                value={newApiKey}
                onChange={(e) => setNewApiKey(e.target.value)}
                className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-500 rounded-lg p-2.5 text-xs font-mono focus:outline-none"
              />
            </div>

            {/* Added Models List Config UI for New Provider */}
            <div className="border border-[#1E293B] bg-[#131B2E] p-4 rounded-xl space-y-4">
              <div className="flex items-center justify-between border-b border-[#1E293B] pb-2">
                <h4 className="text-[11px] font-semibold text-slate-200 tracking-wider uppercase font-mono flex items-center gap-2">
                  <Layers className="h-4 w-4 text-indigo-400" />
                  <span>{lang === 'zh' ? '该渠道关联的大模型列表' : 'CHANNEL MODELS INVENTORY'}</span>
                </h4>
                <span className="text-[10px] font-mono font-semibold text-slate-400 bg-[#090D16] px-2.5 py-0.5 rounded-full border border-[#1E293B]">
                  {tempModels.length} {lang === 'zh' ? '个模型' : 'models'}
                </span>
              </div>

              {/* Added Models preview cards */}
              {tempModels.length === 0 ? (
                <p className="text-[11px] text-slate-450 italic py-2 text-center bg-[#090D16] border border-dashed border-[#1E293B] rounded-lg">
                  {lang === 'zh' ? '暂未添加任何模型，请在下方补充至少一个模型' : 'No models configured yet. Use the wizard below to append.'}
                </p>
              ) : (
                <div className="space-y-2 max-h-48 overflow-y-auto pr-1">
                  {tempModels.map((m, idx) => (
                    <div key={idx} className="flex items-start justify-between bg-[#090D16] border border-[#1E293B] p-3 rounded-lg hover:border-slate-700 transition-all">
                      <div className="space-y-1">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="text-xs font-bold text-slate-100 font-mono">{m.name}</span>
                          {m.supportsImages && (
                            <span className="text-[9px] bg-emerald-500/10 text-emerald-500 border border-emerald-500/20 px-1.5 py-0.2 rounded font-mono font-semibold flex items-center gap-1">
                              <Image className="h-2.5 w-2.5" />
                              {lang === 'zh' ? '支持图片' : 'VISION'}
                            </span>
                          )}
                        </div>
                        <div className="flex items-center gap-3 text-[10px] font-mono text-slate-450">
                          <span>{lang === 'zh' ? '上下文: ' : 'Ctx: '}<strong className="text-slate-350">{m.contextWindow}</strong></span>
                          <span>•</span>
                          <span>{lang === 'zh' ? '最大输出: ' : 'Max Out: '}<strong className="text-slate-350">{m.maxOutput}</strong></span>
                        </div>
                      </div>

                      <button
                        type="button"
                        onClick={() => setTempModels(prev => prev.filter((_, i) => i !== idx))}
                        className="p-1 hover:bg-rose-500/10 text-slate-400 hover:text-rose-450 rounded transition-colors"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  ))}
                </div>
              )}

              {/* Form elements to append model to tempModels */}
              <div className="bg-[#090D16] border border-[#1E293B] p-3 rounded-xl space-y-3">
                <div className="text-[10px] font-bold text-indigo-400 uppercase tracking-wider font-mono">
                  {lang === 'zh' ? '创建并增加模型属性' : 'DEFINE MODEL META SCHEMA'}
                </div>
                
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                  <div className="space-y-1">
                    <label className="text-[9px] font-semibold text-slate-450 uppercase font-mono">{lang === 'zh' ? '模型名称 / ID' : 'MODEL NAME / ID'}</label>
                    <input
                      type="text"
                      placeholder="e.g. gpt-4o"
                      value={addingModelName}
                      onChange={(e) => setAddingModelName(e.target.value)}
                      className="w-full bg-[#131B2E] text-[#F1F5F9] border border-[#1E293B] rounded p-2 text-xs font-mono focus:outline-none focus:border-indigo-500"
                    />
                  </div>

                  <div className="space-y-1">
                    <label className="text-[9px] font-semibold text-slate-450 uppercase font-mono">{lang === 'zh' ? '上下文长度限额' : 'CONTEXT WINDOW'}</label>
                    <input
                      type="text"
                      placeholder="e.g. 128K"
                      value={addingModelContext}
                      onChange={(e) => setAddingModelContext(e.target.value)}
                      className="w-full bg-[#131B2E] text-[#F1F5F9] border border-[#1E293B] rounded p-2 text-xs font-mono focus:outline-none"
                    />
                  </div>

                  <div className="space-y-1">
                    <label className="text-[9px] font-semibold text-slate-450 uppercase font-mono">{lang === 'zh' ? '单次最大输出' : 'MAX OUTPUT'}</label>
                    <input
                      type="text"
                      placeholder="e.g. 4,096 tokens"
                      value={addingModelMaxOutput}
                      onChange={(e) => setAddingModelMaxOutput(e.target.value)}
                      className="w-full bg-[#131B2E] text-[#F1F5F9] border border-[#1E293B] rounded p-2 text-xs font-mono focus:outline-none"
                    />
                  </div>
                </div>

                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2.5 pt-1">
                  <label className="flex items-center gap-2 cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={addingModelSupportsImages}
                      onChange={(e) => setAddingModelSupportsImages(e.target.checked)}
                      className="h-3.5 w-3.5 rounded text-indigo-600 focus:ring-opacity-45 bg-[#131B2E] border-[#1E293B]"
                    />
                    <span className="text-[10px] text-slate-300 font-semibold font-sans">
                      {lang === 'zh' ? '支持视觉图片解析 (图像输入支持)' : 'Model supports image visual recognition (Multimodal)'}
                    </span>
                  </label>

                  <button
                    type="button"
                    onClick={() => {
                      if (!addingModelName.trim()) {
                        alert(lang === 'zh' ? '请输入模型名称' : 'Please input a model name');
                        return;
                      }
                      const modelSlug = addingModelName.trim().toLowerCase().replace('/', '-');
                      setTempModels(prev => [
                        ...prev,
                        {
                          id: modelSlug,
                          name: addingModelName.trim(),
                          tags: addingModelSupportsImages ? ['Multimodal'] : ['Standard'],
                          contextWindow: addingModelContext.trim() || '128K tokens',
                          maxOutput: addingModelMaxOutput.trim() || '4,096 tokens',
                          supportsImages: addingModelSupportsImages
                        }
                      ]);
                      setAddingModelName('');
                    }}
                    className="flex items-center gap-1.5 bg-indigo-600/20 hover:bg-indigo-600 border border-indigo-500/20 hover:border-indigo-500 text-indigo-300 hover:text-white px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer"
                  >
                    <Plus className="h-3 w-3" />
                    <span>{lang === 'zh' ? '加入此模型' : 'Append to Channel'}</span>
                  </button>
                </div>
              </div>
            </div>

            <div className="flex items-center justify-end gap-2.5 pt-2">
              <button
                id="btn-cancel-add-prov"
                type="button"
                onClick={() => setIsAdding(false)}
                className="px-4 py-2 border border-[#1E293B] bg-slate-800 text-slate-300 rounded-lg font-semibold hover:bg-slate-705 cursor-pointer text-xs transition-colors"
              >
                {t('cancel')}
              </button>
              <button
                id="btn-submit-add-prov"
                type="submit"
                className="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white font-semibold rounded-lg text-xs cursor-pointer transition-colors shadow-sm"
              >
                {t('saveSettings')}
              </button>
            </div>
          </form>
        ) : activeProvider ? (
          /* View and Edit Existing Provider */
          <div id="provider-detail-panel" className="p-6 bg-[#0F172A] border border-[#1E293B] rounded-2xl shadow-sm space-y-6">
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-[#1E293B]">
              <div className="space-y-1.5">
                <span className="text-[10px] bg-indigo-605/10 text-indigo-400 border border-indigo-500/20 px-2 py-0.5 rounded font-mono font-bold uppercase w-fit block">
                  {lang === 'zh' ? '管理大模型端点' : 'PROXY ENDPOINT CONFIG'}
                </span>
                <h3 className="text-sm font-bold text-slate-200">{activeProvider.name}</h3>
              </div>

              {/* Header Actions */}
              <div className="flex items-center gap-2">
                <button
                  id="btn-switch-prov-live"
                  type="button"
                  onClick={() => onSwitchProvider(activeProvider.id)}
                  disabled={activeProvider.isCurrent}
                  className="flex items-center gap-1.5 bg-indigo-600 hover:bg-indigo-500 text-white border border-indigo-500 rounded-lg px-3.5 py-1.5 text-xs font-semibold transition-all disabled:bg-slate-800 disabled:text-slate-500 disabled:border-slate-700 disabled:cursor-default cursor-pointer"
                >
                  <CheckCircle2 className="h-3.5 w-3.5" />
                  <span>{activeProvider.isCurrent ? (lang === 'zh' ? '已启用' : 'Active') : (lang === 'zh' ? '启用' : 'Enable')}</span>
                </button>

                <button
                  id="btn-test-prov-connection"
                  type="button"
                  onClick={() => onTestConnection(activeProvider.id)}
                  disabled={activeProvider.status === 'testing'}
                  className="flex items-center gap-1.5 bg-slate-800 hover:bg-slate-705 text-slate-200 border border-slate-700 rounded-lg px-3.5 py-1.5 text-xs font-semibold transition-all disabled:opacity-50 cursor-pointer"
                >
                  <Activity className={`h-3.5 w-3.5 text-emerald-400 ${activeProvider.status === 'testing' ? 'animate-spin' : ''}`} />
                  <span>{activeProvider.status === 'testing' ? t('pinging') : t('testConnection')}</span>
                </button>

                {activeProvider.id.startsWith('custom') && (
                  <button
                     id="btn-delete-prov"
                     type="button"
                     onClick={() => {
                       if(confirm(lang === 'zh' ? `确定要删除提供商 ${activeProvider.name} 吗？` : `Are you sure you want to delete ${activeProvider.name}?`)) {
                         onDeleteProvider(activeProvider.id);
                         setSelectedId(providers[0]?.id || '');
                       }
                     }}
                     className="p-1.5 hover:bg-rose-950/40 text-slate-400 hover:text-rose-400 border border-[#1E293B] rounded-lg bg-slate-800 transition-all cursor-pointer"
                     title={t('deleteProvider')}
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                )}
              </div>
            </div>

            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="space-y-1.5">
                  <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest font-mono">{t('connectionLabel')}</label>
                  <input
                    id="edit-prov-name"
                    type="text"
                    value={activeProvider.name}
                    onChange={(e) => handleFieldChange('name', e.target.value)}
                    className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-500 rounded-lg p-2.5 text-xs focus:outline-none"
                  />
                </div>

                <div className="space-y-1.5">
                  <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest font-mono">{t('coreProtocol')}</label>
                  <select
                     id="edit-prov-type"
                     value={activeProvider.type}
                     disabled
                     className="w-full bg-[#090D16] text-slate-450 border border-[#1E293B] font-mono rounded-lg p-2.5 text-xs cursor-not-allowed uppercase font-semibold"
                  >
                    <option value="custom">{lang === 'zh' ? '通用自定义 Webhook' : 'Generic Custom Webhook'}</option>
                    <option value="openai">{lang === 'zh' ? 'OpenAI 兼容接口规范' : 'OpenAI Compatible Format'}</option>
                    <option value="anthropic">{lang === 'zh' ? 'Anthropic Messages 接口' : 'Anthropic API'}</option>
                    <option value="gemini">{lang === 'zh' ? '谷歌 Gemini 官方 REST 接口' : 'Google Gemini Developer REST'}</option>
                    <option value="openrouter">{lang === 'zh' ? 'OpenRouter 直连端点格式' : 'OpenRouter API'}</option>
                  </select>
                </div>
              </div>

              <div className="space-y-1.5">
                <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest font-mono flex items-center gap-1.5">
                  <Globe className="h-3.5 w-3.5 text-slate-450" />
                  <span>{t('endpointUrl')}</span>
                </label>
                <input
                  id="edit-prov-url"
                  type="text"
                  value={activeProvider.baseUrl || ''}
                  onChange={(e) => handleFieldChange('baseUrl', e.target.value)}
                  className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-500 rounded-lg p-2.5 text-xs font-mono focus:outline-none"
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest font-mono flex items-center gap-1.5">
                  <Key className="h-3.5 w-3.5 text-slate-450" />
                  <span>{t('secretToken')}</span>
                </label>
                <input
                  id="edit-prov-key"
                  type="password"
                  value={activeProvider.apiKey || ''}
                  onChange={(e) => handleFieldChange('apiKey', e.target.value)}
                  placeholder={lang === 'zh' ? '密钥未指定或为空' : 'Key not specified or empty'}
                  className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-505 rounded-lg p-2.5 text-xs font-mono focus:outline-none"
                />
              </div>
            </div>

            {/* Diagnostic Alert box */}
            <div className="p-4 bg-[#090D16] border border-[#1E293B] rounded-xl flex items-center gap-3">
              <div className={`p-2 rounded-lg border ${
                activeProvider.status === 'connected' ? 'bg-indigo-605/10 text-indigo-405 border-indigo-500/20' : 'bg-slate-800 text-slate-450 border-[#1E293B]'
              }`}>
                <Radio className="h-4.5 w-4.5 animate-pulse" />
              </div>
              <div className="text-[11px] leading-relaxed">
                <h4 className="font-bold text-slate-200 font-sans pb-0.5">{t('latencyDiagnostics')}</h4>
                <p className="text-slate-400 font-mono text-[10px] font-medium">
                  {activeProvider.status === 'connected' 
                    ? (lang === 'zh' ? `网关已成功握手。实测往返延迟 (RTT) 耗时为 ${activeProvider.latency}。` : `Proxy verified. Current endpoint roundtrip handling RTT is ${activeProvider.latency}.`) 
                    : activeProvider.status === 'testing'
                    ? t('pinging')
                    : (lang === 'zh' ? '空空如也。等待发起连通性及网络延迟测速。' : 'Awaiting manual connection test.')}
                </p>
              </div>
            </div>

            {/* Models Configuration and Edit Panel for Active Provider */}
            <div className="border border-[#1E293B] bg-[#131B2E] p-4 sm:p-5 rounded-2xl space-y-4">
              <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2 border-b border-[#1E293B] pb-2">
                <h4 className="text-[11px] font-bold text-slate-200 tracking-wider uppercase font-mono flex items-center gap-2">
                  <Layers className="h-4 w-4 text-indigo-400" />
                  <span>{lang === 'zh' ? '对应底层大模型列表与调试参数' : 'CORRESPONDING MODELS ROUTING LAYOUT'}</span>
                </h4>
                <div className="flex items-center gap-2">
                  <button
                    id="btn-sync-provider-models"
                    type="button"
                    onClick={handleSyncModels}
                    disabled={syncingModelsFor === activeProvider.id}
                    className="inline-flex items-center gap-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 rounded-lg px-2.5 py-1.5 text-[10px] font-bold font-mono transition-all disabled:opacity-50 cursor-pointer"
                    title={lang === 'zh' ? '从上游 /v1/models 获取并导入模型列表' : 'Fetch and import upstream /v1/models'}
                  >
                    <RefreshCw className={`h-3.5 w-3.5 text-indigo-400 ${syncingModelsFor === activeProvider.id ? 'animate-spin' : ''}`} />
                    <span>{lang === 'zh' ? '获取模型列表' : 'Fetch Models'}</span>
                  </button>
                  <span className="text-[10px] font-mono font-bold text-indigo-400 bg-indigo-605/10 px-2.5 py-0.5 rounded-full border border-indigo-500/20">
                    {providerModels.length} {lang === 'zh' ? '个模型绑定' : 'models configured'}
                  </span>
                </div>
              </div>

              {providerModels.length === 0 ? (
                <div className="text-center p-6 bg-[#090D16] border border-dashed border-[#1E293B] rounded-xl text-slate-500 text-xs italic">
                  {lang === 'zh' ? '该通道暂无任何模型关联。请在下方补充注册子模型。' : 'No active models bounds with this node. Register one below.'}
                </div>
              ) : (
                <div className="space-y-3.5">
                  {providerModels.map((m) => {
                    return (
                      <div key={m.id} className="p-4 bg-[#090D16] border border-[#1E293B] rounded-xl space-y-3 relative hover:border-slate-700 transition-colors">
                        {/* Top info and deletion button */}
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2 flex-wrap">
                            <span className="text-xs font-bold text-slate-100 font-mono">{m.name}</span>
                            {m.isDefault && (
                              <span className="text-[9px] bg-indigo-600/10 text-indigo-400 border border-indigo-500/20 px-1.5 py-0.2 rounded font-mono font-bold">
                                {lang === 'zh' ? '主路由' : 'DEFAULT'}
                              </span>
                            )}
                          </div>

                          <button
                            type="button"
                            onClick={() => {
                              if (confirm(lang === 'zh' ? `确认移除底层模型 ${m.name} 吗？` : `Confirm deleting model ${m.name}?`)) {
                                onDeleteModel(m.id);
                              }
                            }}
                            className="p-1 hover:bg-rose-500/10 text-slate-4 w-7 h-7 hover:text-rose-450 rounded-lg flex items-center justify-center transition-all cursor-pointer"
                            title={lang === 'zh' ? '移除模型' : 'Delete model'}
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                          </button>
                        </div>

                        {/* Config fields */}
                        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
                          <div className="space-y-1">
                            <label className="text-[9px] font-semibold text-slate-405 uppercase font-mono">{lang === 'zh' ? '上下文窗口限额' : 'CONTEXT WINDOW'}</label>
                            <input
                              type="text"
                              value={m.contextWindow}
                              onChange={(e) => onUpdateModel(m.id, { contextWindow: e.target.value })}
                              className="w-full bg-[#131B2E] text-[#F1F5F9] border border-[#1E293B] rounded p-1.5 text-xs font-mono focus:outline-none focus:border-indigo-500"
                            />
                          </div>

                          <div className="space-y-1">
                            <label className="text-[9px] font-semibold text-slate-455 uppercase font-mono">{lang === 'zh' ? '最大输出 Token' : 'MAX OUTPUT'}</label>
                            <input
                              type="text"
                              value={m.maxOutput}
                              onChange={(e) => onUpdateModel(m.id, { maxOutput: e.target.value })}
                              className="w-full bg-[#131B2E] text-[#F1F5F9] border border-[#1E293B] rounded p-1.5 text-xs font-mono focus:outline-none focus:border-indigo-500"
                            />
                          </div>

                          <div className="flex flex-col justify-end pb-1.5">
                            <label className="flex items-center gap-2 cursor-pointer select-none">
                              <input
                                type="checkbox"
                                checked={!!m.supportsImages}
                                onChange={(e) => onUpdateModel(m.id, { supportsImages: e.target.checked })}
                                className="h-3.5 w-3.5 rounded text-indigo-600 focus:ring-opacity-45 bg-[#131B2E] border-[#1E293B]"
                              />
                              <span className="text-[10px] text-slate-300 font-semibold font-sans">
                                {lang === 'zh' ? '支持视觉图片/多模态' : 'Supports visual / image data'}
                              </span>
                            </label>
                          </div>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}

              {/* Add model inline to existing active provider card */}
              <div className="bg-[#090D16] border border-[#1E293B] p-4 rounded-xl space-y-4">
                <div className="text-[10px] font-bold text-slate-205 uppercase tracking-widest font-mono flex items-center gap-1.5 border-b border-[#1E293B]/60 pb-1.5">
                  <Plus className="h-3.5 w-3.5 text-indigo-400" />
                  <span>{lang === 'zh' ? '为此渠道快速追加绑定新大模型' : 'BIND CONCURRENT SUB-MODEL INSTANCE'}</span>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                  <div className="space-y-1">
                    <label className="text-[9px] font-semibold text-slate-450 uppercase font-mono">{lang === 'zh' ? '模型名称 / ID' : 'MODEL NAME / ID'}</label>
                    <input
                      type="text"
                      placeholder="e.g. deepseek-reasoner"
                      value={addingModelName}
                      onChange={(e) => setAddingModelName(e.target.value)}
                      className="w-full bg-[#131B2E] text-[#F1F5F9] border border-[#1E293B] rounded p-2 text-xs font-mono focus:outline-none focus:border-indigo-500"
                    />
                  </div>

                  <div className="space-y-1">
                    <label className="text-[9px] font-semibold text-slate-450 uppercase font-mono">{lang === 'zh' ? '上下文窗口限度' : 'CONTEXT DEPTH'}</label>
                    <input
                      type="text"
                      placeholder="e.g. 64K tokens"
                      value={addingModelContext}
                      onChange={(e) => setAddingModelContext(e.target.value)}
                      className="w-full bg-[#131B2E] text-[#F1F5F9] border border-[#1E293B] rounded p-2 text-xs font-mono focus:outline-none"
                    />
                  </div>

                  <div className="space-y-1">
                    <label className="text-[9px] font-semibold text-slate-450 uppercase font-mono">{lang === 'zh' ? '最大单次输出' : 'MAX OUT'}</label>
                    <input
                      type="text"
                      placeholder="e.g. 8,192 tokens"
                      value={addingModelMaxOutput}
                      onChange={(e) => setAddingModelMaxOutput(e.target.value)}
                      className="w-full bg-[#131B2E] text-[#F1F5F9] border border-[#1E293B] rounded p-2 text-xs font-mono focus:outline-none"
                    />
                  </div>
                </div>

                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pt-1">
                  <label className="flex items-center gap-2 cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={addingModelSupportsImages}
                      onChange={(e) => setAddingModelSupportsImages(e.target.checked)}
                      className="h-3.5 w-3.5 rounded text-indigo-600 focus:ring-opacity-45 bg-[#131B2E] border-[#1E293B]"
                    />
                    <span className="text-[10px] text-slate-300 font-semibold font-sans">
                      {lang === 'zh' ? '支持视觉处理 (Multimodal)' : 'Supports Image reading (Multimodal)'}
                    </span>
                  </label>

                  <button
                    type="button"
                    onClick={() => {
                      if (!addingModelName.trim()) {
                        alert(lang === 'zh' ? '请输入模型名称' : 'Please input a model name');
                        return;
                      }
                      const modelSlug = addingModelName.trim().toLowerCase().replace('/', '-');
                      
                      onAddModel({
                        id: `${activeProvider.id}-${modelSlug}`,
                        name: addingModelName.trim(),
                        providerId: activeProvider.id,
                        tags: addingModelSupportsImages ? ['Multimodal'] : ['Standard'],
                        isDefault: providerModels.length === 0,
                        contextWindow: addingModelContext.trim() || '128K tokens',
                        latency: '—',
                        maxOutput: addingModelMaxOutput.trim() || '4,096 tokens',
                        supportsImages: addingModelSupportsImages
                      });

                      setAddingModelName('');
                    }}
                    className="flex items-center gap-1.5 bg-indigo-600/20 hover:bg-indigo-600 border border-indigo-500/20 hover:border-indigo-500 text-indigo-300 hover:text-white px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer shadow-sm"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    <span>{lang === 'zh' ? '追加并绑定此大模型' : 'Bind & Activate Model'}</span>
                  </button>
                </div>
              </div>
            </div>

          </div>
        ) : (
          <div className="p-8 text-center text-slate-505 bg-[#0F172A] border border-[#1E293B] rounded-2xl font-bold italic shadow-sm">
            {lang === 'zh' ? '请配置或激活至少一个 AI 提供商' : 'Please configure or choose at least one active AI Provider.'}
          </div>
        )}
      </div>

    </div>
  );
}
