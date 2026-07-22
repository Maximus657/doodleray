import { sanitizeDiagnosticText } from './redaction.ts';

const sanitized = sanitizeDiagnosticText('Failed https://secret.example/path at 192.0.2.1');
if (sanitized !== 'Failed https://[domain]/... at [ip]') {
  throw new Error(`Unexpected diagnostic redaction: ${sanitized}`);
}

const ipv6 = sanitizeDiagnosticText('resolved 2001:db8::10 via hysteria2://secret@example.test:443');
if (ipv6 !== 'resolved [ip] via hysteria2://[redacted]') {
  throw new Error(`Unexpected IPv6/profile redaction: ${ipv6}`);
}

const path = sanitizeDiagnosticText('saved C:\\Users\\Alice\\AppData\\Local\\Temp\\bundle.zip');
if (path !== 'saved [path]') throw new Error(`Unexpected path redaction: ${path}`);
