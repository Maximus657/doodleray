import { Globe } from 'lucide-react';
import lightningIcon from '@lobehub/fluent-emoji-3d/assets/26a1.webp';
import satelliteIcon from '@lobehub/fluent-emoji-3d/assets/1f4e1.webp';
import repeatIcon from '@lobehub/fluent-emoji-3d/assets/1f501.webp';
import deFlag from 'flag-icons/flags/4x3/de.svg';
import kzFlag from 'flag-icons/flags/4x3/kz.svg';
import nlFlag from 'flag-icons/flags/4x3/nl.svg';
import ruFlag from 'flag-icons/flags/4x3/ru.svg';
import usFlag from 'flag-icons/flags/4x3/us.svg';
import type { ServerConfig } from '../../stores/app-store';
import { useAppStore } from '../../stores/app-store';
import { useTranslation } from '../../locales';
import { closedLocationIdFromServer } from '../../lib/app-control-plane';
import { displayLocationTitle, localizedCountryName } from '../../lib/ui-format';

const LOCAL_EMOJI_ASSETS: Record<string, string> = {
  '⚡': lightningIcon,
  '📡': satelliteIcon,
  '🔁': repeatIcon,
};
const LOCAL_FLAG_ASSETS: Record<string, string> = {
  de: deFlag,
  kz: kzFlag,
  nl: nlFlag,
  ru: ruFlag,
  us: usFlag,
};
const LEADING_EMOJI_RE = /^((?:\p{Regional_Indicator}{2})|(?:\p{Extended_Pictographic}(?:\uFE0F|\uFE0E)?\p{Emoji_Modifier}?(?:\u200D\p{Extended_Pictographic}(?:\uFE0F|\uFE0E)?\p{Emoji_Modifier}?)*))/u;

/** Country marker rendered entirely on-device; it never leaks pre-tunnel metadata to a CDN. */
export function FlagIcon({ countryCode, size = 26 }: { countryCode?: string; size?: number }) {
  const cc = countryCode?.trim().toLowerCase();
  if (!cc || !/^[a-z]{2}$/.test(cc)) {
    return <Globe className="text-white/55" style={{ width: size * 0.77, height: size * 0.77 }} strokeWidth={1.8} />;
  }
  const src = LOCAL_FLAG_ASSETS[cc];
  if (!src) {
    return (
      <span
        aria-hidden
        className="inline-flex items-center justify-center rounded-[5px] border border-white/15 bg-white/10 font-bold tracking-[0.04em] text-white/85"
        style={{ width: size, height: Math.round(size * 0.75), fontSize: Math.max(9, Math.round(size * 0.38)) }}
      >
        {cc.toUpperCase()}
      </span>
    );
  }

  return (
    <img
      src={src}
      alt=""
      width={size}
      height={Math.round(size * 0.75)}
      className="rounded-[4px] object-cover shadow-[0_2px_4px_rgba(0,0,0,0.35)]"
      style={{ width: size, height: Math.round(size * 0.75) }}
    />
  );
}

function isValidCountryCode(countryCode?: string): boolean {
  return /^[a-z]{2}$/i.test(countryCode?.trim() ?? '');
}

function splitLeadingEmoji(rawName: string): { emoji: string | null; name: string } {
  const leftTrimmed = rawName.trimStart();
  const emoji = leftTrimmed.match(LEADING_EMOJI_RE)?.[0] ?? null;
  if (!emoji || !leftTrimmed.startsWith(emoji)) return { emoji: null, name: rawName.trim() };

  const name = leftTrimmed
    .slice(emoji.length)
    .replace(/^[\s·|:—–-]+/, '')
    .trim();
  return { emoji, name: name || rawName.trim() };
}

function LeadingEmojiIcon({ emoji, size }: { emoji: string; size: number }) {
  const src = LOCAL_EMOJI_ASSETS[emoji] ?? null;

  if (src) {
    return (
      <img
        src={src}
        alt=""
        width={size}
        height={size}
        className="object-contain drop-shadow-[0_2px_5px_rgba(0,0,0,0.34)]"
        style={{ width: size, height: size }}
      />
    );
  }

  return (
    <span
      aria-hidden
      className="inline-flex items-center justify-center drop-shadow-[0_2px_5px_rgba(0,0,0,0.34)]"
      style={{ width: size, height: size, fontSize: Math.round(size * 0.82), lineHeight: 1 }}
    >
      {emoji}
    </span>
  );
}

export function leadingServerEmoji(server: Pick<ServerConfig, 'name' | 'countryCode'>): string | null {
  if (isValidCountryCode(server.countryCode)) return null;
  return splitLeadingEmoji(server.name).emoji;
}

export function ServerIcon({ server, size = 26 }: { server: Pick<ServerConfig, 'name' | 'countryCode'>; size?: number }) {
  const emoji = leadingServerEmoji(server);
  if (emoji) return <LeadingEmojiIcon emoji={emoji} size={size} />;
  return <FlagIcon countryCode={server.countryCode} size={size} />;
}

interface Props {
  server: ServerConfig;
  active: boolean;
  onSelect: (server: ServerConfig) => void;
}

/**
 * Country names come from the backend baked into `server.name` at fetch
 * time, in whatever language was active then — they never get re-localized
 * on a later language change (matches the "Auto" row's staleness problem,
 * fixed there via a reactive t() call). `language`, when passed, re-derives
 * the name live from the ISO country code instead of trusting the stale
 * baked-in string. Special control-plane locations follow the same rule:
 * their backend titles are not UI translations.
 */
export function displayServerName(
  server: Pick<ServerConfig, 'id' | 'name' | 'countryCode'>,
  language: string | undefined,
  t: (key: never) => string,
): string {
  switch (closedLocationIdFromServer(server)) {
    case 'auto': return t('v6AutoLocationName' as never);
    case 'bypass': return t('v6BypassLocationName' as never);
    case 'reserve': return t('v6ReserveLocationName' as never);
  }
  const localized = localizedCountryName(server.countryCode, language);
  if (localized) return localized;
  const name = isValidCountryCode(server.countryCode) ? server.name : splitLeadingEmoji(server.name).name;
  return displayLocationTitle(name, server.countryCode);
}

/** Design location row: flag + user-facing location name. */
export default function ServerRow({ server, active, onSelect }: Props) {
  const { t } = useTranslation();
  const language = useAppStore((state) => state.language);
  const auto = closedLocationIdFromServer(server) === 'auto';
  const name = displayServerName(server, language, t);

  return (
    <button
      type="button"
      role="option"
      aria-selected={active}
      onClick={() => onSelect(server)}
      className="flex w-full shrink-0 items-center gap-3 rounded-[15px] px-[13px] py-[11px] text-left transition-[background,border-color] duration-150 v6-focus"
      style={{
        background: active ? 'linear-gradient(110deg, rgba(249,127,22,0.22), rgba(249,127,22,0.08))' : 'rgba(255,255,255,0.02)',
        border: active ? '1px solid rgba(255,158,56,0.45)' : '1px solid rgba(255,255,255,0.05)',
        boxShadow: active ? '0 6px 20px rgba(234,109,6,0.18)' : 'none',
      }}
    >
      <span className="flex w-9 shrink-0 items-center justify-center">
        <ServerIcon server={server} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14.5px] font-medium text-white" title={name}>{name}</span>
      </span>
      {auto && (
        <span className="flex shrink-0 items-center justify-end gap-[7px]">
          <span className="whitespace-nowrap text-[12px] font-medium text-[#ffb02e]">{t('v6AutoBest' as never)}</span>
        </span>
      )}
    </button>
  );
}
