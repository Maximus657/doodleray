export default function RetroBackground() {
  return (
    <div className="absolute inset-0 z-0 overflow-hidden pointer-events-none select-none">
      <div className="absolute inset-0 flex items-center justify-center opacity-10">
        <img src="/assets/mascot.png" alt=""
          className="h-[85vh] w-auto max-w-none drop-shadow-2xl"
          draggable={false} />
      </div>
      <span className="absolute top-4 left-4 text-lg font-black tracking-tight text-black/30">
        DOODLERAY
      </span>
    </div>
  );
}
