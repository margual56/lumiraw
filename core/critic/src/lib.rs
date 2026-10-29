//! NIMA (Talebi and Milanfar, 2018) on a MobileNet: how good a photograph
//! looks, as a distribution over the ten marks of a 1-to-10 scale.
//!
//! The weights are idealo's (https://github.com/idealo/image-quality-assessment),
//! one set trained on AVA's aesthetic votes and one on TID2013's technical
//! ones, baked by `tools/bake-nima.mjs`. Nothing here depends on which it is.

pub mod faces;
pub mod graph;
pub mod net;
#[cfg(feature = "module")]
mod wasm;

use net::{Kind, Layer, Map};

pub const SOFTWARE: &str = concat!("LumiRaw ", env!("CARGO_PKG_VERSION"));

/// The side the networks were trained at. Any picture is squashed to it,
/// as idealo's own code does, rather than cropped.
pub const SIDE: usize = 224;

/// What a network made of one photograph.
#[derive(Clone, Debug)]
pub struct Score {
    /// The expected mark, 1 to 10.
    pub mean: f32,
    /// How much the marks it predicts disagree.
    pub std: f32,
    /// The pooled features before the classifier: what the picture is of, near
    /// enough to tell two frames of a burst from two different shots.
    pub feature: Vec<f32>,
}

pub struct Net {
    layers: Vec<Layer>,
}

impl Net {
    /// A network from a baked weights file. The layers must chain: each one's
    /// inputs are the outputs before it, convolutions first and one classifier last.
    pub fn parse(bytes: &[u8]) -> Result<Net, String> {
        let layers = net::parse(bytes)?;
        let (last, convs) = layers.split_last().ok_or("no layers")?;
        if last.kind != Kind::Dense || convs.iter().any(|l| l.kind == Kind::Dense) {
            return Err("the classifier must be the last layer, and the only one".into());
        }
        if convs.first().map(|l| l.kind) != Some(Kind::Conv) || convs[0].inputs != 3 {
            return Err("the first layer must take the three colour channels".into());
        }
        for pair in layers.windows(2) {
            if pair[1].inputs != pair[0].outputs {
                return Err("the layers do not chain".into());
            }
        }
        Ok(Net { layers })
    }

    /// Score an RGBA picture, `width` by `height`, as it is: the caller sizes it.
    pub fn score(&self, rgba: &[u8], width: usize, height: usize) -> Score {
        let mut map = preprocess(rgba, width, height);
        let (classifier, convs) = self.layers.split_last().unwrap();
        for layer in convs {
            map = apply(layer, &map);
        }
        let feature = net::global_average(&map);
        let p = net::dense_softmax(classifier, &feature);
        let mean: f32 = p.iter().enumerate().map(|(i, p)| (i + 1) as f32 * p).sum();
        let var: f32 = p.iter().enumerate()
            .map(|(i, p)| ((i + 1) as f32 - mean).powi(2) * p).sum();
        Score { mean, std: var.sqrt(), feature }
    }
}

/// Keras's MobileNet preprocessing: 0..255 to -1..1, alpha dropped.
fn preprocess(rgba: &[u8], width: usize, height: usize) -> Map {
    let mut map = Map::new(width, height, 3);
    for (px, out) in rgba.chunks_exact(4).zip(map.d.chunks_exact_mut(3)) {
        for c in 0..3 {
            out[c] = px[c] as f32 / 127.5 - 1.0;
        }
    }
    map
}

fn apply(layer: &Layer, map: &Map) -> Map {
    match layer.kind {
        Kind::Conv => net::conv(layer, map),
        Kind::Depthwise => net::depthwise(layer, map),
        Kind::Pointwise => net::pointwise(layer, map),
        Kind::Dense => unreachable!("the classifier runs on the pooled features"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::f16_to_f32;

    fn layer(kind: Kind, stride: usize, inputs: usize, outputs: usize, w: f32, b: f32) -> Layer {
        let n = match kind {
            Kind::Conv => 9 * inputs * outputs,
            Kind::Depthwise => 9 * outputs,
            _ => inputs * outputs,
        };
        Layer { kind, stride, inputs, outputs, weights: vec![w; n], bias: vec![b; outputs] }
    }

    /// A weights file as the bake script writes it.
    fn bake(layers: &[Layer]) -> Vec<u8> {
        let mut out = b"NIMA".to_vec();
        out.extend(1u32.to_le_bytes());
        out.extend((layers.len() as u32).to_le_bytes());
        for l in layers {
            let kind = match l.kind {
                Kind::Conv => 0u8, Kind::Depthwise => 1, Kind::Pointwise => 2, Kind::Dense => 3,
            };
            out.extend([kind, l.stride as u8, 0, 0]);
            out.extend((l.inputs as u32).to_le_bytes());
            out.extend((l.outputs as u32).to_le_bytes());
            for &w in &l.weights {
                // Only values f16 holds exactly are used here.
                let bits = w.to_bits();
                let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
                let h = if w == 0.0 { 0 } else {
                    (((bits >> 16) & 0x8000) | ((exp as u32) << 10) | ((bits >> 13) & 0x3ff)) as u16
                };
                out.extend(h.to_le_bytes());
            }
            for &b in &l.bias {
                out.extend(b.to_le_bytes());
            }
        }
        out
    }

    #[test]
    fn halves() {
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0xc000), -2.0);
        assert_eq!(f16_to_f32(0x3555), 0.333_251_95);
        assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
        assert_eq!(f16_to_f32(0x03ff), 1023.0 * 2f32.powi(-24));
        assert_eq!(f16_to_f32(0x7bff), 65504.0);
        assert!(f16_to_f32(0x7c00).is_infinite());
    }

    /// A stride-2 window padded by one on every side: the corner sees four
    /// pixels of each channel, the edge six, the inside nine.
    #[test]
    fn padding() {
        let stem = layer(Kind::Conv, 2, 3, 1, 0.125, 0.0);
        let input = Map { w: 4, h: 4, c: 3, d: vec![1.0; 48] };
        let out = net::conv(&stem, &input);
        assert_eq!((out.w, out.h), (2, 2));
        assert_eq!(out.d, vec![1.5, 2.25, 2.25, 3.375]);

        let dw = layer(Kind::Depthwise, 1, 2, 2, 0.5, 0.25);
        let input = Map { w: 3, h: 3, c: 2, d: vec![1.0; 18] };
        let out = net::depthwise(&dw, &input);
        assert_eq!((out.w, out.h), (3, 3));
        assert_eq!(out.at(0, 0), [2.25, 2.25]);
        assert_eq!(out.at(1, 0), [3.25, 3.25]);
        assert_eq!(out.at(1, 1), [4.75, 4.75]);
    }

    /// The four-pixel block and the leftover pixels must agree.
    #[test]
    fn pointwise_blocks() {
        let mut pw = layer(Kind::Pointwise, 1, 3, 2, 0.0, 0.5);
        pw.weights = vec![1.0, -1.0, 0.5, 0.0, 0.25, 2.0];
        let input = Map { w: 5, h: 1, c: 3,
                          d: (0..15).map(|i| (i % 4) as f32 * 0.5).collect() };
        let out = net::pointwise(&pw, &input);
        for p in 0..5 {
            let x = &input.d[p * 3..p * 3 + 3];
            let want0 = (0.5 + x[0] + 0.5 * x[1] + 0.25 * x[2]).clamp(0.0, 6.0);
            let want1 = (0.5 - x[0] + 2.0 * x[2]).clamp(0.0, 6.0);
            assert_eq!(&out.d[p * 2..p * 2 + 2], &[want0, want1], "pixel {p}");
        }
    }

    /// A whole network, through the file format: a flat picture gives the
    /// same features everywhere, and a classifier that only favours the top
    /// mark scores near 10.
    #[test]
    fn whole() {
        let mut dense = layer(Kind::Dense, 1, 2, 10, 0.0, 0.0);
        dense.bias[9] = 12.0;
        let layers = vec![
            layer(Kind::Conv, 2, 3, 2, 0.0, 1.0),
            layer(Kind::Depthwise, 1, 2, 2, 0.0, 2.0),
            layer(Kind::Pointwise, 1, 2, 2, 0.0, 3.0),
            dense,
        ];
        let net = Net::parse(&bake(&layers)).unwrap();
        let score = net.score(&vec![128; 8 * 8 * 4], 8, 8);
        assert_eq!(score.feature, vec![3.0, 3.0]);
        assert!(score.mean > 9.9 && score.mean <= 10.0, "{}", score.mean);
        assert!(score.std < 0.1);
    }

    #[test]
    fn refuses_broken_files() {
        assert!(Net::parse(b"NOPE").is_err());
        let layers = vec![layer(Kind::Conv, 2, 3, 2, 0.0, 0.0),
                          layer(Kind::Pointwise, 1, 3, 2, 0.0, 0.0),
                          layer(Kind::Dense, 1, 2, 10, 0.0, 0.0)];
        assert!(Net::parse(&bake(&layers)).is_err());
        let mut bytes = bake(&layers[..1]);
        bytes.pop();
        assert!(Net::parse(&bytes).is_err());
    }
}
