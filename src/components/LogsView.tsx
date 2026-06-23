import React, { useState, useMemo, useRef, useEffect } from 'react';
import { 
  Terminal, 
  Search, 
  Trash2, 
  Download, 
  Copy, 
  Clock, 
  Check
} from 'lucide-react';
import { LogEntry, LogLevel } from '../types';
import { Locale } from '../locales';

interface LogsViewProps {
  logs: LogEntry[];
  onClearLogs: () => void;
  onAddLog: (level: LogLevel, source: string, message: string) => void;
  t: (key: any) => string;
  lang: Locale;
}

export default function LogsView({
  logs,
  onClearLogs,
  onAddLog,
  t,
  lang
}: LogsViewProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedLevel, setSelectedLevel] = useState<LogLevel | 'all'>('all');
  const [autoScroll, setAutoScroll] = useState(true);
  const [copied, setCopied] = useState(false);

  const endOfLogsRef = useRef<HTMLDivElement>(null);

  // Filter logs
  const filteredLogs = useMemo(() => {
    return logs.filter(log => {
      const matchesSearch = log.message.toLowerCase().includes(searchQuery.toLowerCase()) || 
                            log.source.toLowerCase().includes(searchQuery.toLowerCase());
      const matchesLevel = selectedLevel === 'all' ? true : log.level === selectedLevel;
      return matchesSearch && matchesLevel;
    });
  }, [logs, searchQuery, selectedLevel]);

  // Handle Autoscrolling to bottom when logs updates
  useEffect(() => {
    if (autoScroll && endOfLogsRef.current) {
      endOfLogsRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [logs, autoScroll]);

  const handleCopyLogs = () => {
    const logText = logs.map(l => `[${l.timestamp}] [${l.level.toUpperCase()}] [${l.source}]: ${l.message}`).join('\n');
    navigator.clipboard.writeText(logText);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleDownloadLogs = () => {
    const logText = logs.map(l => `[${l.timestamp}] [${l.level.toUpperCase()}] [${l.source}]: ${l.message}`).join('\n');
    const blob = new Blob([logText], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `codex-orchestration-logs-${Date.now()}.log`;
    link.click();
    URL.revokeObjectURL(url);
  };

  const levelBadges: Record<LogLevel, string> = {
    info: 'bg-indigo-500/10 text-indigo-400 border border-indigo-500/20',
    warn: 'bg-amber-550/10 text-amber-400 border border-amber-500/20',
    error: 'bg-rose-500/10 text-rose-450 border border-rose-500/20',
    debug: 'bg-slate-800 text-slate-400 border border-[#1E293B]'
  };

  return (
    <div id="logs-tab-panel" className="space-y-4 h-[calc(100vh-170px)] flex flex-col text-slate-150 font-sans">
      
      {/* Filters & Actions Panel */}
      <div className="p-4 bg-[#0F172A] border border-[#1E293B] rounded-2xl shadow-sm flex flex-col md:flex-row md:items-center md:justify-between justify-start gap-4 shrink-0">
        
        {/* Search & Severity filters */}
        <div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-3 flex-1">
          {/* Search */}
          <div className="relative min-w-[200px] sm:max-w-xs flex-1">
            <Search className="absolute left-3 top-2.5 w-3.5 h-3.5 text-slate-500" />
            <input
              id="log-search-field"
              type="text"
              placeholder={t('filterLogs')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-[#090D16] text-[#F1F5F9] border border-[#1E293B] rounded-lg py-2 pl-9 pr-3 text-xs focus:outline-none placeholder-slate-600 font-mono"
            />
          </div>

          {/* Level Filter Buttons */}
          <div className="flex items-center gap-1 bg-[#090D16] p-1 border border-[#1E293B] rounded-lg shrink-0">
            {(['all', 'info', 'warn', 'error', 'debug'] as const).map((lvl) => {
              const isSelected = selectedLevel === lvl;
              return (
                <button
                  id={`filter-level-btn-${lvl}`}
                  key={lvl}
                  onClick={() => setSelectedLevel(lvl)}
                  className={`text-[10px] font-mono font-bold px-2.5 py-1 rounded-md transition-all capitalize cursor-pointer ${
                    isSelected
                      ? 'bg-indigo-600 text-white shadow-sm'
                      : 'text-slate-500 hover:text-slate-300'
                  }`}
                >
                  {lvl === 'all' ? (lang === 'zh' ? '全量' : 'all') : lvl}
                </button>
              );
            })}
          </div>
        </div>

        {/* Toolbar of commands */}
        <div className="flex flex-wrap items-center gap-2 shrink-0">
          <button
            id="btn-toggle-log-autoscroll"
            onClick={() => setAutoScroll(!autoScroll)}
            className={`flex items-center gap-1 px-3 py-1.5 border rounded-lg text-xs font-mono font-bold transition-all cursor-pointer ${
              autoScroll 
                ? 'bg-indigo-605/10 text-indigo-400 border-indigo-500/25' 
                : 'text-slate-400 bg-slate-800 border-[#1E293B]'
            }`}
          >
            <Clock className="w-3.5 h-3.5" />
            <span>{autoScroll ? t('lockBottom') : t('pauseScroll')}</span>
          </button>

          <button
            id="btn-copy-all-logs"
            onClick={handleCopyLogs}
            className="flex items-center gap-1.5 bg-slate-850 hover:bg-slate-800 text-slate-300 border border-[#1E293B] rounded-lg px-3 py-1.5 text-xs font-semibold font-mono transition-colors cursor-pointer"
            title="Copy all items"
          >
            {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5 text-slate-400" />}
            <span>{copied ? t('copied') : t('copy')}</span>
          </button>

          <button
            id="btn-download-logs"
            onClick={handleDownloadLogs}
            className="flex items-center gap-1.5 bg-slate-850 hover:bg-slate-800 text-slate-300 border border-[#1E293B] rounded-lg px-3 py-1.5 text-xs font-semibold font-mono transition-colors cursor-pointer"
            title="Download log file"
          >
            <Download className="w-3.5 h-3.5 text-slate-400" />
            <span>{t('export')}</span>
          </button>

          <button
            id="btn-clear-logs"
            onClick={onClearLogs}
            className="p-1.5 hover:bg-rose-950/40 border border-[#1E293B] text-slate-400 hover:text-rose-400 rounded-lg bg-slate-805 transition-colors cursor-pointer"
            title="Reset Terminal Logs"
          >
            <Trash2 className="w-4 h-4 text-rose-450" />
          </button>
        </div>

      </div>

      {/* Terminal View Panel */}
      <div className="flex-1 bg-[#090D16] border border-[#1E293B] rounded-2xl p-4.5 font-mono text-[11px] overflow-y-auto space-y-2 scrollbar-thin scrollbar-thumb-slate-800">
        {filteredLogs.length > 0 ? (
          filteredLogs.map((entry) => (
            <div id={`log-row-${entry.id}`} key={entry.id} className="flex items-start gap-3 hover:bg-slate-800/40 py-1 px-1.5 rounded transition-all">
              <span className="text-slate-500 shrink-0 select-none font-bold">
                {entry.timestamp.split(' ')[1] || entry.timestamp}
              </span>
              
              <span className={`px-1.5 py-0.2 rounded text-[10px] shrink-0 font-extrabold uppercase select-none ${levelBadges[entry.level]}`}>
                {entry.level}
              </span>

              <span className="text-indigo-400 font-extrabold shrink-0 min-w-[120px] max-w-[120px] truncate select-none border-r border-[#1E293B] pr-2">
                [{entry.source}]
              </span>

              <span className={`font-mono leading-relaxed break-all font-semibold ${
                entry.level === 'error' 
                  ? 'text-rose-400' 
                  : entry.level === 'warn'
                  ? 'text-amber-400'
                  : 'text-slate-300'
              }`}>
                {entry.message}
              </span>
            </div>
          ))
        ) : (
          <div className="h-full flex items-center justify-center text-slate-500 select-none p-8 text-center text-xs font-bold font-mono">
            {t('consoleTelemetry')}
          </div>
        )}
        <div ref={endOfLogsRef} />
      </div>

    </div>
  );
}
