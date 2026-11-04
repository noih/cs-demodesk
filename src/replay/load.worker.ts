import { decodeReplayBinary } from './binary.ts';
import { packReplay } from './transfer.ts';
self.onmessage = async (event: MessageEvent<{url: string; binary?: boolean}>) => {
  try {
    const response = await fetch(event.data.url);
    if (!response.ok) throw new Error(`replay file: HTTP ${response.status}`);
    const data = event.data.binary ? await decodeReplayBinary(await response.arrayBuffer()) : packReplay(await response.json());
    self.postMessage({ data }, { transfer: [data.rows.buffer] });
  } catch (error) {
    self.postMessage({ error: error instanceof Error ? error.message : String(error) });
  }
};
