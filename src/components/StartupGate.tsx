import { useEffect, useState, type ReactNode } from 'react';
import { Button, Callout, Flex, Heading, Spinner, Text } from '@radix-ui/themes';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { api, errorText } from '../api.ts';

export function StartupGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>();
  const [busy, setBusy] = useState(false);
  const check = () => api.startupError().then(setError).catch((e: unknown) => setError(errorText(e)));
  useEffect(() => { void check(); }, []);

  const recover = async (choose: boolean) => {
    setBusy(true);
    try {
      const path = choose ? await open({ directory: true, multiple: false }) : null;
      if (choose && typeof path !== 'string') return;
      await api.recoverDataDirectory(typeof path === 'string' ? path : null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (error === null) return children;
  return (
    <Flex direction="column" align="center" justify="center" gap="4" p="6" style={{ minHeight: '100vh' }}>
      {error === undefined ? <Spinner /> : <>
        <Heading size="5">{t('startup.title')}</Heading>
        <Text align="center">{t('startup.hint')}</Text>
        <Callout.Root color="red" style={{ maxWidth: 720, overflowWrap: 'anywhere' }}><Callout.Text>{error}</Callout.Text></Callout.Root>
        <Flex gap="3" wrap="wrap" justify="center">
          <Button disabled={busy} onClick={() => void recover(true)}>{t('startup.choose')}</Button>
          <Button variant="soft" disabled={busy} onClick={() => void recover(false)}>{t('startup.default')}</Button>
        </Flex>
      </>}
    </Flex>
  );
}
