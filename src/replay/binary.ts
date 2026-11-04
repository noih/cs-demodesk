import type { PackedReplay } from './transfer.ts';

/** Read the disposable native cache only after its payload integrity check passes. */
export async function decodeReplayBinary(buffer: ArrayBuffer): Promise<PackedReplay> {
  buffer = await new Response(new Blob([buffer]).stream().pipeThrough(new DecompressionStream('gzip'))).arrayBuffer();
  if (buffer.byteLength < 80) throw new Error('Truncated replay binary');
  const bytes = new Uint8Array(buffer);
  if (new TextDecoder().decode(bytes.subarray(0, 8)) !== 'DDRPLY01') throw new Error('Unsupported replay binary');
  const view = new DataView(buffer);
  const metadataLength = Number(view.getBigUint64(32, true));
  const count = Number(view.getBigUint64(40, true));
  const start = 80 + Math.ceil(metadataLength / 8) * 8;
  if (!Number.isSafeInteger(metadataLength) || !Number.isSafeInteger(count) || metadataLength < 2 || start > buffer.byteLength || count !== (buffer.byteLength - start) / 8) throw new Error('Invalid replay binary size');
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes.subarray(80)));
  if (!digest.every((byte, i) => byte === bytes[48 + i])) throw new Error('Replay binary checksum mismatch');
  const metadata = JSON.parse(new TextDecoder().decode(bytes.subarray(80, 80 + metadataLength)));
  if (metadata.meta?.schemaVersion !== 7 || !Array.isArray(metadata.frames) || (metadata.smoke !== undefined && !Array.isArray(metadata.smoke))) throw new Error('Invalid replay binary metadata');
  return {...metadata, rows: new Float64Array(buffer, start, count)};
}
