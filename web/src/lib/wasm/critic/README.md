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

The developer also loads `face-detector.bin`, to read what skin looks like on a
photograph's faces.

# Segmenter

`segmenter.bin` is MediaPipe's DeepLab v3 image segmenter (`deeplab_v3.tflite`,
https://storage.googleapis.com/mediapipe-models/image_segmenter/deeplab_v3/float32/1/deeplab_v3.tflite),
which labels a 257 × 257 picture with PASCAL VOC's 21 classes; the developer's
"Subject" filters follow whatever it does not call background. Converted with
`tools/bake-segmenter.mjs`, the weights narrowed to half floats. Apache License
2.0, Copyright Google LLC.

# Raw denoiser

`denoiser.bin` is PMRID, from Wang et al., "Practical Deep Raw Image Denoising
on Mobile Devices", ECCV 2020 (https://github.com/megvii-research/PMRID,
`models/torch_pretrained.ckp`). Converted with `tools/bake-denoiser.mjs`, which
reads the PyTorch checkpoint and lays the network out for 256 × 256 tiles of a
packed Bayer mosaic, the weights narrowed to half floats. Apache License 2.0,
Copyright Megvii Inc.
