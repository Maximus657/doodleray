export const appRoutes = ['/', '/servers', '/workshop', '/settings'] as const;

export type AppRoute = typeof appRoutes[number];

export function appRouteFromPathname(pathname: string): AppRoute {
  return appRoutes.includes(pathname as AppRoute) ? pathname as AppRoute : '/';
}
