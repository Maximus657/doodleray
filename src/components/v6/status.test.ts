import { deriveOrbState } from './status.ts';

if (deriveOrbState('connected', 'protected', 'protected') !== 'protected') throw new Error('Protected health was not shown as protected');
if (deriveOrbState('connected', 'protected', 'repairing') !== 'degraded') throw new Error('Repairing health was shown as protected');
if (deriveOrbState('connected', 'protected', 'unexpected') !== 'degraded') throw new Error('Unknown health failed open');
