// Source 2 resource manifests: path identity, never resource-content attestation.
// Wire layout cross-checked against skadistats/clarity Resources.java and LZSS.java.
import assert from 'node:assert/strict';
import {modelResourceId} from './resolve-analysis-models.mjs';
export const MANIFEST_VERSION='0.1.0';
function reader(bytes) {
  let offset=0;
  return {
    bits(n) {
      assert(Number.isInteger(n) && n>=0 && n<=32 && offset+n<=bytes.length*8,'Truncated manifest');
      let value=0;
      for(let bit=0;bit<n;bit++,offset++) value+=(bytes[offset>>3]>>(offset%8)&1)*2**bit;
      return value;
    },
    string() {
      const values=[];
      for(let i=0;i<4096;i++){const value=this.bits(8);if(value===0)return new TextDecoder('utf-8',{fatal:true}).decode(Uint8Array.from(values));values.push(value);}
      throw Error('Oversized manifest string');
    },
    finish(){assert(bytes.length*8-offset<8,'Unexpected manifest trailing data');while(offset<bytes.length*8)assert(this.bits(1)===0,'Nonzero manifest padding');},
  };
}
export function decodeManifest(bytes) {
  assert(bytes instanceof Uint8Array && bytes.length<=16*1024*1024,'Invalid manifest bytes');
  const wire=reader(bytes),compressed=wire.bits(1)!==0,size=wire.bits(24);
  assert(size>0 && size<=16*1024*1024,'Invalid manifest size');
  assert(bytes.length===Math.ceil((25+size*8)/8),'Manifest payload size mismatch');
  let data;
  if(compressed) {
    assert(wire.bits(32)===0x53535a4c,'Invalid LZSS magic');
    const outputSize=wire.bits(32);
    assert(outputSize>0 && outputSize<=16*1024*1024,'Invalid LZSS output size');
    data=new Uint8Array(outputSize);let offset=0,done=false;
    while(!done) {
      const command=wire.bits(8);
      for(let bit=0;bit<8;bit++) {
        if(!(command & 1<<bit)){assert(offset<data.length,'LZSS output overflow');data[offset++]=wire.bits(8);continue;}
        const a=wire.bits(8),b=wire.bits(8),distance=1+((a<<4)|(b>>4)),count=1+(b&15);
        if(count===1){done=true;break;}
        assert(distance<=offset && offset+count<=data.length,'Invalid LZSS back reference');
        for(let i=0;i<count;i++,offset++)data[offset]=data[offset-distance];
      }
    }
    assert(offset===data.length,'Incomplete LZSS output');
  } else {data=Uint8Array.from({length:size},()=>wire.bits(8));}
  wire.finish();
  const body=reader(data),types=body.bits(16),directories=body.bits(16),count=body.bits(16);
  assert(types>0 && directories>0,'Missing manifest dictionaries');
  const extensions=Array.from({length:types},()=>body.string()),dirs=Array.from({length:directories},()=>body.string());
  const typeBits=Math.max(1,Math.ceil(Math.log2(types))),dirBits=Math.max(1,Math.ceil(Math.log2(directories)));
  const entries=[];
  for(let i=0;i<count;i++) {
    const dir=body.bits(dirBits),name=body.string(),ext=body.bits(typeBits);
    assert(dir<dirs.length && ext<extensions.length,'Invalid manifest dictionary index');
    const path=dirs[dir]+name+'.'+extensions[ext];
    assert(path.length<=4096 && !path.startsWith('/') && !path.includes('\\') && !path.split('/').some(p=>p==='..' || p==='.' || !p),'Unsafe manifest path');
    entries.push({path,resourceId:extensions[ext]==='vmdl'?modelResourceId(path+'_c'):null});
  }
  body.finish();
  return {compressed,entries,contentVersion:'unknown'};
}
