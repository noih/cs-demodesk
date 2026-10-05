import { useEffect, useState } from 'react';
import { Button, Dialog, Flex, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, errorText, type SettingsResponse } from '../api.ts';
import { Spinner } from './Spinner.tsx';
import { useNotify } from './Notifications.tsx';

const labels = { hlae: 'HLAE', ffmpeg: 'FFmpeg', vrf: 'Source 2 Viewer CLI' };
const phases: Record<string, 'verify' | 'extract' | 'wait' | 'replace'> = {
  'Verifying download': 'verify',
  'Extracting verified download': 'extract',
  'Waiting for tools to finish': 'wait',
  'Replacing tool': 'replace',
};

export function ToolInstallDialog({ setup }: { setup: SettingsResponse['setup'] }) {
  const { t } = useTranslation();
  const notify = useNotify();
  const [dismissed, setDismissed] = useState(false);
  const active = (Object.keys(labels) as (keyof typeof labels)[]).filter(tool => setup[tool]?.running && phases[setup[tool]?.progress ?? '']);
  useEffect(() => { if (active.length === 0) setDismissed(false); }, [active.length]);
  const progressText = (progress?: string | null) => {
    const phase = phases[progress ?? ''];
    return phase ? t(`toolInstall.${phase}`) : progress || t('toolInstall.prepare');
  };
  return <Dialog.Root open={active.length > 0 && !dismissed} onOpenChange={open => { if (!open) setDismissed(true); }}>
    <Dialog.Content maxWidth="480px">
      <Dialog.Title>{t('toolInstall.title')}</Dialog.Title>
      <Dialog.Description>{t('toolInstall.hint')}</Dialog.Description>
      <Flex direction="column" gap="3" my="4">
        {active.map(tool => <Flex key={tool} align="center" gap="3">
          <Spinner />
          <Flex direction="column" flexGrow="1">
            <Text weight="bold">{labels[tool]}</Text>
            <Text size="2">{progressText(setup[tool]?.progress)}</Text>
          </Flex>
          <Button variant="soft" onClick={() => void api.cancelSetup(tool).catch(error => notify(errorText(error)))}>{t('common.cancel')}</Button>
        </Flex>)}
      </Flex>
      <Flex justify="end"><Dialog.Close><Button variant="outline">{t('common.close')}</Button></Dialog.Close></Flex>
    </Dialog.Content>
  </Dialog.Root>;
}
