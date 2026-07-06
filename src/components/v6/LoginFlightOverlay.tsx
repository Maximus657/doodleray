import { useRef } from 'react';
import gsap from 'gsap';
import { useGSAP } from '@gsap/react';

gsap.registerPlugin(useGSAP);

export default function LoginFlightOverlay() {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const iconRef = useRef<HTMLDivElement | null>(null);
  const washRef = useRef<HTMLSpanElement | null>(null);

  useGSAP(() => {
    const root = rootRef.current;
    const icon = iconRef.current;
    const wash = washRef.current;
    if (!root || !icon || !wash) return;

    const brandLogo = document.querySelector<HTMLElement>('[data-v6-brand-logo]');
    const brandWord = document.querySelector<HTMLElement>('.v6-brand-word');
    const iconSize = 72;
    const targetRect = brandLogo?.getBoundingClientRect();
    const targetScale = targetRect ? targetRect.width / iconSize : 34 / iconSize;
    const startX = window.innerWidth / 2 - iconSize / 2;
    const startY = window.innerHeight / 2 - iconSize / 2;
    const endX = targetRect ? targetRect.left + targetRect.width / 2 - iconSize / 2 : 30;
    const endY = targetRect ? targetRect.top + targetRect.height / 2 - iconSize / 2 : 26;
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    if (reducedMotion) {
      gsap.set([brandLogo, brandWord].filter(Boolean), { clearProps: 'all', autoAlpha: 1 });
      gsap.set(root, { autoAlpha: 0 });
      return;
    }

    gsap.set(root, { autoAlpha: 1 });
    gsap.set(brandLogo, {
      autoAlpha: 0,
      scale: 0.94,
      transformOrigin: '50% 50%',
    });
    gsap.set(brandWord, {
      autoAlpha: 0,
      x: -9,
      clipPath: 'inset(0 100% 0 0)',
    });
    gsap.set(icon, {
      x: startX,
      y: startY,
      scale: 1.16,
      autoAlpha: 0,
      transformOrigin: '50% 50%',
    });
    gsap.set(wash, { autoAlpha: 0, scale: 0.96, transformOrigin: '50% 48%' });

    const tl = gsap.timeline({
      defaults: { ease: 'power3.out' },
    });

    tl.to(wash, { autoAlpha: 1, scale: 1, duration: 0.22 }, 0)
      .to(icon, { autoAlpha: 1, scale: 1.3, duration: 0.18 }, 0.03)
      .to(icon, { scale: 1.04, duration: 0.28, ease: 'expo.out' }, 0.19)
      .to(icon, {
        x: endX,
        y: endY,
        scale: targetScale,
        duration: 0.72,
        ease: 'power4.inOut',
      }, 0.42)
      .to(wash, { autoAlpha: 0, scale: 1.04, duration: 0.42, ease: 'power2.out' }, 0.58)
      .to(brandLogo, {
        autoAlpha: 1,
        scale: 1,
        duration: 0.14,
        ease: 'power2.out',
      }, 1.12)
      .to(icon, { autoAlpha: 0, duration: 0.18 }, 1.16)
      .to(brandWord, {
        autoAlpha: 1,
        x: 0,
        clipPath: 'inset(0 0% 0 0)',
        duration: 0.4,
        ease: 'power3.out',
      }, 1.22)
      .to(root, { autoAlpha: 0, duration: 0.14 }, 1.52)
      .set([brandLogo, brandWord].filter(Boolean), {
        clearProps: 'opacity,visibility,transform,filter,clipPath',
      }, 2.04);
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
      </div>
    </div>
  );
}
