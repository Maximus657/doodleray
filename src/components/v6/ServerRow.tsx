import { useState } from 'react';
import { Loader2, Globe } from 'lucide-react';
import { getEmoji, getFluentEmojiCDN } from '@lobehub/fluent-emoji';
import type { ServerConfig } from '../../stores/app-store';
import { useTranslation } from '../../locales';
import { isClosedAutoLocationServer } from '../../lib/app-control-plane';
import { displayLocationTitle } from '../../lib/ui-format';

function pingColor(p?: number): string {
  if (p === -1) return '#ff6b5a';
  if (p === undefined || p <= 0) return 'rgba(255,255,255,0.35)';
  if (p < 80) return '#3ddc84';
  if (p < 160) return '#ffb02e';
  return '#ff6b5a';
}

/**
 * Country flag as an image (flagcdn, already allowed by the CSP img-src).
 * Windows has no emoji flags — regional indicators render as bare letters —
 * so images are the only way to match the design. Falls back to a globe.
 */
export function FlagIcon({ countryCode, size = 26 }: { countryCode?: string; size?: number }) {
  const [failed, setFailed] = useState(false);
  const cc = countryCode?.trim().toLowerCase();
  if (!cc || !/^[a-z]{2}$/.test(cc) || failed) {
    return <Globe className="text-white/55" style={{ width: size * 0.77, height: size * 0.77 }} strokeWidth={1.8} />;
  }
  return (
    <img
      src={`https://flagcdn.com/w40/${cc}.png`}
      alt=""
      width={size}
      height={Math.round(size * 0.75)}
      loading="lazy"
      onError={() => setFailed(true)}
      className="rounded-[4px] object-cover"
      style={{ width: size, height: Math.round(size * 0.75), filter: 'drop-shadow(0 2px 4px rgba(0,0,0,0.35))' }}
    />
  );
}

function isValidCountryCode(countryCode?: string): boolean {
  return /^[a-z]{2}$/i.test(countryCode?.trim() ?? '');
}

function splitLeadingEmoji(rawName: string): { emoji: string | null; name: string } {
  const leftTrimmed = rawName.trimStart();
  const emoji = getEmoji(leftTrimmed);
  if (!emoji || !leftTrimmed.startsWith(emoji)) return { emoji: null, name: rawName.trim() };

  const name = leftTrimmed
    .slice(emoji.length)
    .replace(/^[\s·|:—–-]+/, '')
    .trim();
  return { emoji, name: name || rawName.trim() };
}

function LeadingEmojiIcon({ emoji, size }: { emoji: string; size: number }) {
  const [failed, setFailed] = useState(false);
  const src = failed
    ? null
    : getFluentEmojiCDN(emoji, { cdn: 'unpkg', type: '3d' });

  if (!src) {
    return (
      <span
        aria-hidden
        className="inline-flex items-center justify-center"
        style={{ width: size, height: size, fontSize: Math.round(size * 0.82), lineHeight: 1 }}
      >
        {emoji}
      </span>
    );
  }

  return (
    <img
      src={src}
      alt=""
      width={size}
      height={size}
      loading="lazy"
      onError={() => setFailed(true)}
      className="object-contain"
      style={{ width: size, height: size, filter: 'drop-shadow(0 2px 5px rgba(0,0,0,0.34))' }}
    />
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
  pinging?: boolean;
  onSelect: (server: ServerConfig) => void;
}

/**
 * Clean a server name for display next to the flag image: subscription names
 * often embed an emoji flag (🇳🇱) — Windows renders regional indicators as
 * bare letters ("NL") — plus sometimes an ASCII ISO prefix. Strip both.
 */
export function displayServerName(server: Pick<ServerConfig, 'name' | 'countryCode'>): string {
  const name = isValidCountryCode(server.countryCode) ? server.name : splitLeadingEmoji(server.name).name;
  return displayLocationTitle(name, server.countryCode);
}

/** Design location row: flag, user-facing location name, ping dot + ms. */
export default function ServerRow({ server, active, pinging, onSelect }: Props) {
  const { t } = useTranslation();
  const pc = pingColor(server.ping);
  const hasPing = server.ping !== undefined && server.ping > 0;
  const auto = isClosedAutoLocationServer(server);
  const name = displayServerName(server);

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
      <span className="flex shrink-0 items-center justify-end gap-[7px]">
        {auto ? (
          <span className="whitespace-nowrap text-[12px] font-medium text-[#ffb02e]">{t('v6AutoBest' as never)}</span>
        ) : pinging ? (
          <Loader2 className="h-3.5 w-3.5 v6-orb-spin text-white/50" />
        ) : (
          <>
            <span className="h-[7px] w-[7px] rounded-full" style={{ background: pc, boxShadow: hasPing ? `0 0 8px ${pc}` : 'none' }} />
            <span className="w-[86px] whitespace-nowrap text-right text-[12px] tabular-nums text-white/60">
              {hasPing ? `${server.ping}\u00a0ms` : server.ping === -1 ? t('v6NoResponse' as never) : t('v6NotChecked' as never)}
            </span>
          </>
        )}
      </span>
    </button>
  );
}
