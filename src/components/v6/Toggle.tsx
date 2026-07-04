/** Design toggle switch (46x27 track, 21px knob, orange glow when on). */
export default function Toggle({ on, label }: { on: boolean; label?: string }) {
  return (
    <span
      role="switch"
      aria-checked={on}
      aria-label={label}
      className="relative block h-[27px] w-[46px] shrink-0 cursor-pointer rounded-[30px] transition-colors duration-200"
      style={{
        background: on ? '#FF6B2C' : 'rgba(255,255,255,0.15)',
        boxShadow: on ? '0 0 14px rgba(255,107,44,0.5)' : 'none',
      }}
    >
      <span
        className="absolute left-[3px] top-[3px] h-[21px] w-[21px] rounded-full bg-white transition-transform duration-200"
        style={{ transform: on ? 'translateX(19px)' : 'translateX(0)', boxShadow: '0 2px 5px rgba(0,0,0,0.3)' }}
      />
    </span>
  );
}
