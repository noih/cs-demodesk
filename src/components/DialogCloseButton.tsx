import { AlertDialog, Dialog, IconButton } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';

export function DialogCloseButton({ alert = false }: { alert?: boolean }) {
  const { t } = useTranslation();
  const Close = alert ? AlertDialog.Cancel : Dialog.Close;
  return <Close>
    <IconButton size="2" variant="ghost" color="gray" aria-label={t('common.close')} title={t('common.close')} style={{flexShrink: 0}}>
      <i aria-hidden="true" className="bi bi-x-lg app-icon" />
    </IconButton>
  </Close>;
}
