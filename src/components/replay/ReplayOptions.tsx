import { Box, Button, Flex, Popover, SegmentedControl, Switch, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import type { DrawToggles } from '../../replay/draw.ts';

type BoolToggle = Exclude<keyof DrawToggles, 'sound'>;
/** One switch per thing on screen, in display order. */
const TOGGLES: BoolToggle[] = ['names', 'hp', 'weapon', 'view', 'grenades', 'shots', 'bomb', 'deaths', 'killFeed', 'clock'];

export function ReplayOptions({ toggles, onChange }: { toggles: DrawToggles; onChange: (t: DrawToggles) => void }) {
  const { t } = useTranslation();
  return (
    <Popover.Root>
      <Popover.Trigger>
        <Button size="2" variant="surface" color="gray" aria-label={t('common.options')}>
          <i aria-hidden="true" className="bi bi-sliders app-icon"  /> {t('common.options')}
        </Button>
      </Popover.Trigger>
      <Popover.Content size="1" align="end" style={{ width: 236 }}>
        <Flex direction="column" gap="2">
          {TOGGLES.map((key) => (
            <Text key={key} as="label" size="2">
              <Flex gap="2" align="center" justify="between">
                {t(`replay.toggle.${key}`)}
                <Switch size="1" checked={toggles[key]} onCheckedChange={(v) => onChange({ ...toggles, [key]: v })} />
              </Flex>
            </Text>
          ))}
          <Box style={{ borderTop: '1px solid var(--gray-a5)', margin: '2px 0' }} />
          <Text as="label" size="2">
            <Flex gap="2" align="center" justify="between">
              {t('replay.sound')}
              <SegmentedControl.Root size="1" value={toggles.sound} onValueChange={(v) => onChange({ ...toggles, sound: v as DrawToggles['sound'] })}>
                <SegmentedControl.Item value="all">{t('replay.soundAll')}</SegmentedControl.Item>
                <SegmentedControl.Item value="focus">{t('replay.soundFocus')}</SegmentedControl.Item>
                <SegmentedControl.Item value="off">{t('replay.soundOff')}</SegmentedControl.Item>
              </SegmentedControl.Root>
            </Flex>
          </Text>
        </Flex>
      </Popover.Content>
    </Popover.Root>
  );
}
