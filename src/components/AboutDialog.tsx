import { Tooltip } from '@radix-ui/themes';
import { useEffect, useState } from 'react';
import { Badge, Button, Dialog, Flex, IconButton, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, type UpdateStatus } from '../api.ts';
import logo from '../../src-tauri/icons/128x128.png';

const THIRD_PARTY: Array<{ name: string; repo: string; license: string; url: string }> = [
  { name: 'HLAE', repo: 'advancedfx/advancedfx', license: 'custom', url: 'https://github.com/advancedfx/advancedfx' },
  { name: 'FFmpeg', repo: 'BtbN/FFmpeg-Builds', license: 'GPL', url: 'https://github.com/BtbN/FFmpeg-Builds' },
  { name: 'Source 2 Viewer', repo: 'ValveResourceFormat/ValveResourceFormat', license: 'MIT', url: 'https://github.com/ValveResourceFormat/ValveResourceFormat' },
  { name: 'demoparser', repo: 'LaihoE/demoparser', license: 'MIT', url: 'https://github.com/LaihoE/demoparser' },
];

function LinkIcon({ url, label }: { url: string; label: string }) {
  return (
    <IconButton size="1" variant="ghost" color="gray" aria-label={label} onClick={() => void api.openUrl(url)}>
      <i aria-hidden="true" className="bi bi-box-arrow-up-right app-icon"  />
    </IconButton>
  );
}

/** Small "about" dialog: name, version, author, third-party components. */
export function AboutDialog() {
  const { t } = useTranslation();
  const [update, setUpdate] = useState<UpdateStatus | 'failed'>();
  useEffect(() => {
    let active = true;
    void api.checkForUpdates().then(value => { if (active) setUpdate(value); }).catch(() => { if (active) setUpdate('failed'); });
    return () => { active = false; };
  }, []);
  const available = update !== 'failed' && update?.status === 'available' ? update.version : undefined;
  const [version, setVersion] = useState<string>();
  useEffect(() => {
    void api.status().then((s) => setVersion(s.version)).catch(() => undefined);
  }, []);
  return (
    <Dialog.Root>
      <Tooltip delayDuration={150} content={t('about.button')}><Dialog.Trigger>
        <IconButton variant="ghost" color="gray" aria-label={available ? t('about.button') + ': ' + t('about.updateAvailable', { version: available }) : t('about.button')} >
          <i aria-hidden="true" className={available ? "bi bi-arrow-up-circle-fill app-icon" : "bi bi-info-circle app-icon"} style={available ? { color: "var(--green-11)" } : undefined} />

        </IconButton>
      </Dialog.Trigger></Tooltip>
      <Dialog.Content maxWidth="800px" style={{ padding: 32 }}>
        <Flex justify="between" align="center" gap="3">
          <Flex align="center" gap="3">
            <img src={logo} alt="" width="48" height="48" style={{ background: 'var(--app-logo-background, #121518)', borderRadius: 10, padding: 4 }} />
            <Dialog.Title mb="0">CS DemoDesk</Dialog.Title>
          </Flex>
          <Text size="2" color="gray" className="mono">
            v{version ?? '…'}
          </Text>
        </Flex>
        <Dialog.Description size="2" color="gray" mt="4" mb="0" style={{ lineHeight: 1.7 }}>
          {t('about.tagline')}
        </Dialog.Description>

        {update === 'failed' && <Text as="p" size="2" color="gray">{t('about.updateFailed')}</Text>}
        {update !== 'failed' && update?.status === 'available' && (
          <Flex align="center" gap="2" mt="3">
            <Text size="2">{t('about.updateAvailable', { version: update.version })}</Text>
            <LinkIcon url="https://github.com/noih/cs-demodesk/releases/latest" label={t('about.openGithub')} />
          </Flex>
        )}
        <Flex align="center" gap="2" mt="5" wrap="wrap">
          <Text size="2">{t('about.author')}</Text>
          <Text size="2" color="gray">· AGPL-3.0</Text>
        </Flex>
        <Flex align="center" gap="4" mt="3" wrap="wrap">
          <Button size="2" variant="soft" color="gray" onClick={() => void api.openUrl('https://github.com/noih/cs-demodesk')}>
            GitHub <i aria-hidden="true" className="bi bi-box-arrow-up-right app-icon" />
          </Button>
          <Button size="2" variant="soft" color="gray" onClick={() => void api.openUrl('https://apps.microsoft.com/detail/9N5G4VXSDGS5')}>
            Microsoft Store <i aria-hidden="true" className="bi bi-box-arrow-up-right app-icon" />
          </Button>
        </Flex>
        <Text as="div" size="2" weight="medium" mt="5" mb="2">
          {t('about.thirdParty')}
        </Text>
        <div className="about-list">
          {THIRD_PARTY.map((c) => (
            <div key={c.name} className="about-row">
              <Text size="2">{c.name}</Text>
              <Text size="1" color="gray" className="mono" truncate title={c.repo}>
                {c.repo}
              </Text>
              <Badge size="1" variant="soft" color="gray">
                {c.license}
              </Badge>
              <LinkIcon url={c.url} label={t('about.openSite', { name: c.name })} />
            </div>
          ))}
        </div>

        <Flex justify="end" mt="4">
          <Dialog.Close>
            <Button variant="soft">{t('common.close')}</Button>
          </Dialog.Close>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
