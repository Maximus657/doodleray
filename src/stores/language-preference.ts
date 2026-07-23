export type SupportedLanguage = 'ru' | 'en' | 'zh';

export function supportedLanguage(value: unknown): SupportedLanguage | null {
  return value === 'ru' || value === 'en' || value === 'zh' ? value : null;
}

export function resolveLanguagePreference(
  persistedLanguage: unknown,
  fallbackLanguage: SupportedLanguage,
  immediateLanguage: unknown,
): SupportedLanguage {
  return supportedLanguage(immediateLanguage)
    ?? supportedLanguage(persistedLanguage)
    ?? fallbackLanguage;
}
