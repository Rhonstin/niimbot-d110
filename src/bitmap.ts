export const HEAD_WIDTH = 96;
export const DPI = 203;

export function mmToPixels(mm: number): number {
  if (!Number.isFinite(mm) || mm <= 0) throw new Error("Invalid label size.");
  return Math.round((mm * DPI) / 25.4);
}

function black(pixels: ArrayLike<number>, i: number, threshold: number): boolean {
  const alpha = pixels[i + 3] / 255;
  const luminance = (0.299 * pixels[i] + 0.587 * pixels[i + 1] + 0.114 * pixels[i + 2]) * alpha + 255 * (1 - alpha);
  return luminance < threshold;
}

function validate(pixels: ArrayLike<number>, width: number, height: number, threshold: number): void {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || width > 800 || height < 1 || height > HEAD_WIDTH) {
    throw new Error("Invalid D110 image dimensions.");
  }
  if (pixels.length !== width * height * 4) throw new Error("Invalid pixel data.");
  if (!Number.isInteger(threshold) || threshold < 1 || threshold > 254) throw new Error("Invalid monochrome threshold.");
}

export function monochrome(
  pixels: ArrayLike<number>, width: number, height: number,
  threshold: number, flip: boolean, offset: number,
): Uint8ClampedArray<ArrayBuffer> {
  validate(pixels, width, height, threshold);
  if (!Number.isInteger(offset) || Math.abs(offset) > 64) throw new Error("Invalid print offset.");
  const output = new Uint8ClampedArray(pixels.length).fill(255);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const shiftedX = x - offset;
      if (shiftedX < 0 || shiftedX >= width) continue;
      const sx = flip ? width - 1 - shiftedX : shiftedX;
      const sy = flip ? height - 1 - y : y;
      if (black(pixels, (sy * width + sx) * 4, threshold)) {
        const i = (y * width + x) * 4;
        output[i] = output[i + 1] = output[i + 2] = 0;
      }
    }
  }
  return output;
}

/** Rotate the landscape preview clockwise to printhead-width × feed-length.
 *  Each byte is MSB-first and a set bit means black, exactly as on the wire. */
export function packLandscape(pixels: ArrayLike<number>, width: number, height: number): number[] {
  validate(pixels, width, height, 128);
  const stride = Math.ceil(height / 8);
  const packed = new Array<number>(stride * width).fill(0);
  for (let row = 0; row < width; row++) {
    for (let column = 0; column < height; column++) {
      const i = ((height - 1 - column) * width + row) * 4;
      if (black(pixels, i, 128)) packed[row * stride + (column >> 3)] |= 0x80 >> (column & 7);
    }
  }
  return packed;
}
