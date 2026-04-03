import { useEffect, useCallback } from 'react';

export interface ConfirmModalState {
  show: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
}

export const EMPTY_CONFIRM: ConfirmModalState = {
  show: false, title: '', message: '', onConfirm: () => {},
};

interface Props extends ConfirmModalState {
  cancelLabel?: string;
  onClose: () => void;
}

export default function ConfirmModal({ show, title, message, confirmLabel, cancelLabel, danger = true, onConfirm, onClose }: Props) {
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') onClose();
  }, [onClose]);

  useEffect(() => {
    if (show) {
      document.addEventListener('keydown', handleKeyDown);
      return () => document.removeEventListener('keydown', handleKeyDown);
    }
  }, [show, handleKeyDown]);

  if (!show) return null;

  return (
    <>
      <div className="fixed inset-0 z-[60] bg-black/40 backdrop-blur-sm" onClick={onClose} />
      <div className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-[70] w-72 bg-white border-[3px] border-black rounded-2xl p-5 shadow-[6px_6px_0_#000] animate-slide-up flex flex-col gap-4">
        <div>
          <h3 className="text-xs font-black uppercase tracking-widest leading-tight">{title}</h3>
          <p className="text-xs text-black/60 font-bold mt-2 leading-relaxed whitespace-pre-wrap">{message}</p>
        </div>
        <div className="flex gap-2 mt-2">
          <button
            onClick={onClose}
            className="flex-1 py-2 bg-white text-black border-[2px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer hover:bg-black/5 hover:-translate-y-0.5 active:translate-y-0 transition-all">
            {cancelLabel || 'Cancel'}
          </button>
          <button
            onClick={onConfirm}
            className={`flex-1 py-2 text-white border-[2px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer shadow-[2px_2px_0_#000] hover:shadow-[3px_3px_0_#000] hover:-translate-y-0.5 active:translate-y-1 active:shadow-none transition-all ${
              danger ? 'bg-danger' : 'bg-black'
            }`}>
            {confirmLabel || 'OK'}
          </button>
        </div>
      </div>
    </>
  );
}
