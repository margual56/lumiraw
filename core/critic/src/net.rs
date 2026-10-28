//! MobileNet v1 as NIMA uses it: the weights as `tools/bake-nima.mjs` writes
//! them, and the four kinds of layer it is made of.
//!
//! Activations are kept channels-last, the same order as Keras's kernels, so
//! every inner loop runs along contiguous channels and vectorises.

/// A feature map, `h` rows of `w` pixels of `c` channels.
#[derive(Clone, Debug)]
pub struct Map {
    pub w: usize,
    pub h: usize,
    pub c: usize,
    pub d: Vec<f32>,
}

impl Map {
    pub fn new(w: usize, h: usize, c: usize) -> Map {
        Map { w, h, c, d: vec![0.0; w * h * c] }
    }

    pub fn at(&self, x: usize, y: usize) -> &[f32] {
        let i = (y * self.w + x) * self.c;
        &self.d[i..i + self.c]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// 3x3 over every input channel: the stem.
    Conv,
    /// 3x3 over each channel on its own.
    Depthwise,
    /// 1x1 across channels.
    Pointwise,
    /// The classifier after the pooling.
    Dense,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub kind: Kind,
    pub stride: usize,
    pub inputs: usize,
    pub outputs: usize,
    pub weights: Vec<f32>,
    pub bias: Vec<f32>,
}

impl Layer {
    fn weight_count(kind: Kind, inputs: usize, outputs: usize) -> usize {
        match kind {
            Kind::Conv => 9 * inputs * outputs,
            Kind::Depthwise => 9 * outputs,
            Kind::Pointwise | Kind::Dense => inputs * outputs,
        }
    }
}

const MAGIC: &[u8; 4] = b"NIMA";
const VERSION: u32 = 1;

/// IEEE half to single, subnormals included.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = match exp {
        0 if frac == 0 => sign,
        0 => {
            // Subnormal: shift the fraction up until its leading one is implicit.
            let mut e = 113u32;
            let mut f = frac;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            sign | (e << 23) | ((f & 0x3ff) << 13)
        }
        0x1f => sign | 0x7f80_0000 | (frac << 13),
        _ => sign | ((exp + 112) << 23) | (frac << 13),
    };
    f32::from_bits(bits)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len())
            .ok_or("the weights file is cut short")?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

/// Read a weights file.
pub fn parse(bytes: &[u8]) -> Result<Vec<Layer>, String> {
    let mut r = Reader { bytes, at: 0 };
    if r.take(4)? != MAGIC {
        return Err("not a NIMA weights file".into());
    }
    let version = r.u32()?;
    if version != VERSION {
        return Err(format!("weights file version {version}, expected {VERSION}"));
    }
    let count = r.u32()? as usize;
    let mut layers = Vec::with_capacity(count);
    for _ in 0..count {
        let head = r.take(4)?;
        let kind = match head[0] {
            0 => Kind::Conv,
            1 => Kind::Depthwise,
            2 => Kind::Pointwise,
            3 => Kind::Dense,
            k => return Err(format!("unknown layer kind {k}")),
        };
        let stride = head[1].max(1) as usize;
        let inputs = r.u32()? as usize;
        let outputs = r.u32()? as usize;
        let n = Layer::weight_count(kind, inputs, outputs);
        let weights = r.take(n * 2)?.chunks_exact(2)
            .map(|b| f16_to_f32(u16::from_le_bytes([b[0], b[1]])))
            .collect();
        let bias = r.take(outputs * 4)?.chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        layers.push(Layer { kind, stride, inputs, outputs, weights, bias });
    }
    if r.at != bytes.len() {
        return Err("trailing bytes after the last layer".into());
    }
    Ok(layers)
}

#[inline]
fn relu6(v: &mut [f32]) {
    for x in v {
        *x = x.clamp(0.0, 6.0);
    }
}

#[inline]
fn axpy(acc: &mut [f32], a: f32, w: &[f32]) {
    for (o, w) in acc.iter_mut().zip(w) {
        *o += a * w;
    }
}

/// The output size of a 3x3 window padded by one on every side, which is how
/// the Keras 2.1 MobileNet the weights were trained in pads all of them.
fn out_len(len: usize, stride: usize) -> usize {
    (len + 2 - 3) / stride + 1
}

/// The input rows (or columns) a window centred on output `o` covers, as
/// (tap, input) pairs that fall inside the picture.
fn taps(o: usize, stride: usize, len: usize) -> impl Iterator<Item = (usize, usize)> {
    (0..3).filter_map(move |k| {
        let i = (o * stride + k).checked_sub(1)?;
        (i < len).then_some((k, i))
    })
}

/// The 3x3 stem, every input channel into every output.
pub fn conv(layer: &Layer, input: &Map) -> Map {
    let (s, co) = (layer.stride, layer.outputs);
    let mut out = Map::new(out_len(input.w, s), out_len(input.h, s), co);
    for oy in 0..out.h {
        for ox in 0..out.w {
            let i = (oy * out.w + ox) * co;
            let acc = &mut out.d[i..i + co];
            acc.copy_from_slice(&layer.bias);
            for (ky, iy) in taps(oy, s, input.h) {
                for (kx, ix) in taps(ox, s, input.w) {
                    for (ci, &a) in input.at(ix, iy).iter().enumerate() {
                        let k = ((ky * 3 + kx) * layer.inputs + ci) * co;
                        axpy(acc, a, &layer.weights[k..k + co]);
                    }
                }
            }
            relu6(acc);
        }
    }
    out
}

/// A 3x3 window over each channel on its own.
pub fn depthwise(layer: &Layer, input: &Map) -> Map {
    let (s, c) = (layer.stride, input.c);
    let mut out = Map::new(out_len(input.w, s), out_len(input.h, s), c);
    let mut tmp = vec![0.0f32; c];
    for oy in 0..out.h {
        for ox in 0..out.w {
            let i = (oy * out.w + ox) * c;
            let acc = &mut out.d[i..i + c];
            acc.copy_from_slice(&layer.bias);
            for (ky, iy) in taps(oy, s, input.h) {
                for (kx, ix) in taps(ox, s, input.w) {
                    let k = (ky * 3 + kx) * c;
                    let w = &layer.weights[k..k + c];
                    for ((t, a), w) in tmp.iter_mut().zip(input.at(ix, iy)).zip(w) {
                        *t = a * w;
                    }
                    for (o, t) in acc.iter_mut().zip(&tmp) {
                        *o += t;
                    }
                }
            }
            relu6(acc);
        }
    }
    out
}

/// 1x1 across channels, which is where nearly all the arithmetic is. Four
/// pixels share each row of weights while it is in cache.
pub fn pointwise(layer: &Layer, input: &Map) -> Map {
    let (ci, co) = (layer.inputs, layer.outputs);
    let pixels = input.w * input.h;
    let mut out = Map::new(input.w, input.h, co);
    for acc in out.d.chunks_mut(co) {
        acc.copy_from_slice(&layer.bias);
    }
    const BLOCK: usize = 4;
    let mut p = 0;
    while p < pixels {
        let n = BLOCK.min(pixels - p);
        let rows = &mut out.d[p * co..(p + n) * co];
        let ins = &input.d[p * ci..(p + n) * ci];
        if n == BLOCK {
            let (r0, rest) = rows.split_at_mut(co);
            let (r1, rest) = rest.split_at_mut(co);
            let (r2, r3) = rest.split_at_mut(co);
            for k in 0..ci {
                let w = &layer.weights[k * co..(k + 1) * co];
                let (a0, a1, a2, a3) = (ins[k], ins[ci + k], ins[2 * ci + k], ins[3 * ci + k]);
                for ((((o0, o1), o2), o3), &w) in
                    r0.iter_mut().zip(r1.iter_mut()).zip(r2.iter_mut()).zip(r3.iter_mut()).zip(w)
                {
                    *o0 += a0 * w;
                    *o1 += a1 * w;
                    *o2 += a2 * w;
                    *o3 += a3 * w;
                }
            }
        } else {
            for (row, inp) in rows.chunks_mut(co).zip(ins.chunks(ci)) {
                for (k, &a) in inp.iter().enumerate() {
                    axpy(row, a, &layer.weights[k * co..(k + 1) * co]);
                }
            }
        }
        p += n;
    }
    relu6(&mut out.d);
    out
}

/// The mean of every channel over the whole map.
pub fn global_average(input: &Map) -> Vec<f32> {
    let mut out = vec![0.0f32; input.c];
    for px in input.d.chunks(input.c) {
        axpy(&mut out, 1.0, px);
    }
    let n = (input.w * input.h).max(1) as f32;
    out.iter_mut().for_each(|v| *v /= n);
    out
}

/// The classifier, as probabilities.
pub fn dense_softmax(layer: &Layer, feature: &[f32]) -> Vec<f32> {
    let co = layer.outputs;
    let mut logits = layer.bias.clone();
    for (k, &a) in feature.iter().enumerate() {
        axpy(&mut logits, a, &layer.weights[k * co..(k + 1) * co]);
    }
    let top = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    logits.iter_mut().for_each(|v| *v = (*v - top).exp());
    let sum: f32 = logits.iter().sum();
    logits.iter_mut().for_each(|v| *v /= sum);
    logits
}
