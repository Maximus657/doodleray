import { resolveLanguagePreference } from './language-preference.ts';

function assertEqual(actual: unknown, expected: unknown) {
  if (actual !== expected) throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
}

// A just-selected language must win over an older asynchronous secure-store
// snapshot after an immediate application close.
assertEqual(resolveLanguagePreference('en', 'en', 'ru'), 'ru');
assertEqual(resolveLanguagePreference('ru', 'en', null), 'ru');
assertEqual(resolveLanguagePreference('not-a-language', 'zh', null), 'zh');
assertEqual(resolveLanguagePreference(null, 'en', 'not-a-language'), 'en');

console.log('app store preference tests passed');
