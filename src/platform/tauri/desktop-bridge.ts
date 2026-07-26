import { invoke } from '@tauri-apps/api/core';
import type {
  AppApiLocationsResponse,
  AppApiSessionStatus,
  AppApiSubscriptionSummary,
} from '../../lib/app-control-plane';

type InvokeCommand = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export class DesktopBridge {
  private readonly invokeCommand: InvokeCommand;

  constructor(invokeCommand: InvokeCommand = invoke) {
    this.invokeCommand = invokeCommand;
  }

  appApiSessionStatus(): Promise<AppApiSessionStatus> {
    return this.invokeCommand('app_api_session_status');
  }

  appApiExchangeCode(code: string): Promise<AppApiSessionStatus> {
    return this.invokeCommand('app_api_exchange_code', { request: { code } });
  }

  appApiExchangeLegacySubscription(subscriptionUrl: string): Promise<AppApiSessionStatus> {
    return this.invokeCommand('app_api_exchange_legacy_subscription', {
      request: { subscription_url: subscriptionUrl },
    });
  }

  appApiRefresh(): Promise<AppApiSessionStatus> {
    return this.invokeCommand('app_api_refresh');
  }

  appApiLogout(): Promise<void> {
    return this.invokeCommand('app_api_logout');
  }

  appApiLocations(): Promise<AppApiLocationsResponse> {
    return this.invokeCommand('app_api_locations');
  }

  appApiSubscriptionStatus(): Promise<AppApiSubscriptionSummary> {
    return this.invokeCommand('app_api_subscription_status');
  }
}

export const desktopBridge = new DesktopBridge();
