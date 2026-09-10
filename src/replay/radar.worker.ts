import { sharpenCas } from './cas.ts';

self.onmessage = async (event: MessageEvent<string[]>) => {
  const images: ImageBitmap[] = [];
  try {
    for (const url of event.data) {
      const response = await fetch(url);
      if (!response.ok) throw new Error(`radar image: HTTP ${response.status}`);
      const original = await createImageBitmap(await response.blob());
      try {
        const canvas = new OffscreenCanvas(original.width, original.height);
        const ctx = canvas.getContext('2d')!;
        ctx.drawImage(original, 0, 0);
        const source = ctx.getImageData(0, 0, canvas.width, canvas.height);
        const pixels = sharpenCas(source.data, canvas.width, canvas.height);
        // Pixel-backed bitmaps remain drawable after the worker and its canvas are destroyed.
        images.push(await createImageBitmap(new ImageData(pixels, canvas.width, canvas.height)));
        original.close();
      } catch (error) {
        console.warn('Radar CAS processing failed; using the original image', error);
        images.push(original);
      }
    }
    self.postMessage({ images }, { transfer: images });
  } catch (error) {
    images.forEach(image => image.close());
    self.postMessage({ error: error instanceof Error ? error.message : String(error) });
  }
};
