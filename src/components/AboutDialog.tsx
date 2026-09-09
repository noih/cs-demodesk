import { Tooltip } from '@radix-ui/themes';
import { useEffect, useState } from 'react';
import { Badge, Button, Dialog, Flex, IconButton, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api } from '../api.ts';
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
  const [version, setVersion] = useState<string>();
  useEffect(() => {
    void api.status().then((s) => setVersion(s.version)).catch(() => undefined);
  }, []);
  return (
    <Dialog.Root>
      <Tooltip delayDuration={150} content={t('about.button')}><Dialog.Trigger>
        <IconButton variant="ghost" color="gray" aria-label={t('about.button')} >
          <i aria-hidden="true" className="bi bi-info-circle app-icon" />
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

        <Flex align="center" gap="2" mt="5" wrap="wrap">
          <Text size="2">{t('about.author')}</Text>
          <Text size="2" color="gray" className="mono">
            github.com/noih/cs-demodesk
          </Text>
          <LinkIcon url="https://github.com/noih/cs-demodesk" label={t('about.openGithub')} />
          <Text size="2" color="gray">
            · AGPL-3.0
          </Text>
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
