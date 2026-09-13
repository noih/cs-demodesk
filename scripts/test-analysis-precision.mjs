import test from 'node:test';
import assert from 'node:assert/strict';
import {roundMeasurement,measurePrecision} from './measure-analysis-precision.mjs';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
test('precision experiment rounds signed coordinates, preserves angles and measures without emitting datasets',async t=>{
  assert.equal(roundMeasurement(-1.2345,3),-1.235);
  assert.equal(roundMeasurement(1.2345,3),1.235);
  assert.equal(roundMeasurement(1.23456789,null),1.23456789);
  const root=await mkdtemp(path.join(tmpdir(),'demodesk-precision-'));t.after(()=>rm(root,{recursive:true,force:true}));
  const input=path.join(root,'body.ndjson');
  await writeFile(input,Array.from({length:30},(_,tick)=>JSON.stringify({tick,views:{p:{eye:[1.23456789+tick*0.0000001,2.123456789,3.123456789],view:[0.123456789,1.23456789]}},points:{p:[12.3456789,34.5678912,56.7891234]}})+'\n').join(''));
  const sizes=await measurePrecision(input);
  assert(sizes[2].bytes<sizes[0].bytes);
  assert(sizes[4].bytes<sizes[2].bytes);
  assert(sizes.every(size=>size.gzipBytes>0&&size.gzipBytes<size.bytes));
});
