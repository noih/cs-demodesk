import { format } from 'date-fns';
import { useState } from 'react';
import { Badge, Button, Flex, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, errorText, type AnalysisJob, type DemoMeta } from '../api.ts';

export function AnalysisQueueList({jobs,demos,onSelect,onChanged}: {
  jobs:AnalysisJob[]; demos:DemoMeta[]; onSelect:(id:string)=>void; onChanged:()=>Promise<void>;
}) {
  const {t}=useTranslation();
  const [pending,setPending]=useState<string>();
  const [error,setError]=useState<string>();
  const queued=jobs.filter(job=>job.status==='queued').sort((a,b)=>a.sequence-b.sequence);
  const running=jobs.filter(job=>job.status==='running');
  const ordered=[...running,...queued,...jobs.filter(job=>job.status==='error'||job.status==='done').sort((a,b)=>b.sequence-a.sequence)];
  const retry=async(job:AnalysisJob)=>{
    setPending(job.id);setError(undefined);
    try {await api.scoreMatch(job.demoId);await onChanged();}
    catch(error){setError(errorText(error));}
    finally{setPending(undefined);}
  };
  return <>
      {error && <Text color="red" role="alert">{error}</Text>}
      <div className="queue-list">{ordered.length ? ordered.map(job=>{
        const demo=demos.find(demo=>demo.id===job.demoId);
        return <section className="queue-item" key={job.id} data-analysis-job={job.id}>
          <Flex justify="between" gap="2" align="center"><Text weight="bold">{demo?.mapName ?? '—'} · {format(new Date(demo?.matchTimeMs ?? demo?.createdMs ?? job.createdAt),'yyyy-MM-dd HH:mm')}</Text>
            <Badge color={job.status==='error'?'red':job.status==='done'?'green':'amber'}>{job.status==='queued'?t('scoring.queue.waiting',{position:queued.findIndex(j=>j.id===job.id)+1}):t(`scoring.queue.${job.status}`)}</Badge>
          </Flex>
          {job.step && <Text as="p" size="2">{t('scoring.progress',{step:job.step,total:3})} · {t(`scoring.steps.${job.step}`)}</Text>}
          {job.error && <Text as="p" color="red" style={{overflowWrap:'anywhere'}}>{job.error}</Text>}
          <Flex justify="end" gap="2" mt="2">
            {job.status==='error' && <Button variant="soft" disabled={!demo||!!pending||jobs.some(j=>j.demoId===job.demoId&&(j.status==='running'||j.status==='queued'))} onClick={()=>void retry(job)}>{t('scoring.queue.retry')}</Button>}
            <Button variant="outline" disabled={!demo} onClick={()=>{onSelect(job.demoId);}}>{t('ui.goToDemo')}</Button>
          </Flex>
        </section>;
      }):<Text>{t('ui.queueEmpty')}</Text>}</div>
  </>;
}
