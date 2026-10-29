#!/usr/bin/env node
/**
 * Bake MediaPipe's face models into the critic's graph format.
 *
 *   node tools/bake-faces.mjs <face_detection_full_range.tflite> <face_landmarks_detector.tflite>
 *
 * Both are Apache-2.0. The detector is
 * https://storage.googleapis.com/mediapipe-assets/face_detection_full_range.tflite,
 * and finds faces up to about five metres away. The landmark model is inside
 * https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/1/face_landmarker.task
 * (a zip), and places 478 points on a face, which is where the eyes are read
 * from; unlike the older face_landmark.tflite, its eyelids follow a blink.
 *
 * The output is `web/src/lib/wasm/critic/face-{detector,landmarks}.bin`, in the
 * format `tools/tfg.mjs` describes.
 *
 * The weights are half floats in the models already and are copied as they
 * are, so nothing is lost: DEQUANTIZE (half to single) is folded away here,
 * once, rather than done at run time.
 */
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fromTflite, writeGraph } from './tfg.mjs';

const [detector, landmarks] = process.argv.slice(2);
if (!detector || !landmarks) {
  console.error('usage: node tools/bake-faces.mjs <face_detection_full_range.tflite> <face_landmarks_detector.tflite>');
  process.exit(1);
}
const dir = join(dirname(fileURLToPath(import.meta.url)), '../web/src/lib/wasm/critic');
for (const [name, path] of [['face-detector', detector], ['face-landmarks', landmarks]]) {
  const { bytes, ops, tensors, weights } = writeGraph(fromTflite(path));
  writeFileSync(join(dir, `${name}.bin`), bytes);
  console.log(`${name}.bin: ${ops} operators, ${tensors} tensors, ${weights} weights, ${bytes.length} bytes`);
}
