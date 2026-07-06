import { useRef } from 'react';
import gsap from 'gsap';
import { useGSAP } from '@gsap/react';

gsap.registerPlugin(useGSAP);

export default function LoginFlightOverlay() {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const iconRef = useRef<HTMLDivElement | null>(null);
  const wordRef = useRef<HTMLSpanElement | null>(null);
  const washRef = useRef<HTMLSpanElement | null>(null);

  useGSAP(() => {
    const root = rootRef.current;
    const icon = iconRef.current;
    const word = wordRef.current;
    const wash = washRef.current;
    if (!root || !icon || !word || !wash) return;

    const brandLogo = document.querySelector<HTMLElement>('[data-v6-brand-logo]');
    const iconSize = 72;
    const targetRect = brandLogo?.getBoundingClientRect();
    const targetScale = targetRect ? targetRect.width / iconSize : 34 / iconSize;
    const startX = window.innerWidth / 2 - iconSize / 2;
    const startY = window.innerHeight / 2 - iconSize / 2;
    const endX = targetRect ? targetRect.left + targetRect.width / 2 - iconSize / 2 : 30;
    const endY = targetRect ? targetRect.top + targetRect.height / 2 - iconSize / 2 : 26;
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    if (reducedMotion) {
      gsap.set(root, { autoAlpha: 0 });
      return;
    }

    gsap.set(root, { autoAlpha: 1 });
    gsap.set(icon, {
      x: startX,
      y: startY,
      scale: 1.16,
      autoAlpha: 0,
      filter: 'blur(14px)',
      transformOrigin: '50% 50%',
    });
    gsap.set(word, { autoAlpha: 0, x: -14, filter: 'blur(10px)' });
    gsap.set(wash, { autoAlpha: 0, scale: 0.96, transformOrigin: '50% 48%' });

    const tl = gsap.timeline({
      defaults: { ease: 'power3.out' },
    });

    tl.to(wash, { autoAlpha: 1, scale: 1, duration: 0.22 }, 0)
      .to(icon, { autoAlpha: 1, scale: 1.34, filter: 'blur(0px)', duration: 0.22 }, 0.03)
      .to(icon, { scale: 1.03, duration: 0.38, ease: 'expo.out' }, 0.22)
      .to(word, { autoAlpha: 1, x: 0, filter: 'blur(0px)', duration: 0.34 }, 0.42)
      .to(icon, {
        x: endX,
        y: endY,
        scale: targetScale,
        duration: 0.78,
        ease: 'power4.inOut',
      }, 0.64)
      .to(wash, { autoAlpha: 0, scale: 1.04, duration: 0.46, ease: 'power2.out' }, 0.74)
      .to(word, { autoAlpha: 0, x: 8, filter: 'blur(2px)', duration: 0.2 }, 1.22)
      .to(icon, { autoAlpha: 0, filter: 'blur(1px)', duration: 0.22 }, 1.28)
      .to(root, { autoAlpha: 0, duration: 0.14 }, 1.42);
  }, { scope: rootRef });

  return (
    <div ref={rootRef} aria-hidden className="v6-login-flight-stage pointer-events-none fixed inset-0 z-[80] overflow-visible">
      <span ref={washRef} className="v6-login-flight-wash" />
      <div ref={iconRef} className="v6-login-flight">
        <img
          src="/assets/mascot.png"
          alt=""
          draggable={false}
          className="h-full w-full rounded-[18px]"
        />
        <span ref={wordRef} className="v6-login-flight-word">
          Doodle<span>Ray</span>
        </span>
      </div>
    </div>
  );
}
