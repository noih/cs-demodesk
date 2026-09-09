import i18n from 'i18next';

/** Keep real names intact; only replace names with no visible characters. */
export function displayPlayerName(name: string | undefined, fallback = i18n.t('common.unnamed')): string {
  return name && name.replace(/[\s\p{Cc}\p{Default_Ignorable_Code_Point}\u2800\u3164\uffa0]/gu, '') ? name : fallback;
}
