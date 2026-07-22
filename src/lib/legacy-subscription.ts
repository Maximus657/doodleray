const LEGACY_DOODLE_SUBSCRIPTION_HOSTS = new Set([
  'ddlvpn.lol',
  'www.ddlvpn.lol',
  'doodlevpn.online',
  'www.doodlevpn.online',
  'sub.brewsandrologistics.fun',
]);

export function isLegacyDoodleSubscriptionUrl(value: string): boolean {
  try {
    const url = new URL(value);
    const parts = url.pathname.split('/').filter(Boolean);
    return (
      url.protocol === 'https:'
      && LEGACY_DOODLE_SUBSCRIPTION_HOSTS.has(url.hostname.toLowerCase())
      && parts.length === 2
      && (parts[0] === 's' || parts[0] === 'sub')
      && parts[1].length >= 8
    );
  } catch {
    return false;
  }
}

export function findLegacyDoodleSubscriptionUrls(
  subscriptions: { id?: string; url: string }[],
  preferredSubscriptionId?: string,
): string[] {
  const found: string[] = [];
  const ordered = preferredSubscriptionId
    ? [
        ...subscriptions.filter((subscription) => subscription.id === preferredSubscriptionId),
        ...subscriptions.filter((subscription) => subscription.id !== preferredSubscriptionId),
      ]
    : subscriptions;
  for (const subscription of ordered) {
    try {
      const url = new URL(subscription.url);
      const normalized = url.toString();
      if (isLegacyDoodleSubscriptionUrl(normalized) && !found.includes(normalized)) found.push(normalized);
    } catch {
      // Old manual entries can contain proxy links or malformed text.
    }
  }
  return found;
}

export function findLegacyDoodleSubscriptionUrl(subscriptions: { url: string }[]): string | null {
  return findLegacyDoodleSubscriptionUrls(subscriptions)[0] ?? null;
}
