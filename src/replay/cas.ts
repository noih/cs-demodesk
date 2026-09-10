/*!
Adapted from AMD FidelityFX CAS (sharpen-only, per-channel weights):
https://github.com/GPUOpen-Effects/FidelityFX-CAS/blob/master/ffx-cas/ffx_cas.h
Copyright (c) 2017-2019 Advanced Micro Devices, Inc. All rights reserved.
Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation
files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy,
modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following conditions:
The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
*/

/** CAS on RGB; preserve alpha and avoid sampling hidden colors outside the map. */
export function sharpenCas(source: Uint8ClampedArray, width: number, height: number, sharpness = 0.5): Uint8ClampedArray<ArrayBuffer> {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1 || source.length !== width * height * 4) throw new Error('Invalid CAS image dimensions');
  if (!Number.isFinite(sharpness) || sharpness < 0 || sharpness > 1) throw new Error('CAS sharpness must be between 0 and 1');
  const output = new Uint8ClampedArray(source);
  // AMD's documented gamma-2 approximation: filter in linear light, then encode with sqrt.
  const linear = Float64Array.from({ length: 256 }, (_, i) => (i / 255) ** 2);
  const peak = -1 / (8 - 3 * sharpness);
  for (let y = 0; y < height; y++) {
    const above = Math.max(0, y - 1) * width * 4;
    const below = Math.min(height - 1, y + 1) * width * 4;
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      if (!source[i + 3]) continue;
      const b = above + x * 4;
      const h = below + x * 4;
      const d = i - (x > 0 ? 4 : 0);
      const f = i + (x + 1 < width ? 4 : 0);
      for (let c = 0; c < 3; c++) {
        const e = linear[source[i + c]!]!;
        const bv = source[b + 3] ? linear[source[b + c]!]! : e;
        const hv = source[h + 3] ? linear[source[h + c]!]! : e;
        const dv = source[d + 3] ? linear[source[d + c]!]! : e;
        const fv = source[f + 3] ? linear[source[f + c]!]! : e;
        const min = Math.min(e, bv, hv, dv, fv);
        const max = Math.max(e, bv, hv, dv, fv);
        const weight = max > 0 ? peak * Math.sqrt(Math.max(0, Math.min(min, 1 - max) / max)) : 0;
        const value = (e + weight * (bv + hv + dv + fv)) / (1 + 4 * weight);
        output[i + c] = 255 * Math.sqrt(Math.max(0, Math.min(1, value)));
      }
    }
  }
  return output;
}
