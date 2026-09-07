import { useEffect, useState } from 'react';
import { Badge, Button, Dialog, Flex, IconButton, Text } from '@radix-ui/themes';
import { ExternalLinkIcon, InfoCircledIcon } from '@radix-ui/react-icons';
import { api } from '../api.ts';

const THIRD_PARTY: Array<{ name: string; repo: string; license: string; url: string }> = [
  { name: 'HLAE', repo: 'advancedfx/advancedfx', license: 'custom', url: 'https://github.com/advancedfx/advancedfx' },
  { name: 'FFmpeg', repo: 'BtbN/FFmpeg-Builds', license: 'GPL', url: 'https://github.com/BtbN/FFmpeg-Builds' },
  { name: 'Source 2 Viewer', repo: 'ValveResourceFormat/ValveResourceFormat', license: 'MIT', url: 'https://github.com/ValveResourceFormat/ValveResourceFormat' },
  { name: 'demoparser', repo: 'LaihoE/demoparser', license: 'MIT', url: 'https://github.com/LaihoE/demoparser' },
];

function LinkIcon({ url, label }: { url: string; label: string }) {
  return (
    <IconButton size="1" variant="ghost" color="gray" aria-label={label} onClick={() => void api.openUrl(url)}>
      <ExternalLinkIcon />
    </IconButton>
  );
}

/** Small "about" dialog: name, version, author, third-party components. */
export function AboutDialog() {
  const [version, setVersion] = useState<string>();
  useEffect(() => {
    void api.status().then((s) => setVersion(s.version)).catch(() => undefined);
  }, []);
  return (
    <Dialog.Root>
      {/* a plain button: wrapping the trigger in a Tooltip swallowed the click */}
      <Dialog.Trigger>
        <Button variant="soft" color="gray">
          <InfoCircledIcon /> 關於
        </Button>
      </Dialog.Trigger>
      <Dialog.Content maxWidth="560px">
        <Flex justify="between" align="baseline" gap="3">
          <Dialog.Title mb="0">CS DemoDesk</Dialog.Title>
          <Text size="2" color="gray" className="mono">
            v{version ?? '…'}
          </Text>
        </Flex>
        <Dialog.Description size="2" color="gray" mt="1">
          Counter-Strike demo analysis desktop app
        </Dialog.Description>

        <Flex align="center" gap="2" mt="4">
          <Text size="2">作者 NOIH</Text>
          <Text size="2" color="gray" className="mono">
            github.com/noih
          </Text>
          <LinkIcon url="https://github.com/noih" label="開啟 GitHub" />
          <Text size="2" color="gray">
            · AGPL-3.0
          </Text>
        </Flex>

        <Text as="div" size="2" weight="medium" mt="4" mb="2">
          第三方元件
        </Text>
        <div className="about-list">
          {THIRD_PARTY.map((t) => (
            <div key={t.name} className="about-row">
              <Text size="2">{t.name}</Text>
              <Text size="1" color="gray" className="mono" truncate title={t.repo}>
                {t.repo}
              </Text>
              <Badge size="1" variant="soft" color="gray">
                {t.license}
              </Badge>
              <LinkIcon url={t.url} label={`開啟 ${t.name} 網頁`} />
            </div>
          ))}
        </div>

        <Flex justify="end" mt="4">
          <Dialog.Close>
            <Button variant="soft">關閉</Button>
          </Dialog.Close>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
