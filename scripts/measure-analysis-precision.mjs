// Measure storage only; rounded datasets are never published as qualified analysis inputs.
import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import {createReadStream} from 'node:fs';
import {createInterface} from 'node:readline';
import {Readable,Writable} from 'node:stream';
import {pipeline} from 'node:stream/promises';
import {createGzip,createGunzip} from 'node:zlib';
export function roundMeasurement(value,digits) {
  if(digits===null)return value;
  const scale=10**digits;return Math.sign(value)*Math.round(Math.abs(value)*scale)/scale;
}
export async function measurePrecision(input) {
  const modes=[[null,null],[2,null],[3,null],[4,null],[3,3],[4,4]],results=[];
  for(const [positionDecimals,viewDecimals] of modes) {
    let bytes=0,gzipBytes=0;const lastViews=new Map(),lastPoints=new Map();
    async function* rows(){
      const source=createReadStream(input),decoded=input.endsWith('.gz')?source.pipe(createGunzip()):source;
      const lines=createInterface({input:decoded,crlfDelay:Infinity});
      try {for await(const line of lines) {
        const row=JSON.parse(line);
        if(positionDecimals!==null) {
          for(const view of Object.values(row.views??{})){view.eye=view.eye.map(n=>roundMeasurement(n,positionDecimals));view.view=view.view.map(n=>roundMeasurement(n,viewDecimals));}
          for(const [key,point] of Object.entries(row.points??{}))row.points[key]=point.map(n=>roundMeasurement(n,positionDecimals));
          for(const [field,previous] of [['views',lastViews],['points',lastPoints]])if(row[field]){
            for(const [key,value] of Object.entries(row[field])){const encoded=JSON.stringify(value);if(previous.get(key)===encoded)delete row[field][key];else previous.set(key,encoded);}
            if(!Object.keys(row[field]).length)delete row[field];
          }
        }
        const text=JSON.stringify(row)+'\n';bytes+=Buffer.byteLength(text);yield text;
      }}finally{lines.close();decoded.destroy();source.destroy();}
    }
    await pipeline(Readable.from(rows()),createGzip(),new Writable({write(chunk,_,next){gzipBytes+=chunk.length;next();}}));
    results.push({positionDecimals,viewDecimals,bytes,gzipBytes});
  }
  return results;
}
if(process.argv[1] && import.meta.url===(await import('node:url')).pathToFileURL(process.argv[1]).href){
  const [input,output]=process.argv.slice(2);assert(input&&output,'Expected BODY_JOURNAL SUMMARY.json');
  const result=await measurePrecision(input);await writeFile(output,JSON.stringify(result,null,2),{flag:'wx'});console.log(JSON.stringify(result));
}
