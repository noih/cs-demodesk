// Source 2 Viewer physics adapter; no scene, smoke, pose or rule dependency.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import path from 'node:path';
export const PHYSICS_VERSION='source2viewer-physics/0.1.0';
const vector = v => Array.isArray(v) && v.length === 3 && v.every(Number.isFinite);
const sub = (a,b) => a.map((n,i) => n-b[i]);
const dot = (a,b) => a.reduce((n,x,i) => n+x*b[i],0);
const cross = (a,b) => [a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];

export function intersect(start, end, a, b, c) {
  const direction = sub(end,start), e1 = sub(b,a), e2 = sub(c,a);
  const h = cross(direction,e2), determinant = dot(e1,h);
  if (Math.abs(determinant) < 1e-9) return null;
  const s = sub(start,a), u = dot(s,h)/determinant;
  if (u < 0 || u > 1) return null;
  const q = cross(s,e1), v = dot(direction,q)/determinant;
  if (v < 0 || u+v > 1) return null;
  const t = dot(e2,q)/determinant;
  return t > 1e-7 && t < 1-1e-7 ? t : null;
}

function accessor(gltf, bytes, index, type) {
  const a = gltf.accessors[index];
  assert(a && !a.sparse && !a.normalized, 'Unsupported sparse/normalized accessor');
  const v = gltf.bufferViews[a.bufferView];
  assert(v && v.buffer === 0, 'Expected one external buffer');
  const width = type === 'VEC3' ? 3 : 1;
  const size = a.componentType === 5126 || a.componentType === 5125 ? 4 : a.componentType === 5123 ? 2 : a.componentType === 5121 ? 1 : 0;
  assert(a.type === type && size > 0 && (type === 'VEC3' ? a.componentType === 5126 : [5121,5123,5125].includes(a.componentType)), 'Unsupported accessor type');
  const offset = (v.byteOffset ?? 0)+(a.byteOffset ?? 0), stride = v.byteStride ?? size*width;
  const end = offset + Math.max(0,a.count-1)*stride + (a.count ? size*width : 0);
  assert([v.byteOffset ?? 0,a.byteOffset ?? 0,offset,stride,a.count,v.byteLength].every(n => Number.isSafeInteger(n) && n >= 0) && stride >= size*width, 'Invalid accessor layout');
  assert(end <= (v.byteOffset ?? 0)+v.byteLength && end <= bytes.length, 'Accessor exceeds buffer');
  return { count:a.count, at(i) {
    assert(Number.isSafeInteger(i) && i >= 0 && i < a.count, 'Invalid vertex index');
    const pos = offset+i*stride;
    if (type === 'VEC3') {
      const result = [bytes.readFloatLE(pos),bytes.readFloatLE(pos+4),bytes.readFloatLE(pos+8)];
      assert(vector(result),'Invalid position');
      return result;
    }
    return size === 4 ? bytes.readUInt32LE(pos) : size === 2 ? bytes.readUInt16LE(pos) : bytes.readUInt8(pos);
  } };
}

export function trace(gltf, bytes, start, end) {
  assert(vector(start) && vector(end) && dot(sub(end,start),sub(end,start)) > 0, 'Expected distinct finite endpoints');
  assert(gltf.asset?.generator?.startsWith('Source 2 Viewer '), 'Expected Source 2 Viewer physics export');
  const nodes = gltf.scenes[gltf.scene ?? 0]?.nodes;
  assert(nodes?.length && nodes.length === gltf.nodes.length, 'Expected a flat physics scene');
  const matrix = gltf.nodes[nodes[0]].matrix;
  assert(Array.isArray(matrix) && matrix.length === 16 && matrix.every(Number.isFinite), 'Expected exporter root matrix');
  const hits = [];
  let triangles = 0;
  for (const index of nodes) {
    const node = gltf.nodes[index];
    assert(!node.children?.length && !node.translation && !node.rotation && !node.scale, 'Unsupported per-shape transform');
    assert.deepEqual(node.matrix,matrix,'Physics nodes must share the export coordinate conversion');
    assert(typeof node.extras?.SurfaceProperty === 'string', 'Expected physics surface metadata');
    let nearest = null;
    for (const primitive of gltf.meshes[node.mesh].primitives) {
      assert((primitive.mode ?? 4) === 4, 'Expected triangles');
      const vertices = accessor(gltf,bytes,primitive.attributes.POSITION,'VEC3');
      const indices = accessor(gltf,bytes,primitive.indices,'SCALAR');
      assert(indices.count % 3 === 0, 'Incomplete triangles');
      // ponytail: linear scan for individual diagnostic rays; add a BVH before per-tick scoring.
      for (let i=0; i<indices.count; i+=3) {
        const t = intersect(start,end,vertices.at(indices.at(i)),vertices.at(indices.at(i+1)),vertices.at(indices.at(i+2)));
        triangles++;
        if (t !== null && (nearest === null || t < nearest)) nearest = t;
      }
    }
    if (nearest !== null) hits.push({surface:node.extras.SurfaceProperty,interactAs:node.extras.InteractAs ?? [],fraction:nearest});
  }
  hits.sort((a,b) => a.fraction-b.fraction || a.surface.localeCompare(b.surface));
  return {coordinateSpace:'Source 2 exporter local coordinates (game units)',visibility:'unknown',triangles,hits};
}

export async function loadPhysics(filename) {
  const gltf = JSON.parse((await readFile(filename,'utf8')).replace(/^\uFEFF/,''));
  assert(gltf.buffers?.length === 1,'Expected one external buffer');
  const uri = gltf.buffers[0].uri;
  assert(typeof uri === 'string' && uri === path.basename(uri) && !/[\\/:]/.test(uri) && uri !== '..','Expected a sibling binary buffer');
  const bytes = await readFile(path.join(path.dirname(filename),uri));
  return {gltf,bytes};
}
