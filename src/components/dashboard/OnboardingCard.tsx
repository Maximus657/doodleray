import { Zap, Link, Loader2, ClipboardPaste } from 'lucide-react';

interface Props {
  quickInput: string;
  setQuickInput: (v: string) => void;
  onQuickAdd: () => void;
  onQuickPaste: () => void;
  importing: boolean;
  t: (key: any) => string;
}

export default function OnboardingCard({ quickInput, setQuickInput, onQuickAdd, onQuickPaste, importing, t }: Props) {
  return (
    <div className="flex-1 flex flex-col items-center justify-center w-full max-w-md relative z-10 animate-slide-up gap-6 py-10">
      {/* Logo */}
      <div className="flex items-center gap-3 mb-2">
        <div className="w-14 h-14 bg-black rounded-2xl flex items-center justify-center border-[3px] border-black shadow-[4px_4px_0_#000]">
          <Zap className="w-7 h-7 text-white stroke-[3px]" />
        </div>
        <div>
          <h1 className="text-3xl font-black text-black tracking-tighter uppercase leading-none">DoodleRay</h1>
          <p className="text-[10px] font-black text-black/40 uppercase tracking-widest">Fast & Secure VPN</p>
        </div>
      </div>

      {/* Big onboarding card */}
      <div className="w-full bg-white border-[4px] border-black rounded-3xl p-8 shadow-[8px_8px_0_#000] space-y-5">
        <div className="text-center space-y-2">
          <h2 className="text-xl font-black text-black uppercase tracking-tight">{t('welcome')}</h2>
          <p className="text-xs font-bold text-black/50 uppercase tracking-widest">
            {t('welcomeHint')}
          </p>
        </div>

        <div className="flex gap-2">
          <input
            type="text"
            value={quickInput}
            onChange={(e) => setQuickInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && onQuickAdd()}
            autoFocus
            placeholder={t('pasteHint')}
            className="flex-1 bg-gray-50 border-[3px] border-black rounded-xl px-4 py-4 text-sm text-black placeholder:text-black/25 focus:outline-none focus:shadow-[2px_2px_0_#000] font-bold tracking-tight transition-shadow"
          />
          <button
            onClick={onQuickPaste}
            className="group p-4 bg-white border-[3px] border-black rounded-xl shadow-[2px_2px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none cursor-pointer transition-all hover:bg-gray-50"
            title="Paste from clipboard"
          >
            <ClipboardPaste className="w-5 h-5 text-black stroke-[2.5px] transition-transform duration-300 group-hover:scale-110 group-hover:-rotate-6" />
          </button>
        </div>

        <button
          onClick={onQuickAdd}
          disabled={importing || !quickInput.trim()}
          className="w-full py-4 bg-black text-white border-[3px] border-black rounded-2xl text-sm font-black uppercase tracking-widest cursor-pointer shadow-[6px_6px_0_#000] hover:-translate-y-1 hover:-translate-x-1 hover:shadow-[8px_8px_0_#000] active:translate-x-[4px] active:translate-y-[4px] active:shadow-none transition-all disabled:opacity-40 disabled:cursor-not-allowed flex items-center justify-center gap-2"
        >
          {importing ? (
            <><Loader2 className="w-5 h-5 animate-spin" /> {t('loading')}</>
          ) : (
            <><Link className="w-5 h-5 stroke-[3px]" /> {t('connect')}</>
          )}
        </button>
      </div>
    </div>
  );
}
