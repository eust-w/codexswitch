import React, { useState, useEffect, useRef } from 'react';
import { 
  Send, 
  Bot, 
  User, 
  Trash2, 
  Zap, 
  Sliders, 
  Sparkles, 
  Activity, 
  Binary,
  Layers,
  HelpCircle
} from 'lucide-react';
import { Provider, Model, LogEntry } from '../types';
import { Locale } from '../locales';
import { apiRequest } from '../lib/api';

interface ChatViewProps {
  providers: Provider[];
  models: Model[];
  lang: Locale;
  t: (key: any) => string;
  onAddLog: (level: 'info' | 'warn' | 'error' | 'debug', source: string, message: string) => void;
}

interface ChatMessage {
  id: string;
  sender: 'user' | 'model';
  content: string;
  timestamp: string;
  latency?: string;
  tokensUsed?: number;
  providerId: string;
  modelId: string;
  formatConverted?: boolean;
}

export default function ChatView({
  providers,
  models,
  lang,
  t,
  onAddLog
}: ChatViewProps) {
  // Find connected providers
  const activeProviders = providers.filter(p => p.status === 'connected' || p.apiKey);
  
  // States
  const [selectedProviderId, setSelectedProviderId] = useState<string>(
    activeProviders[0]?.id || providers[0]?.id || ''
  );
  
  const currentModels = models.filter(m => m.providerId === selectedProviderId);
  const [selectedModelId, setSelectedModelId] = useState<string>(
    currentModels[0]?.id || ''
  );

  // Sync selected model when provider changes
  useEffect(() => {
    const matched = models.filter(m => m.providerId === selectedProviderId);
    if (matched.length > 0) {
      setSelectedModelId(matched[0].id);
    } else {
      setSelectedModelId('');
    }
  }, [selectedProviderId, models]);

  // Parameters
  const [temperature, setTemperature] = useState<number>(0.7);
  const [maxTokens, setMaxTokens] = useState<number>(4096);
  const [contextLimit, setContextLimit] = useState<number>(128);
  const [autoConvert, setAutoConvert] = useState<boolean>(true);

  // Messages State
  const [messages, setMessages] = useState<ChatMessage[]>(() => {
    const saved = localStorage.getItem('ah_playground_chats');
    return saved ? JSON.parse(saved) : [];
  });

  const [input, setInput] = useState<string>('');
  const [isTyping, setIsTyping] = useState<boolean>(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Persist Messages
  useEffect(() => {
    localStorage.setItem('ah_playground_chats', JSON.stringify(messages));
  }, [messages]);

  // Adjust scroll lock
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, isTyping]);

  const handleSend = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isTyping) return;

    const userMessageText = input.trim();
    setInput('');

    const providerObj = providers.find(p => p.id === selectedProviderId);
    const modelObj = models.find(m => m.id === selectedModelId);

    const timestamp = new Date().toISOString().replace('T', ' ').substring(11, 19);

    const userMsg: ChatMessage = {
      id: `chat-msg-${Date.now()}-user`,
      sender: 'user',
      content: userMessageText,
      timestamp,
      providerId: selectedProviderId,
      modelId: selectedModelId
    };

    setMessages(prev => [...prev, userMsg]);
    onAddLog('info', 'Playground', `Dispatching test query to ${modelObj?.name || selectedModelId} (${providerObj?.name || selectedProviderId})`);

    setIsTyping(true);

    try {
      const res = await apiRequest('/api/chat/send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          input: userMessageText,
          selectedProviderId,
          selectedModelId,
          temperature,
          maxTokens,
          contextLimit,
          autoConvert,
          lang
        })
      });

      if (res.ok) {
        const result = await res.json();
        
        const modelReply: ChatMessage = {
          id: `chat-msg-${Date.now()}-model`,
          sender: 'model',
          content: result.replyContent,
          timestamp: new Date().toISOString().replace('T', ' ').substring(11, 19),
          latency: result.isSuccess ? `${result.estimatedLatencyMs}ms` : undefined,
          tokensUsed: result.isSuccess ? Math.floor(result.replyContent.length * 0.75) : undefined,
          providerId: selectedProviderId,
          modelId: selectedModelId,
          formatConverted: result.isSuccess && autoConvert
        };

        setMessages(prev => [...prev, modelReply]);
        setIsTyping(false);
      } else {
        setIsTyping(false);
      }
    } catch (err) {
      console.error(err);
      setIsTyping(false);
    }
  };

  const handleClearHistory = () => {
    setMessages([]);
    onAddLog('warn', 'Playground', 'Cleared sandbox chat playground history.');
  };

  // Quick prompt helper
  const handleQuickPrompt = (txt: string) => {
    setInput(txt);
  };

  return (
    <div id="chat-tab-panel" className="grid grid-cols-1 lg:grid-cols-4 gap-6 h-full items-stretch text-slate-100 font-sans">
      
      {/* 1. Left Side: Parameter Panel */}
      <div className="lg:col-span-1 p-5 bg-[#0F172A] border border-[#1E293B] rounded-2xl flex flex-col justify-between space-y-5 h-full select-none">
        <div className="space-y-4">
          <div className="flex items-center gap-2 pb-2.5 border-b border-[#1E293B]">
            <Sliders className="h-4 w-4 text-indigo-400" />
            <h3 className="text-xs font-bold text-slate-300 uppercase tracking-wider font-mono">
              {lang === 'zh' ? '沙盒环境调试参数' : 'SANDBOX PARAMETERS'}
            </h3>
          </div>

          {/* Model selection */}
          <div className="space-y-3.5">
            <div className="space-y-1.5">
              <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest block font-mono">
                {lang === 'zh' ? '测速服务商' : 'PROVIDER'}
              </label>
              <select
                id="pg-provider-select"
                value={selectedProviderId}
                onChange={(e) => setSelectedProviderId(e.target.value)}
                className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-500 rounded-lg p-2.5 text-xs font-mono font-bold focus:outline-none"
              >
                {providers.map(p => (
                  <option key={p.id} value={p.id}>
                    {p.name} {p.status === 'connected' ? '🟢' : '⚪'}
                  </option>
                ))}
              </select>
            </div>

            <div className="space-y-1.5">
              <label className="text-[10px] font-semibold text-slate-400 uppercase tracking-widest block font-mono">
                {lang === 'zh' ? '评估目标模型' : 'TARGET ENGINE MODEL'}
              </label>
              <select
                id="pg-model-select"
                value={selectedModelId}
                onChange={(e) => setSelectedModelId(e.target.value)}
                disabled={currentModels.length === 0}
                className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-500 rounded-lg p-2.5 text-xs font-mono font-bold focus:outline-none disabled:opacity-40"
              >
                {currentModels.length > 0 ? (
                  currentModels.map(m => (
                    <option key={m.id} value={m.id}>{m.name}</option>
                  ))
                ) : (
                  <option value="">{lang === 'zh' ? '-- 无可用模型 --' : '-- No models --'}</option>
                )}
              </select>
            </div>
          </div>

          <div className="p-4 bg-[#090D16] border border-[#1E293B] rounded-xl space-y-4 pt-3.5">
            {/* Format Adaptor Switch */}
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-bold text-slate-300 font-sans">
                {lang === 'zh' ? '格式协议自适应' : 'AUTO ADAPT FORMAT'}
              </span>
              <input
                id="pg-convert-switch"
                type="checkbox"
                checked={autoConvert}
                onChange={(e) => setAutoConvert(e.target.checked)}
                className="w-7 h-4 bg-slate-800 rounded-full appearance-none cursor-pointer relative checked:bg-indigo-600 before:content-[''] before:absolute before:w-3 before:h-3 before:bg-slate-500 before:rounded-full before:top-[1px] before:left-[1px] before:transition-all checked:before:translate-x-3 checked:before:bg-white border border-[#1E293B]"
              />
            </div>

            {/* Context Slider */}
            <div className="space-y-1.5">
              <div className="flex items-center justify-between font-mono text-[10px]">
                <span className="text-slate-400 font-sans font-semibold">{lang === 'zh' ? '上下文窗口限制' : 'Context Pool bounds'}</span>
                <span className="text-indigo-400 font-extrabold">{contextLimit}K tokens</span>
              </div>
              <input
                id="pg-context-slider"
                type="range"
                min="8"
                max="1024"
                step="8"
                value={contextLimit}
                onChange={(e) => setContextLimit(parseInt(e.target.value))}
                className="w-full h-1 bg-slate-800 rounded appearance-none cursor-pointer accent-indigo-500"
              />
            </div>

            {/* Max tokens */}
            <div className="space-y-1.5">
              <div className="flex items-center justify-between font-mono text-[10px]">
                <span className="text-slate-400 font-sans font-semibold">{lang === 'zh' ? '最大吐字参数限制' : 'Max Output tokens'}</span>
                <span className="text-indigo-400 font-extrabold">{maxTokens}</span>
              </div>
              <input
                id="pg-output-slider"
                type="range"
                min="512"
                max="16384"
                step="512"
                value={maxTokens}
                onChange={(e) => setMaxTokens(parseInt(e.target.value))}
                className="w-full h-1 bg-slate-800 rounded appearance-none cursor-pointer accent-indigo-500"
              />
            </div>

            {/* Temperature */}
            <div className="space-y-1.5">
              <div className="flex items-center justify-between font-mono text-[10px]">
                <span className="text-slate-400 font-sans font-semibold">{lang === 'zh' ? '热度温度调节' : 'Temperature'}</span>
                <span className="text-indigo-400 font-extrabold">{temperature.toFixed(1)}</span>
              </div>
              <input
                id="pg-temp-slider"
                type="range"
                min="0.0"
                max="2.0"
                step="0.1"
                value={temperature}
                onChange={(e) => setTemperature(parseFloat(e.target.value))}
                className="w-full h-1 bg-slate-800 rounded appearance-none cursor-pointer accent-indigo-500"
              />
            </div>
          </div>
        </div>

        {/* Clear history */}
        <div className="pt-3 border-t border-[#1E293B]">
          <button
            id="pg-clear-btn"
            onClick={handleClearHistory}
            disabled={messages.length === 0}
            className="w-full flex items-center justify-center gap-2 py-2 px-3 bg-[#090D16] hover:bg-rose-950/25 border border-[#1E293B] text-slate-300 disabled:opacity-30 disabled:cursor-not-allowed font-bold text-xs rounded-lg transition-colors cursor-pointer"
          >
            <Trash2 className="h-3.5 w-3.5" />
            <span>{t('clearChat')}</span>
          </button>
        </div>
      </div>

      {/* 2. Right Side: Interactive Dialogue Box */}
      <div className="lg:col-span-3 flex flex-col bg-[#0F172A] border border-[#1E293B] rounded-2xl overflow-hidden h-[600px] lg:h-full relative shadow-sm">
        
        {/* Chat view header */}
        <div className="p-4 bg-[#131B2E]/40 border-b border-[#1E293B] flex flex-col sm:flex-row sm:items-center justify-between gap-3 shrink-0">
          <div className="space-y-1">
            <h2 className="text-xs font-bold text-slate-200 uppercase font-mono flex items-center gap-1.5 leading-none">
              <Bot className="h-4.5 w-4.5 text-indigo-400" />
              <span>{t('chatTitle')}</span>
              <span className="text-[10px] bg-indigo-500/10 text-indigo-400 px-1.5 py-0.5 border border-indigo-500/20 rounded font-bold font-mono">
                {lang === 'zh' ? '实时沙盒测试' : 'SANDBOX SESSION'}
              </span>
            </h2>
            <p className="text-[10.5px] text-slate-400 max-w-xl truncate font-semibold font-sans">
              {t('chatSubtitle')}
            </p>
          </div>
          
          <div className="flex items-center gap-2 font-mono text-[10px] shrink-0">
            <span className="text-slate-400 font-bold">{lang === 'zh' ? '活动模型:' : 'Active Model:'}</span>
            <span className="text-indigo-400 bg-[#090D16] border border-[#1E293B] px-2 py-0.5 font-bold rounded">
              {models.find(m => m.id === selectedModelId)?.name || 'Default fallback'}
            </span>
          </div>
        </div>

        {/* Message container scroll pane */}
        <div 
          className="flex-1 overflow-y-auto p-5 space-y-4 bg-[#090D16] focus:outline-none scrollbar-thin scrollbar-thumb-slate-800" 
          ref={scrollRef}
          style={{ scrollBehavior: 'smooth' }}
        >
          {messages.length === 0 ? (
            <div className="h-full flex flex-col items-center justify-center text-center p-8 max-w-lg mx-auto space-y-5 mt-12 select-none">
              <div className="p-3.5 bg-indigo-500/10 text-indigo-400 border border-indigo-500/20 rounded-xl shadow-sm animate-bounce">
                <Sparkles className="h-6 w-6" />
              </div>
              <p className="text-xs text-slate-400 leading-relaxed font-sans font-bold">
                {t('noMessageYet')}
              </p>

              {/* Sample helper cards */}
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 w-full pt-4">
                <button
                  id="pg-suggest-btn-1"
                  onClick={() => handleQuickPrompt(lang === 'zh' ? '来一个简单的 Python 快排算法测试' : 'Write a quick python quicksort method to test code generation')}
                  className="p-3.5 bg-[#0F172A] border border-[#1E293B] hover:bg-[#131B2E] text-[11px] text-left text-slate-200 rounded-xl transition-all font-sans leading-snug cursor-pointer shadow-sm"
                >
                  <span className="text-indigo-400 font-bold block mb-0.5 font-mono">💻 {lang === 'zh' ? '代码生成测试' : 'Code Generation'}</span>
                  {lang === 'zh' ? '“测试下发 Python 快排...”' : '“Write quicksort function...”'}
                </button>
                <button
                  id="pg-suggest-btn-2"
                  onClick={() => handleQuickPrompt(lang === 'zh' ? '我们的架构里，怎么和 AI 代理进行格式自适应？' : 'How does the adaptive prompt format conversion work?')}
                  className="p-3.5 bg-[#0F172A] border border-[#1E293B] hover:bg-[#131B2E] text-[11px] text-left text-slate-200 rounded-xl transition-all font-sans leading-snug cursor-pointer shadow-sm"
                >
                  <span className="text-indigo-400 font-bold block mb-0.5 font-mono">🧠 {lang === 'zh' ? '参数与自适应' : 'Parameters Adaptor'}</span>
                  {lang === 'zh' ? '“了解智能协议格式对应方式...”' : '“Explain prompt formatting...”'}
                </button>
              </div>
            </div>
          ) : (
            messages.map((msg) => {
              const isUser = msg.sender === 'user';
              
              return (
                <div
                  id={`playground-message-${msg.id}`}
                  key={msg.id}
                  className={`flex gap-3.5 p-1 ${isUser ? 'justify-end' : 'justify-start'}`}
                >
                  {!isUser && (
                    <div className="h-9 w-9 rounded-full bg-indigo-600/10 border border-indigo-500/20 text-indigo-400 flex items-center justify-center shrink-0 shadow-sm">
                      <Bot className="h-4.5 w-4.5" />
                    </div>
                  )}

                  <div className="space-y-1 max-w-[84%] flex flex-col">
                    {/* Timestamp & Tag info */}
                    <div className={`flex items-center gap-1.5 text-[9.5px] text-slate-500 font-mono ${isUser ? 'justify-end' : 'justify-start'}`}>
                      <span className="font-semibold">{msg.timestamp}</span>
                      {!isUser && msg.latency && (
                        <span className="text-emerald-400 font-bold flex items-center gap-0.5">
                          ● {msg.latency}
                        </span>
                      )}
                      {!isUser && msg.formatConverted && (
                        <span className="text-indigo-400 font-bold flex items-center gap-0.5 bg-indigo-500/10 border border-indigo-500/20 px-1 text-[9px] rounded">
                          ADAPTED
                        </span>
                      )}
                    </div>

                    {/* Chat Bubble card container */}
                    <div className={`p-3.5 border text-[12px] leading-relaxed select-text shadow-sm rounded-xl ${
                      isUser
                        ? 'bg-indigo-600/15 text-slate-100 border-indigo-500/30'
                        : 'bg-[#131B2E] text-slate-200 border-[#1E293B] whitespace-pre-wrap'
                    }`}>
                      {msg.content}
                    </div>
                  </div>

                  {isUser && (
                    <div className="h-9 w-9 rounded-full bg-slate-800 border border-slate-700 text-slate-350 flex items-center justify-center shrink-0 shadow-sm">
                      <User className="h-4.5 w-4.5" />
                    </div>
                  )}
                </div>
              );
            })
          )}

          {/* Typing / Thinking Placeholder Bubble */}
          {isTyping && (
            <div id="pg-typing-placeholder" className="flex gap-3.5 p-1 justify-start">
              <div className="h-9 w-9 rounded-full bg-indigo-600/10 border border-indigo-500/20 text-indigo-400 flex items-center justify-center shrink-0 animate-pulse">
                <Bot className="h-4.5 w-4.5" />
              </div>
              <div className="space-y-1 max-w-xs">
                <div className="text-[10px] text-slate-400 font-mono font-bold">
                  {lang === 'zh' ? '云端端点交互进行中...' : 'Accessing Cloud API ...'}
                </div>
                <div className="p-3 bg-[#131B2E] border border-[#1E293B] rounded-xl flex items-center gap-1.5 py-3.5 px-4 shadow-sm">
                  <span className="h-1.5 w-1.5 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                  <span className="h-1.5 w-1.5 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                  <span className="h-1.5 w-1.5 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Input area */}
        <form 
          id="playground-chat-input-form"
          onSubmit={handleSend} 
          className="p-4 bg-[#131B2E]/40 border-t border-[#1E293B] flex items-center gap-3 shrink-0"
        >
          <input
            id="pg-chat-input-field"
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            disabled={isTyping || currentModels.length === 0}
            placeholder={
              currentModels.length > 0 
                ? t('typeMessagePlaceholder') 
                : (lang === 'zh' ? '请先在左侧选择/添加有效的云端服务商和模型' : 'Please select a valid provider/model first')
            }
            className="flex-1 bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] focus:border-indigo-500 rounded-lg p-2.5 text-xs focus:outline-none placeholder:text-slate-600 font-sans disabled:opacity-40"
          />
          <button
            id="pg-send-submit-btn"
            type="submit"
            disabled={!input.trim() || isTyping || currentModels.length === 0}
            className="flex items-center justify-center shrink-0 p-2.5 bg-indigo-600 hover:bg-indigo-500 disabled:bg-slate-800 disabled:text-slate-600 border border-indigo-500/20 text-white rounded-lg transition-colors cursor-pointer disabled:cursor-not-allowed shrink-0 h-10 w-10 shadow-sm"
          >
            <Send className="h-4.5 w-4.5" />
          </button>
        </form>
      </div>
    </div>
  );
}
