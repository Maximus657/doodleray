import { useState, useCallback } from 'react';
import { Globe, SplitSquareHorizontal, AppWindow, Plus, X, Search, Shield } from 'lucide-react';
import { useAppStore } from '../stores/app-store';
import type { RoutingMode } from '../stores/app-store';

const MODES: { id: RoutingMode; label: string; desc: string; icon: typeof Globe; badge?: string }[] = [
  { id: 'global', label: 'Global Proxy', desc: 'All traffic through VPN — maximum security', icon: Globe, badge: 'SECURE' },
  { id: 'bypass', label: 'Smart Bypass', desc: 'Local sites go direct, rest via VPN', icon: SplitSquareHorizontal, badge: 'RECOMMENDED' },
  { id: 'app-split', label: 'Per-App Split', desc: 'Choose which apps bypass the VPN', icon: AppWindow, badge: 'ADVANCED' },
];

// Common Windows apps that users might want to bypass
const SUGGESTED_APPS = [
  { name: 'Steam', exe: 'steam.exe', icon: '🎮' },
  { name: 'Discord', exe: 'Discord.exe', icon: '💬' },
  { name: 'Spotify', exe: 'Spotify.exe', icon: '🎵' },
  { name: 'Telegram', exe: 'Telegram.exe', icon: '📩' },
  { name: 'VS Code', exe: 'Code.exe', icon: '💻' },
  { name: 'Chrome', exe: 'chrome.exe', icon: '🌐' },
  { name: 'Firefox', exe: 'firefox.exe', icon: '🦊' },
  { name: 'Slack', exe: 'slack.exe', icon: '💼' },
  { name: 'OBS Studio', exe: 'obs64.exe', icon: '📹' },
  { name: 'Zoom', exe: 'Zoom.exe', icon: '📞' },
  { name: 'Microsoft Teams', exe: 'ms-teams.exe', icon: '👥' },
  { name: 'WhatsApp', exe: 'WhatsApp.exe', icon: '📱' },
];

export default function Routing() {
  const { routingMode, setRoutingMode, bypassApps, addBypassApp, removeBypassApp, addLog } = useAppStore();
  const [customApp, setCustomApp] = useState('');
  const [searchFilter, setSearchFilter] = useState('');

  const handleAddCustom = useCallback(() => {
    const app = customApp.trim();
    if (app && !bypassApps.includes(app)) {
      addBypassApp(app);
      addLog('info', `Added ${app} to bypass list`);
      setCustomApp('');
    }
  }, [customApp, bypassApps, addBypassApp, addLog]);

  const handleToggleSuggested = useCallback((exe: string, name: string) => {
    if (bypassApps.includes(exe)) {
      removeBypassApp(exe);
      addLog('info', `Removed ${name} from bypass list`);
    } else {
      addBypassApp(exe);
      addLog('info', `Added ${name} to bypass list`);
    }
  }, [bypassApps, addBypassApp, removeBypassApp, addLog]);

  const filteredSuggested = SUGGESTED_APPS.filter(app =>
    app.name.toLowerCase().includes(searchFilter.toLowerCase()) ||
    app.exe.toLowerCase().includes(searchFilter.toLowerCase())
  );

  return (
    <div className="flex-1 p-5 overflow-y-auto animate-fade-in">
      <div className="max-w-xl mx-auto space-y-6 pt-4">
        <h1 className="text-2xl font-black text-text-on-orange flex items-center gap-3 drop-shadow-sm mb-6">
          <span className="p-2.5 bg-el-primary text-text-on-dark rounded-xl shadow-md"><Shield className="w-5 h-5" /></span>
          Traffic Routing
        </h1>

        {/* Mode Selector */}
        <div className="space-y-2">
          {MODES.map(({ id, label, desc, icon: Icon, badge }) => (
            <button key={id} onClick={() => setRoutingMode(id)}
              className={`w-full rounded-2xl p-4 text-left flex items-center gap-4 transition-all duration-200 cursor-pointer
                ${routingMode === id ? 'card shadow-xl scale-[1.01]' : 'bg-black/10 hover:bg-black/15 border border-black/5'}
              `}>
              <div className={`w-10 h-10 rounded-lg flex items-center justify-center shrink-0 transition-colors
                ${routingMode === id ? 'bg-bg-primary text-el-primary' : 'bg-black/10 text-text-on-orange-muted'}`}>
                <Icon className="w-5 h-5" />
              </div>
              <div className="flex-1">
                <div className="flex items-center gap-2">
                  <p className={`text-sm font-bold ${routingMode === id ? 'text-text-on-dark' : 'text-text-on-orange'}`}>{label}</p>
                  {badge && (
                    <span className={`text-[8px] font-black px-1.5 py-0.5 rounded-full tracking-wider
                      ${routingMode === id ? 'bg-el-primary/30 text-el-primary' : 'bg-black/10 text-text-on-orange-muted'}`}>
                      {badge}
                    </span>
                  )}
                </div>
                <p className={`text-xs mt-0.5 ${routingMode === id ? 'text-text-on-dark-muted' : 'text-text-on-orange-muted'}`}>{desc}</p>
              </div>
              <div className={`w-4 h-4 rounded-full border-2 flex items-center justify-center
                ${routingMode === id ? 'border-bg-primary' : 'border-text-on-orange-muted'}`}>
                {routingMode === id && <div className="w-2 h-2 rounded-full bg-bg-primary" />}
              </div>
            </button>
          ))}
        </div>

        {/* Per-App Split Tunneling Panel */}
        {routingMode === 'app-split' && (
          <div className="card rounded-2xl p-5 space-y-4 animate-slide-up">
            <h3 className="text-sm font-black text-text-on-dark flex items-center gap-2">
              <AppWindow className="w-4 h-4 text-el-primary" />
              Apps that BYPASS the VPN
            </h3>
            <p className="text-xs text-text-on-dark-muted">
              Selected apps will connect directly, everything else goes through VPN.
            </p>

            {/* Search + Custom Add */}
            <div className="flex gap-2">
              <div className="flex-1 relative">
                <Search className="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-text-on-dark-muted" />
                <input
                  type="text"
                  value={searchFilter}
                  onChange={(e) => setSearchFilter(e.target.value)}
                  placeholder="Search apps..."
                  className="w-full bg-white/10 border border-white/10 rounded-lg pl-9 pr-3 py-2 text-xs text-text-on-dark placeholder:text-text-on-dark-muted focus:outline-none"
                />
              </div>
            </div>

            {/* Suggested Apps Grid */}
            <div className="grid grid-cols-2 gap-1.5 max-h-48 overflow-y-auto pr-1">
              {filteredSuggested.map((app) => {
                const isActive = bypassApps.includes(app.exe);
                return (
                  <button
                    key={app.exe}
                    onClick={() => handleToggleSuggested(app.exe, app.name)}
                    className={`flex items-center gap-2 px-3 py-2 rounded-lg text-xs font-bold transition-all cursor-pointer
                      ${isActive ? 'bg-el-primary/20 text-el-primary border border-el-primary/30' : 'bg-white/5 text-text-on-dark-muted hover:bg-white/10 border border-white/5'}
                    `}
                  >
                    <span className="text-sm">{app.icon}</span>
                    <span className="truncate">{app.name}</span>
                  </button>
                );
              })}
            </div>

            {/* Custom exe input */}
            <div className="flex gap-2">
              <input
                type="text"
                value={customApp}
                onChange={(e) => setCustomApp(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleAddCustom()}
                placeholder="custom-app.exe"
                className="flex-1 bg-white/10 border border-white/10 rounded-lg px-3 py-2 text-xs text-text-on-dark placeholder:text-text-on-dark-muted focus:outline-none font-mono"
              />
              <button
                onClick={handleAddCustom}
                className="px-3 py-2 bg-el-primary/20 text-el-primary hover:bg-el-primary/30 rounded-lg text-xs font-bold cursor-pointer transition-colors"
              >
                <Plus className="w-3.5 h-3.5" />
              </button>
            </div>

            {/* Active bypass list */}
            {bypassApps.length > 0 && (
              <div className="space-y-1 pt-2 border-t border-white/10">
                <p className="text-[10px] font-black text-text-on-dark-muted uppercase tracking-widest">Active Bypasses ({bypassApps.length})</p>
                <div className="flex flex-wrap gap-1.5">
                  {bypassApps.map((app) => (
                    <span key={app} className="inline-flex items-center gap-1 bg-white/10 text-text-on-dark text-[10px] font-mono px-2 py-1 rounded-lg">
                      {app}
                      <button onClick={() => removeBypassApp(app)} className="text-text-on-dark-muted hover:text-danger cursor-pointer">
                        <X className="w-3 h-3" />
                      </button>
                    </span>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
