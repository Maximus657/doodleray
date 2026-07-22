const LEGACY_DOODLE_SUBSCRIPTION_HOSTS = new Set([
  'ddlvpn.lol',
  'www.ddlvpn.lol',
  'doodlevpn.online',
  'www.doodlevpn.online',
  'sub.brewsandrologistics.fun',
]);

export function findLegacyDoodleSubscriptionUrl(subscriptions: { url: string }[]): string | null {
  for (const subscription of subscriptions) {
    try {
      const url = new URL(subscription.url);
      const parts = url.pathname.split('/').filter(Boolean);
      if (
        url.protocol === 'https:'
        && LEGACY_DOODLE_SUBSCRIPTION_HOSTS.has(url.hostname.toLowerCase())
        && parts.length === 2
        && (parts[0] === 's' || parts[0] === 'sub')
        && parts[1].length >= 8
      ) {
        return url.toString();
      }
    } catch {
      // Old manual entries can contain proxy links or malformed text.
    }
  }
  return null;
}
