import test from "node:test";
import assert from "node:assert/strict";
import { mmToPixels, monochrome, packLandscape } from "./bitmap.ts";

function pixels(width: number, height: number, points: [number, number][]): Uint8ClampedArray {
  const data = new Uint8ClampedArray(width * height * 4).fill(255);
  for (const [x, y] of points) {
    const i = (y * width + x) * 4;
    data[i] = data[i + 1] = data[i + 2] = 0;
  }
  return data;
}

test("30 mm becomes 240 feed rows at 203 dpi", () => {
  assert.equal(mmToPixels(30), 240);
  assert.equal(mmToPixels(50), 400);
  assert.throws(() => mmToPixels(NaN));
});

test("landscape rotation preserves all four corners and MSB-first bits", () => {
  assert.deepEqual(packLandscape(pixels(2, 8, [[0, 0]]), 2, 8), [1, 0]);
  assert.deepEqual(packLandscape(pixels(2, 8, [[0, 7]]), 2, 8), [128, 0]);
  assert.deepEqual(packLandscape(pixels(2, 8, [[1, 0]]), 2, 8), [0, 1]);
  assert.deepEqual(packLandscape(pixels(2, 8, [[1, 7]]), 2, 8), [0, 128]);
});

test("a D110 landscape label packs into 12 bytes for every feed row", () => {
  const output = packLandscape(pixels(240, 96, [[239, 95]]), 240, 96);
  assert.equal(output.length, 2880);
  assert.equal(output[239 * 12], 128);
  assert.equal(output.filter(x => x !== 0).length, 1);
});

test("transparent black is white after alpha compositing", () => {
  assert.deepEqual(packLandscape(new Uint8ClampedArray([0, 0, 0, 0]), 1, 1), [0]);
});

test("calibration shifts along feed without wrapping pixels", () => {
  const input = pixels(3, 8, [[0, 7]]);
  assert.deepEqual(packLandscape(monochrome(input, 3, 8, 128, false, 1), 3, 8), [0, 128, 0]);
  assert.deepEqual(packLandscape(monochrome(input, 3, 8, 128, false, -1), 3, 8), [0, 0, 0]);
});

test("180 degree flip rotates the image before printer rotation", () => {
  const input = pixels(3, 8, [[0, 7]]);
  assert.deepEqual(packLandscape(monochrome(input, 3, 8, 128, true, 0), 3, 8), [0, 0, 1]);
});

test("threshold produces the same black pixels for preview and packed output", () => {
  const input = new Uint8ClampedArray([160, 160, 160, 255]);
  assert.equal(packLandscape(monochrome(input, 1, 1, 128, false, 0), 1, 1)[0], 0);
  assert.equal(packLandscape(monochrome(input, 1, 1, 180, false, 0), 1, 1)[0], 128);
});

test("invalid geometry, pixel data and threshold are rejected", () => {
  assert.throws(() => packLandscape([], 2, 8));
  assert.throws(() => packLandscape(pixels(2, 8, []), 801, 8));
  assert.throws(() => monochrome(pixels(2, 8, []), 2, 8, 255, false, 0));
});
