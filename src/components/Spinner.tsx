import { useTranslation } from 'react-i18next';

/** One stable rotating indicator; reduced-motion users get a static status icon. */
export function Spinner({ size = '2' }: { size?: '1' | '2' | '3' }) {
  const { t } = useTranslation();
  return <span role="status" aria-label={t('replay.preparing')} className={`app-spinner app-spinner-${size}`}></span>;
}
