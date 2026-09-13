import test from 'node:test';
import assert from 'node:assert/strict';
import {decodeManifest} from './analysis-manifest.mjs';
function bitsWriter(){const bits=[];return {bits(value,n){for(let i=0;i<n;i++)bits.push(Math.floor(value/2**i)%2);},string(value){for(const c of Buffer.from(value))this.bits(c,8);this.bits(0,8);},bytes(){return Uint8Array.from({length:Math.ceil(bits.length/8)},(_,i)=>bits.slice(i*8,i*8+8).reduce((n,b,j)=>n+b*2**j,0));}};}
function wrap(body,compressed=false){let data=body;if(compressed){const b=bitsWriter();b.bits(0x53535a4c,32);b.bits(body.length,32);for(let i=0;i<body.length;i+=8){const count=Math.min(8,body.length-i);b.bits(count===8?0:1<<count,8);for(const x of body.slice(i,i+8))b.bits(x,8);if(count<8){b.bits(0,16);}}if(body.length%8===0){b.bits(1,8);b.bits(0,16);}data=b.bytes();}const w=bitsWriter();w.bits(compressed?1:0,1);w.bits(data.length,24);for(const b of data)w.bits(b,8);return w.bytes();}
test('manifest decodes uncompressed and LZSS literal payloads and refuses corruption',()=>{
 const b=bitsWriter();b.bits(1,16);b.bits(1,16);b.bits(1,16);b.string('vmdl');b.string('models/');b.bits(0,1);b.string('synthetic');b.bits(0,1);
 const raw=b.bytes(),plain=wrap(raw),compressed=wrap(raw,true);
 assert.deepEqual(decodeManifest(plain).entries,decodeManifest(compressed).entries);
 assert.equal(decodeManifest(plain).entries[0].path,'models/synthetic.vmdl');
 assert.equal(decodeManifest(plain).contentVersion,'unknown');
 for(let n=0;n<compressed.length;n++)assert.throws(()=>decodeManifest(compressed.slice(0,n)));
 const corrupt=compressed.slice();corrupt[4]^=1;assert.throws(()=>decodeManifest(corrupt));
 const index=raw.slice();index[index.length-1]|=64;assert.throws(()=>decodeManifest(wrap(index)));
});
