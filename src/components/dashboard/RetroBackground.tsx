export default function RetroBackground() {
  return (
    <>
      <div className="absolute inset-0 overflow-hidden pointer-events-none select-none flex items-center justify-center opacity-10">
        <img src="/assets/mascot.png" alt=""
          className="h-[85vh] w-auto drop-shadow-2xl"
          draggable={false} />
      </div>
      <span className="absolute top-4 left-4 text-lg font-black tracking-tight text-black/30 select-none pointer-events-none z-10">
        DOODLERAY
      </span>
    </>
  );
}
