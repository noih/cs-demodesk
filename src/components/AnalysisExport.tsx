import { useEffect, useState } from 'react';
import { Button, Flex, Switch, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, errorText, type Assessment, type RuleClips, type Status } from '../api.ts';
import { ExportDialog } from './ExportDialog.tsx';

export function AnalysisExport({record,status,onSetup,onRendered}: {record:Assessment;status?:Status;onSetup:()=>void;onRendered:()=>void}) {
  const {t}=useTranslation();
  const [open,setOpen]=useState(false);
  const [rules,setRules]=useState<string[]>([]);
  const [mergeRules,setMergeRules]=useState(false);
  const [groups,setGroups]=useState<RuleClips[]>([]);
  const [loading,setLoading]=useState(false);
  const [error,setError]=useState<string>();
  const observed=record.checks.filter(check=>check.state!=='unavailable'&&check.state!=='failed'&&check.occurrences.length>0);
  useEffect(()=>{
    let alive=true;setError(undefined);
    if(!open||!rules.length){setGroups([]);setLoading(false);return;}
    setLoading(true);
    void api.analysisClips(record.demoId,{playerId:record.playerId,assessmentId:record.id,ruleIds:rules},mergeRules)
      .then(groups=>{if(alive)setGroups(groups);}).catch(error=>{if(alive)setError(errorText(error));}).finally(()=>{if(alive)setLoading(false);});
    return ()=>{alive=false;};
  },[open,rules,mergeRules,record.demoId,record.playerId,record.id]);
  if(!observed.length)return null;
  const clips=groups.flatMap(group=>group.highlights);
  const expectedGroups=mergeRules && rules.length>1 ? [rules.join('+')] : rules;
  return <>
    <Button variant="outline" onClick={()=>{setRules([]);setMergeRules(false);setOpen(true);}}>{t('highlights.export')}</Button>
    <ExportDialog open={open} onOpenChange={setOpen} count={clips.length} seconds={clips.reduce((sum,h)=>sum+(h.endTick-h.startTick)/record.tickRate,0)}
      status={status} onSetup={onSetup} onSubmitted={onRendered} forceMerge disabled={loading||!!error||!groups.length||groups.length!==expectedGroups.length||groups.some(group=>!expectedGroups.includes(group.ruleId))}
      onSubmit={options=>api.renderAnalysis(record.demoId,{playerId:record.playerId,assessmentId:record.id,ruleIds:rules},{...options,merge:mergeRules})}>
      <Flex direction="column" gap="3" mt="3">
        <Text size="2" color="gray">{t('highlights.selectRulesHint')}</Text>
        <Flex wrap="wrap" gap="2" aria-busy={loading}>{observed.map(check=><Button key={check.definition.id} type="button" aria-pressed={rules.includes(check.definition.id)}
          color={rules.includes(check.definition.id)?undefined:'gray'} variant={rules.includes(check.definition.id)?'solid':'outline'}
          onClick={()=>setRules(previous=>previous.includes(check.definition.id)?previous.filter(id=>id!==check.definition.id):[...previous,check.definition.id])}>
          {t(`scoring.ruleNames.${check.definition.id}`,{defaultValue:check.definition.name})} · {check.occurrences.length}
        </Button>)}</Flex>
        <Text as="label" size="2"><Flex gap="2" align="center">
          <Switch size="1" checked={mergeRules} disabled={rules.length<2} onCheckedChange={setMergeRules} />
          {t('highlights.mergeRules')}
        </Flex></Text>
        {error && <Text role="alert" color="red">{error}</Text>}
      </Flex>
    </ExportDialog>
  </>;
}
