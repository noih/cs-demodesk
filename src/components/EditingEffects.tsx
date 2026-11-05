import { useId } from 'react';
import { Box, Flex, Grid, Switch, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';

const EFFECTS = ['opening', 'combat', 'teamwork', 'push'] as const;
export function EditingEffects({ enabled, onChange, opening, onOpeningChange, combat, onCombatChange, push, onPushChange }: {enabled:boolean;onChange:(enabled:boolean)=>void;opening:boolean;onOpeningChange:(enabled:boolean)=>void;combat:boolean;onCombatChange:(enabled:boolean)=>void;push:boolean;onPushChange:(enabled:boolean)=>void}) {
  const { t } = useTranslation();
  const id = useId();

  return (
    <details className="editing-effects">
      <summary>
        <Text weight="medium">{t('editingEffects.title')}</Text>
      </summary>
      <Box pt="3">

        <Grid columns={{ initial: '1', sm: '2', lg: '4' }} gap="3">
          {EFFECTS.map(key => (
            <Box key={key} className="editing-effect-card" data-enabled={(key === 'teamwork' && enabled) || (key === 'opening' && opening) || (key === 'combat' && combat) || (key === 'push' && push)}>
              <Flex justify="between" align="center" gap="3" mb="2">
                <Text as="label" htmlFor={`${id}-${key}`} size="2" weight="medium">{t(`editingEffects.${key}.title`)}</Text>
                <Switch id={`${id}-${key}`} size="1" checked={(key === 'teamwork' && enabled) || (key === 'opening' && opening) || (key === 'combat' && combat) || (key === 'push' && push)} aria-describedby={`${id}-${key}-hint`}
                  onCheckedChange={key === 'opening' ? onOpeningChange : key === 'combat' ? onCombatChange : key === 'push' ? onPushChange : onChange} />
              </Flex>
              <Text as="p" id={`${id}-${key}-hint`} size="1" color="gray">{t(`editingEffects.${key}.hint`)}</Text>
            </Box>
          ))}
        </Grid>
      </Box>
    </details>
  );
}
