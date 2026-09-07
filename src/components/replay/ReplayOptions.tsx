import { Box, Button, Flex, Popover, SegmentedControl, Switch, Text } from '@radix-ui/themes';
import { MixerHorizontalIcon } from '@radix-ui/react-icons';
import type { DrawToggles } from '../../replay/draw.ts';

type BoolToggle = Exclude<keyof DrawToggles, 'sound'>;
/** One switch per thing on screen, in display order. */
const LABELS: Array<[BoolToggle, string]> = [
  ['names', '玩家名字'],
  ['hp', '血量'],
  ['weapon', '手持武器'],
  ['view', '視角方向'],
  ['grenades', '道具'],
  ['shots', '射擊'],
  ['bomb', '炸彈'],
  ['deaths', '死亡位置'],
  ['killFeed', '擊殺訊息'],
  ['clock', '回合時間'],
];

export function ReplayOptions({ toggles, onChange }: { toggles: DrawToggles; onChange: (t: DrawToggles) => void }) {
  return (
    <Popover.Root>
      <Popover.Trigger>
        <Button size="2" variant="surface" color="gray" aria-label="選項">
          <MixerHorizontalIcon /> 選項
        </Button>
      </Popover.Trigger>
      <Popover.Content size="1" align="end" style={{ width: 236 }}>
        <Flex direction="column" gap="2">
          {LABELS.map(([key, label]) => (
            <Text key={key} as="label" size="2">
              <Flex gap="2" align="center" justify="between">
                {label}
                <Switch size="1" checked={toggles[key]} onCheckedChange={(v) => onChange({ ...toggles, [key]: v })} />
              </Flex>
            </Text>
          ))}
          <Box style={{ borderTop: '1px solid var(--gray-a5)', margin: '2px 0' }} />
          <Text as="label" size="2">
            <Flex gap="2" align="center" justify="between">
              聲音範圍
              <SegmentedControl.Root size="1" value={toggles.sound} onValueChange={(v) => onChange({ ...toggles, sound: v as DrawToggles['sound'] })}>
                <SegmentedControl.Item value="all">全部</SegmentedControl.Item>
                <SegmentedControl.Item value="focus">跟隨</SegmentedControl.Item>
                <SegmentedControl.Item value="off">關</SegmentedControl.Item>
              </SegmentedControl.Root>
            </Flex>
          </Text>
        </Flex>
      </Popover.Content>
    </Popover.Root>
  );
}
