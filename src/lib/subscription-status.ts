import type { Subscription } from '../stores/app-store';

export type SubscriptionLimitReason = 'traffic' | 'expired';

export interface SubscriptionTrafficStatus {
  hasTrafficInfo: boolean;
  hasQuota: boolean;
  used: number;
  total: number;
  remaining: number;
  usedPercent: number;
  remainingPercent: number;
  isLimited: boolean;
  reason: SubscriptionLimitReason | null;
}

const TRAFFIC_LIMIT_EPSILON_BYTES = 1024;

function cleanBytes(value: number | undefined): number {
  return typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : 0;
}

export function getSubscriptionTrafficStatus(sub: Subscription): SubscriptionTrafficStatus {
  const traffic = sub.traffic;
  const upload = cleanBytes(traffic?.upload);
  const download = cleanBytes(traffic?.download);
  const total = cleanBytes(traffic?.total);
  const used = upload + download;
  const hasTrafficInfo = !!traffic;
  const hasQuota = total > 0;
  const remaining = hasQuota ? Math.max(0, total - used) : 0;
  const usedPercent = hasQuota ? Math.min(100, Math.max(0, (used / total) * 100)) : 0;
  const remainingPercent = hasQuota ? Math.min(100, Math.max(0, (remaining / total) * 100)) : 0;
  const isTrafficLimited = hasQuota && remaining <= TRAFFIC_LIMIT_EPSILON_BYTES;
  const isExpired = !!traffic?.expire && traffic.expire > 0 && traffic.expire * 1000 <= Date.now();
  const reason = isTrafficLimited ? 'traffic' : isExpired ? 'expired' : null;

  return {
    hasTrafficInfo,
    hasQuota,
    used,
    total,
    remaining,
    usedPercent,
    remainingPercent,
    isLimited: isTrafficLimited || isExpired,
    reason,
  };
}

export function getSubscriptionById(subscriptions: Subscription[], id?: string) {
  if (!id) return null;
  return subscriptions.find((sub) => sub.id === id) || null;
}
