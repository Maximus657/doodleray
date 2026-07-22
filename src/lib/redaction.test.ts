import { sanitizeDiagnosticText } from './redaction.ts';

const sanitized = sanitizeDiagnosticText('Failed https://secret.example/path at 192.0.2.1');
if (sanitized !== 'Failed https://[domain]/... at [ip]') {
  throw new Error(`Unexpected diagnostic redaction: ${sanitized}`);
}
