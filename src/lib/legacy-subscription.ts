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

type PersistedSubscription = { id?: string; url?: string; [key: string]: unknown };
type PersistedState = {
  subscriptions?: PersistedSubscription[];
  activeServer?: { subscriptionId?: string; [key: string]: unknown } | null;
  [key: string]: unknown;
};
type PersistedEnvelope = { state?: PersistedState; [key: string]: unknown };

/**
 * Preserve DoodleVPN links from 5.x when an earlier v6 attempt has already
 * created an empty secure-store snapshot. The secure snapshot stays
 * canonical; only missing DoodleVPN subscriptions and their active hint are
 * carried over so the account exchange can run without asking for a code.
 */
export function mergeLegacyDoodleSubscriptionState(
  secureValue: string,
  legacyValue: string,
): string {
  let secureEnvelope: PersistedEnvelope;
  let legacyEnvelope: PersistedEnvelope;
  try {
    secureEnvelope = JSON.parse(secureValue) as PersistedEnvelope;
    legacyEnvelope = JSON.parse(legacyValue) as PersistedEnvelope;
  } catch {
    return secureValue;
  }

  const secureState = secureEnvelope?.state;
  const legacyState = legacyEnvelope?.state;
  if (!secureState || !legacyState) return secureValue;

  const secureSubscriptions = Array.isArray(secureState.subscriptions)
    ? secureState.subscriptions
    : [];
  const legacySubscriptions = Array.isArray(legacyState.subscriptions)
    ? legacyState.subscriptions.filter(
        (subscription) => typeof subscription?.url === 'string'
          && isLegacyDoodleSubscriptionUrl(subscription.url),
      )
    : [];
  if (legacySubscriptions.length === 0) return secureValue;

  const knownUrls = new Set(
    secureSubscriptions
      .map((subscription) => {
        if (typeof subscription?.url !== 'string') return null;
        try { return new URL(subscription.url).toString(); }
        catch { return subscription.url; }
      })
      .filter((url): url is string => url !== null),
  );
  const missing = legacySubscriptions.filter((subscription) => {
    const normalized = new URL(subscription.url as string).toString();
    if (knownUrls.has(normalized)) return false;
    knownUrls.add(normalized);
    return true;
  });
  if (missing.length === 0) return secureValue;

  const migratedIds = new Set(missing.map((subscription) => subscription.id).filter(Boolean));
  const legacyActive = legacyState.activeServer;
  const shouldCarryActiveHint = !secureState.activeServer
    && legacyActive?.subscriptionId
    && migratedIds.has(legacyActive.subscriptionId);

  return JSON.stringify({
    ...secureEnvelope,
    state: {
      ...secureState,
      subscriptions: [...secureSubscriptions, ...missing],
      ...(shouldCarryActiveHint ? { activeServer: legacyActive } : {}),
    },
  });
}
