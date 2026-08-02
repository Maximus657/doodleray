import { appRouteFromPathname } from './app-route.ts';

for (const route of ['/', '/servers', '/workshop', '/settings']) {
  if (appRouteFromPathname(route) !== route) throw new Error(`route ${route} was not preserved`);
}
if (appRouteFromPathname('/untrusted-redirect') !== '/') throw new Error('unknown route did not fail closed');

console.log('app route checks passed');
