#!/usr/bin/env node
/**
 * Bake MediaPipe's DeepLab v3 into the critic's graph format, for the
 * developer's subject masks.
 *
 *   node tools/bake-segmenter.mjs <deeplab_v3.tflite>
 *
 * Apache-2.0, from
 * https://storage.googleapis.com/mediapipe-models/image_segmenter/deeplab_v3/float32/1/deeplab_v3.tflite:
 * a MobileNet v2 DeepLab that labels each pixel of a 257 × 257 picture as one
 * of PASCAL VOC's 21 classes (background, people, animals, vehicles and some
 * furniture). The subject is whatever is not background.
 *
 * The output is `web/src/lib/wasm/critic/segmenter.bin`, in the format
 * `tools/tfg.mjs` describes, the weights narrowed to half floats.
 */
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fromTflite, writeGraph } from './tfg.mjs';

const [path] = process.argv.slice(2);
if (!path) {
  console.error('usage: node tools/bake-segmenter.mjs <deeplab_v3.tflite>');
  process.exit(1);
}
const out = join(dirname(fileURLToPath(import.meta.url)), '../web/src/lib/wasm/critic/segmenter.bin');
const { bytes, ops, tensors, weights } = writeGraph(fromTflite(path));
writeFileSync(out, bytes);
console.log(`segmenter.bin: ${ops} operators, ${tensors} tensors, ${weights} weights, ${bytes.length} bytes`);
