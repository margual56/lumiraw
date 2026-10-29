# critic

NIMA photo scoring and face checks for the culling page, built into a wasm module of its own
so the main one doesn't grow and nobody downloads it until they cull.

```rust
let net = critic::Net::parse(&std::fs::read("nima-aesthetic.bin")?)?;
let score = net.score(&rgba, 224, 224);   // any RGBA picture, squashed to 224x224 first
println!("{:.2} ± {:.2}", score.mean, score.std);
```

The network is MobileNet v1 as Keras 2.1 builds it (every 3×3 layer padded by
one on each side), written out by hand in `src/net.rs`: a 3×3 stem, depthwise
and pointwise layers, average pooling and a ten-way softmax over the marks
1 to 10. `score.feature` is the pooled 1024-value vector before the classifier,
which the page uses to tell frames of a burst apart from different shots.

The weights are idealo's MobileNet NIMA models, converted by
`tools/bake-nima.mjs` into the flat format described at the top of that script
(BatchNorm folded in, f16). They live in `web/src/lib/wasm/critic/`.

`src/faces.rs` finds faces and reads their eyes, with MediaPipe's face
detector and landmark model run by `src/graph.rs`, a small executor for the
handful of TensorFlow Lite operators they use. `tools/bake-faces.mjs` writes them
in its format.

```rust
let detector = critic::graph::Graph::parse(&std::fs::read("face-detector.bin")?)?;
let landmarks = critic::graph::Graph::parse(&std::fs::read("face-landmarks.bin")?)?;
for face in critic::faces::find(&detector, Some(&landmarks), &rgba, w, h) {
    println!("{:?} eyes {:?}", (face.x0, face.y0, face.x1, face.y1), face.eyes);
}
```

`src/wasm.rs` is the same as `extern "C"` functions (`cr_load`, `cr_score`, ...);
`web/src/lib/wasm/critic-worker.js` is the other side.

## Building

```
cargo build --release -p critic --target wasm32-unknown-unknown
```

or `tools/build.sh` from the repo root, which copies it into `web/src/lib/wasm/`.
`node tools/check-critic.mjs` checks the committed module and weights.
