import { invoke } from '@tauri-apps/api/core';
import type {
  AppApiLocationsResponse,
  AppApiSessionStatus,
  AppApiSubscriptionSummary,
} from '../../lib/app-control-plane';
import type { ConnectionHealthReport } from '../../lib/connection-health';
import type { ProxyMode, SystemProxyMode } from '../../stores/app-store';

type InvokeCommand = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

export class DesktopBridge {
  private readonly invokeCommand: InvokeCommand;

  constructor(invokeCommand: InvokeCommand = invoke) {
    this.invokeCommand = invokeCommand;
  }

  private invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    return this.invokeCommand(command, args) as Promise<T>;
  }

  appApiSessionStatus(): Promise<AppApiSessionStatus> {
    return this.invoke('app_api_session_status');
  }

  appApiExchangeCode(code: string): Promise<AppApiSessionStatus> {
    return this.invoke('app_api_exchange_code', { request: { code } });
  }

  appApiExchangeLegacySubscription(subscriptionUrl: string): Promise<AppApiSessionStatus> {
    return this.invoke('app_api_exchange_legacy_subscription', {
      request: { subscription_url: subscriptionUrl },
    });
  }

  appApiRefresh(): Promise<AppApiSessionStatus> {
    return this.invoke('app_api_refresh');
  }

  appApiLogout(): Promise<void> {
    return this.invoke('app_api_logout');
  }

  appApiLocations(): Promise<AppApiLocationsResponse> {
    return this.invoke('app_api_locations');
  }

  appApiSubscriptionStatus(): Promise<AppApiSubscriptionSummary> {
    return this.invoke('app_api_subscription_status');
  }

  vpnDisconnect(): Promise<unknown> {
    return this.invoke('vpn_disconnect');
  }

  prepareForAppUpdate(): Promise<unknown> {
    return this.invoke('prepare_for_app_update');
  }

  secureStoreSet(key: string, value: string): Promise<void> {
    return this.invoke('secure_store_set', { key, value });
  }

  secureStoreDelete(key: string): Promise<void> {
    return this.invoke('secure_store_delete', { key });
  }

  getConnectionHealth(
    proxyMode: ProxyMode,
    systemProxyMode: SystemProxyMode,
    socksPort: number,
    httpPort: number,
  ): Promise<ConnectionHealthReport> {
    return this.invoke('get_connection_health', {
      proxyMode,
      systemProxyMode,
      socksPort,
      httpPort,
    });
  }

  checkDefenderExclusion(): Promise<boolean> {
    return this.invoke('check_defender_exclusion');
  }

  addDefenderExclusion(): Promise<string> {
    return this.invoke('add_defender_exclusion');
  }

  toggleSilentAutostart(enable: boolean): Promise<string> {
    return this.invoke('toggle_silent_autostart', { enable });
  }

  isAdmin(): Promise<boolean> {
    return this.invoke('is_admin');
  }

  restartAsAdmin(): Promise<void> {
    return this.invoke('restart_as_admin');
  }
}

export const desktopBridge = new DesktopBridge();
