import { invoke } from '@tauri-apps/api/core';
import type {
  AppApiLocationsResponse,
  AppApiSessionStatus,
  AppApiSubscriptionSummary,
} from '../../lib/app-control-plane';
import type { ConnectionHealthReport } from '../../lib/connection-health';
import type { ProxyMode, SystemProxyMode } from '../../stores/app-store';

type InvokeCommand = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

export interface ConnectionResult {
  success: boolean;
  message: string;
  health?: ConnectionHealthReport | null;
}

export class DesktopBridge {
  private readonly invokeCommand: InvokeCommand;

  constructor(invokeCommand: InvokeCommand = invoke) {
    this.invokeCommand = invokeCommand;
  }

  command<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    return this.invokeCommand(command, args) as Promise<T>;
  }

  appApiSessionStatus(): Promise<AppApiSessionStatus> {
    return this.command('app_api_session_status');
  }

  appApiExchangeCode(code: string): Promise<AppApiSessionStatus> {
    return this.command('app_api_exchange_code', { request: { code } });
  }

  appApiExchangeLegacySubscription(subscriptionUrl: string): Promise<AppApiSessionStatus> {
    return this.command('app_api_exchange_legacy_subscription', {
      request: { subscription_url: subscriptionUrl },
    });
  }

  appApiRefresh(): Promise<AppApiSessionStatus> {
    return this.command('app_api_refresh');
  }

  appApiRefreshCachedProfiles(): Promise<void> {
    return this.command('app_api_refresh_cached_profiles');
  }

  appApiLogout(): Promise<void> {
    return this.command('app_api_logout');
  }

  appApiLocations(): Promise<AppApiLocationsResponse> {
    return this.command('app_api_locations');
  }

  appApiSubscriptionStatus(): Promise<AppApiSubscriptionSummary> {
    return this.command('app_api_subscription_status');
  }

  appConnectLocation(request: object): Promise<ConnectionResult> {
    return this.command('app_connect_location', { request });
  }

  vpnConnect(request: object): Promise<ConnectionResult> {
    return this.command('vpn_connect', { request });
  }

  vpnDisconnect(): Promise<ConnectionResult> {
    return this.command('vpn_disconnect');
  }

  prepareForAppUpdate(): Promise<unknown> {
    return this.command('prepare_for_app_update');
  }

  secureStoreSet(key: string, value: string): Promise<void> {
    return this.command('secure_store_set', { key, value });
  }

  secureStoreDelete(key: string): Promise<void> {
    return this.command('secure_store_delete', { key });
  }

  getConnectionHealth(
    proxyMode: ProxyMode,
    systemProxyMode: SystemProxyMode,
    socksPort: number,
    httpPort: number,
  ): Promise<ConnectionHealthReport> {
    return this.command('get_connection_health', {
      proxyMode,
      systemProxyMode,
      socksPort,
      httpPort,
    });
  }

  checkDefenderExclusion(): Promise<boolean> {
    return this.command('check_defender_exclusion');
  }

  addDefenderExclusion(): Promise<string> {
    return this.command('add_defender_exclusion');
  }

  toggleSilentAutostart(enable: boolean): Promise<string> {
    return this.command('toggle_silent_autostart', { enable });
  }

  isAdmin(): Promise<boolean> {
    return this.command('is_admin');
  }

  restartAsAdmin(): Promise<void> {
    return this.command('restart_as_admin');
  }
}

export const desktopBridge = new DesktopBridge();
