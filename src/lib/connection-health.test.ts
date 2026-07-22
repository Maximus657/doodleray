import { getUserVisibleHealthVerdict, isNonActionableProtectedDegraded, type ConnectionHealthReport } from './connection-health.ts';

const mixedRoutes: ConnectionHealthReport = {
  verdict: 'protected_degraded',
  mode: 'tun',
  generated_at_ms: 1,
  service_degraded_checks: ['ipv6_default_route=Ethernet metric=1\nipv6_default_route=DoodleRay Tunnel metric=50'],
  checks: [],
};

if (isNonActionableProtectedDegraded(mixedRoutes)) throw new Error('Competing IPv6 route was hidden');
if (getUserVisibleHealthVerdict(mixedRoutes) !== 'protected_degraded') throw new Error('Degraded verdict was upgraded to protected');
