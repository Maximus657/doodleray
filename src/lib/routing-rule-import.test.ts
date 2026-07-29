import { parseRoutingRuleImport } from './routing-rule-import.ts';

const parsed = parseRoutingRuleImport([
  { type: 'domain', value: '  steampowered.com  ', action: 'direct', enabled: true, comment: ' Steam ' },
]);

if (parsed.length !== 1 || parsed[0]?.value !== 'steampowered.com' || parsed[0]?.comment !== 'Steam') {
  throw new Error('Valid imported routing rule was not normalized');
}

let rejected = false;
try {
  parseRoutingRuleImport([
    { type: 'domain', value: 'example.com', action: 'direct' },
    { type: 'domain', value: 'invalid.example', action: 'execute' },
  ]);
} catch {
  rejected = true;
}

if (!rejected) throw new Error('Partially invalid routing import was accepted');
