# Scoring networks

These are idealo's MobileNet NIMA models
(https://github.com/idealo/image-quality-assessment, `models/MobileNet`):
`weights_mobilenet_aesthetic_0.07.hdf5`, trained on AVA, and
`weights_mobilenet_technical_0.11.hdf5`, trained on TID2013. They were converted
with `tools/bake-nima.mjs`, which folds the batch normalisation into the
convolutions and stores the weights as 16-bit floats.

Like the original, they're licensed under the Apache License 2.0
(https://www.apache.org/licenses/LICENSE-2.0), Copyright 2018 idealo internet
GmbH. NIMA itself is described in Talebi and Milanfar, "NIMA: Neural Image
Assessment", IEEE Transactions on Image Processing, 2018.

# Face models

`face-detector.bin` is MediaPipe's full-range face detector
(`face_detection_full_range.tflite`), and `face-landmarks.bin` the landmark
model from MediaPipe's Face Landmarker (`face_landmarks_detector.tflite`, inside
`face_landmarker.task`), both from https://storage.googleapis.com/mediapipe-assets/
and https://storage.googleapis.com/mediapipe-models/. They were converted with
`tools/bake-faces.mjs`, which reads the TensorFlow Lite files and writes their
operators and half-float weights unchanged.

They're licensed under the Apache License 2.0
(https://www.apache.org/licenses/LICENSE-2.0), Copyright Google LLC.
