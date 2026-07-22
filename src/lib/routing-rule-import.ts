export type ImportedRoutingRule = {
  type: 'domain' | 'exe';
  value: string;
  action: 'proxy' | 'direct' | 'block';
  enabled: boolean;
  comment?: string;
};

export function parseRoutingRuleImport(input: unknown): ImportedRoutingRule[] {
  if (!Array.isArray(input) || input.length > 256) throw new Error('Invalid format');

  return input.map((value) => {
    if (!value || typeof value !== 'object') throw new Error('Invalid rule');
    const rule = value as Record<string, unknown>;
    if (rule.type !== 'domain' && rule.type !== 'exe') throw new Error('Invalid rule type');
    if (rule.action !== 'proxy' && rule.action !== 'direct' && rule.action !== 'block') {
      throw new Error('Invalid rule action');
    }
    if (typeof rule.value !== 'string' || !rule.value.trim() || rule.value.length > 2048) {
      throw new Error('Invalid rule value');
    }
    if (rule.comment !== undefined && (typeof rule.comment !== 'string' || rule.comment.length > 500)) {
      throw new Error('Invalid rule comment');
    }

    return {
      type: rule.type,
      value: rule.value.trim(),
      action: rule.action,
      enabled: rule.enabled !== false,
      ...(typeof rule.comment === 'string' && rule.comment.trim()
        ? { comment: rule.comment.trim() }
        : {}),
    };
  });
}
