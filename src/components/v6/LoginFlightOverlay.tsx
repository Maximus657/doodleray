import { useRef } from 'react';
import gsap from 'gsap';
import { useGSAP } from '@gsap/react';

gsap.registerPlugin(useGSAP);

type BrandLogoTargetRect = Pick<DOMRect, 'left' | 'top' | 'width' | 'height'>;

function measureBrandLogoTarget(brandLogo: HTMLElement | null): BrandLogoTargetRect | null {
  if (!brandLogo) return null;

  const animatedShell = brandLogo.closest<HTMLElement>('.v6-brand-enter');
  const touched = [animatedShell, brandLogo].filter((el): el is HTMLElement => !!el);
  const previous = touched.map((el) => ({
    el,
    animation: el.style.animation,
    transform: el.style.transform,
    transition: el.style.transition,
  }));

  touched.forEach((el) => {
    el.style.animation = 'none';
    el.style.transform = 'none';
    el.style.transition = 'none';
  });

  const measured = brandLogo.getBoundingClientRect();
  const rect = {
    left: measured.left,
    top: measured.top,
    width: measured.width,
    height: measured.height,
  };

  previous.forEach(({ el, animation, transform, transition }) => {
    el.style.animation = animation;
    el.style.transform = transform;
    el.style.transition = transition;
  });

  return rect;
}

export default function LoginFlightOverlay() {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const iconRef = useRef<HTMLDivElement | null>(null);
  const washRef = useRef<HTMLSpanElement | null>(null);

  useGSAP((_context, contextSafe) => {
    const root = rootRef.current;
    const icon = iconRef.current;
    const wash = washRef.current;
    if (!root || !icon || !wash) return;

    let timeline: ReturnType<typeof gsap.timeline> | null = null;
    let brandLogo: HTMLElement | null = null;
    let brandWord: HTMLElement | null = null;
    const runAnimation = contextSafe?.(() => {
      brandLogo = document.querySelector<HTMLElement>('[data-v6-brand-logo]');
      brandWord = document.querySelector<HTMLElement>('.v6-brand-word');
      const iconSize = 72;
      const targetRect = measureBrandLogoTarget(brandLogo);
      const targetScale = targetRect ? targetRect.width / iconSize : 34 / iconSize;
      const centerX = window.innerWidth / 2;
      const centerY = window.innerHeight / 2;
      const xForCenteredScale = (scale: number) => centerX - (iconSize * scale) / 2;
      const yForCenteredScale = (scale: number) => centerY - (iconSize * scale) / 2;
      const endX = targetRect ? targetRect.left : 30;
      const endY = targetRect ? targetRect.top : 26;
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
      x: xForCenteredScale(1.16),
      y: yForCenteredScale(1.16),
      scale: 1.16,
      autoAlpha: 0,
      transformOrigin: '0 0',
    });
      gsap.set(wash, { autoAlpha: 0, scale: 0.96, transformOrigin: '50% 48%' });

      const tl = gsap.timeline({
        defaults: { ease: 'power3.out' },
      });
      timeline = tl;

      tl.to(wash, { autoAlpha: 1, scale: 1, duration: 0.22 }, 0)
      .to(icon, { autoAlpha: 1, x: xForCenteredScale(1.3), y: yForCenteredScale(1.3), scale: 1.3, duration: 0.18 }, 0.03)
      .to(icon, { x: xForCenteredScale(1.04), y: yForCenteredScale(1.04), scale: 1.04, duration: 0.28, ease: 'expo.out' }, 0.19)
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
    }) ?? (() => undefined);
    const frame = window.requestAnimationFrame(runAnimation);

    return () => {
      window.cancelAnimationFrame(frame);
      timeline?.kill();
      gsap.set([brandLogo, brandWord].filter(Boolean), {
        clearProps: 'opacity,visibility,transform,filter,clipPath',
      });
    };
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
