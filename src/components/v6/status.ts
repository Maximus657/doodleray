import type { ConnectionStatus, ProductMode } from '../../stores/app-store';

/** Visual state of the connect orb / status dot. Honest about limited protection. */
export type OrbState =
  | 'idle'
  | 'connecting'
  | 'disconnecting'
  | 'protected'
  | 'degraded'
  | 'limited'
  | 'failed';

/** i18n key for the short status label shown in the orb status pill. */
export const ORB_LABEL_KEY: Record<OrbState, string> = {
  idle: 'notConnected',
  connecting: 'connecting',
  disconnecting: 'disconnecting',
  protected: 'v6StatusProtected',
  degraded: 'v6StatusDegraded',
  limited: 'v6StatusLimited',
  failed: 'v6StatusFailed',
};

/**
 * Derive the orb state from the connection status, the active product mode and
 * (when connected) the latest structured health verdict. Falls back to a coarse
 * mode-based reading when no verdict is available yet.
 */
export function deriveOrbState(
  status: ConnectionStatus,
  productMode: ProductMode,
  healthVerdict?: string | null,
): OrbState {
  if (status === 'connecting') return 'connecting';
  if (status === 'disconnecting') return 'disconnecting';
  if (status !== 'connected') return 'idle';

  if (healthVerdict === 'failed' || healthVerdict === 'cleanup_pending') return 'failed';

  if (productMode === 'protected') {
    if (healthVerdict === 'protected') return 'protected';
    if (healthVerdict === 'protected_degraded') return 'degraded';
    if (healthVerdict === 'limited') return 'limited';
    return 'degraded';
  }
  // Browsers / manual are honestly "limited" — never fake full protection.
  return 'limited';
}
