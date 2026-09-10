/** Each load owns its worker and bitmaps; no map cache survives leaving the replay. */
export async function loadRadarImages(urls: string[], signal: AbortSignal): Promise<ImageBitmap[]> {
  signal.throwIfAborted();
  const worker = new Worker(new URL('./radar.worker.ts', import.meta.url), { type: 'module' });
  let abort: () => void = () => {};
  try {
    return await new Promise<ImageBitmap[]>((resolve, reject) => {
      abort = () => reject(signal.reason);
      signal.addEventListener('abort', abort, { once: true });
      worker.onmessage = ({ data }: MessageEvent<{ images?: ImageBitmap[]; error?: string }>) => {
        if (data.images) resolve(data.images);
        else reject(new Error(data.error ?? 'Radar processing failed'));
      };
      worker.onerror = (event) => reject(new Error(event.message));
      worker.postMessage(urls);
    });
  } finally {
    signal.removeEventListener('abort', abort);
    worker.terminate();
  }
}
