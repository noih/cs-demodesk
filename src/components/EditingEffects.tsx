import { useId } from 'react';
import { Box, Flex, Grid, Switch, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';

const EFFECTS = ['opening', 'combat', 'teamwork', 'push'] as const;
export function EditingEffects({ enabled, onChange, opening, onOpeningChange, combat, onCombatChange, push, onPushChange, hasSizeLimit }: {enabled:boolean;onChange:(enabled:boolean)=>void;opening:boolean;onOpeningChange:(enabled:boolean)=>void;combat:boolean;onCombatChange:(enabled:boolean)=>void;push:boolean;onPushChange:(enabled:boolean)=>void;hasSizeLimit:boolean}) {
  const { t } = useTranslation();
  const id = useId();

  return (
    <details className="editing-effects">
      <summary>
        <Text weight="medium">{t('editingEffects.title')}</Text>
      </summary>
      <Box pt="2">
        {hasSizeLimit && (enabled || opening || combat || push) && (
          <Text as="p" role="status" size="1" color="amber" mt="0" mb="2">{t('editingEffects.sizeLimitWarning')}</Text>
        )}
        <Grid columns={{ initial: '1', sm: '2' }} gap="3">
          {EFFECTS.map(key => (
            <label key={key} htmlFor={`${id}-${key}`} className="editing-effect-card" data-enabled={(key === 'teamwork' && enabled) || (key === 'opening' && opening) || (key === 'combat' && combat) || (key === 'push' && push)}>
              <Flex as="span" justify="between" align="start" gap="4" mb="2">
                <Text id={`${id}-${key}-title`} size="2" weight="medium">{t(`editingEffects.${key}.title`)}</Text>
                <Switch id={`${id}-${key}`} size="1" checked={(key === 'teamwork' && enabled) || (key === 'opening' && opening) || (key === 'combat' && combat) || (key === 'push' && push)} aria-labelledby={`${id}-${key}-title`} aria-describedby={`${id}-${key}-hint`}
                  onCheckedChange={key === 'opening' ? onOpeningChange : key === 'combat' ? onCombatChange : key === 'push' ? onPushChange : onChange} />
              </Flex>
              <Text as="span" style={{ display: 'block' }} id={`${id}-${key}-hint`} size="1" color="gray">{t(`editingEffects.${key}.hint`)}</Text>
            </label>
          ))}
        </Grid>
      </Box>
    </details>
  );
}
