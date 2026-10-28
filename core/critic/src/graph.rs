//! A small network as a graph of operators, read from the format
//! `tools/bake-faces.mjs` writes: just the operators MediaPipe's face models
//! are made of, with TensorFlow Lite's meaning for each.
//!
//! Tensors are batch-of-one NHWC, held without the batch, so every inner loop
//! runs along contiguous channels and vectorises.

use crate::net::{f16_to_f32, Reader};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Conv,
    Depthwise,
    Add,
    Relu,
    Prelu,
    Pad,
    MaxPool,
    Reshape,
    Concat,
    Resize,
    Logistic,
}

#[derive(Clone, Debug)]
struct Op {
    kind: Kind,
    inputs: Vec<usize>,
    outputs: Vec<usize>,
    params: Vec<i32>,
}

#[derive(Clone, Debug)]
struct Tensor {
    shape: Vec<usize>,
    /// The constant's values, laid out for the operator that reads it.
    data: Option<Vec<f32>>,
}

impl Tensor {
    /// Height, width and channels, the batch of one dropped.
    fn hwc(&self) -> (usize, usize, usize) {
        match self.shape.as_slice() {
            [_, h, w, c] => (*h, *w, *c),
            [_, n, c] => (1, *n, *c),
            [n, c] => (1, *n, *c),
            [c] => (1, 1, *c),
            _ => (1, 1, self.shape.iter().product()),
        }
    }
}

pub struct Graph {
    tensors: Vec<Tensor>,
    ops: Vec<Op>,
    input: usize,
    outputs: Vec<usize>,
    /// After which operator each tensor is no longer read, so it can go.
    last_use: Vec<usize>,
}

impl Graph {
    pub fn parse(bytes: &[u8]) -> Result<Graph, String> {
        let mut r = Reader::new(bytes);
        if r.take(4)? != b"TFG1" || r.u32()? != 1 {
            return Err("not a graph file this version reads".into());
        }
        let count = r.u32()? as usize;
        let mut tensors = Vec::with_capacity(count);
        for _ in 0..count {
            let rank = r.u8()? as usize;
            let shape = (0..rank).map(|_| r.i32().map(|v| v.max(1) as usize))
                .collect::<Result<Vec<_>, _>>()?;
            let data = if r.u8()? == 1 {
                let n = r.u32()? as usize;
                let raw = r.take(n * 2)?;
                Some(raw.chunks_exact(2).map(|b| f16_to_f32(u16::from_le_bytes([b[0], b[1]])))
                    .collect::<Vec<f32>>())
            } else {
                None
            };
            tensors.push(Tensor { shape, data });
        }
        let count = r.u32()? as usize;
        let mut ops = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = match r.u8()? {
                0 => Kind::Conv,
                1 => Kind::Depthwise,
                2 => Kind::Add,
                3 => Kind::Relu,
                4 => Kind::Prelu,
                5 => Kind::Pad,
                6 => Kind::MaxPool,
                7 => Kind::Reshape,
                8 => Kind::Concat,
                9 => Kind::Resize,
                10 => Kind::Logistic,
                k => return Err(format!("unknown operator {k}")),
            };
            let (ni, no, np) = (r.u8()? as usize, r.u8()? as usize, r.u8()? as usize);
            let mut ids = |n: usize| -> Result<Vec<usize>, String> {
                (0..n).map(|_| r.u32().map(|v| v as usize)).collect()
            };
            let inputs = ids(ni)?;
            let outputs = ids(no)?;
            let params = (0..np).map(|_| r.i32()).collect::<Result<Vec<_>, _>>()?;
            if inputs.iter().chain(&outputs).any(|t| *t >= tensors.len()) {
                return Err("an operator names a tensor that is not there".into());
            }
            ops.push(Op { kind, inputs, outputs, params });
        }
        let input = r.u32()? as usize;
        let n = r.u32()? as usize;
        let outputs = (0..n).map(|_| r.u32().map(|v| v as usize)).collect::<Result<Vec<_>, _>>()?;
        if input >= tensors.len() || outputs.iter().any(|t| *t >= tensors.len()) {
            return Err("the graph's ends are not among its tensors".into());
        }

        // Convolution filters come as [out, kh, kw, in]; the kernel wants
        // [kh, kw, in, out] so its innermost loop is over outputs.
        for op in &ops {
            if op.kind == Kind::Conv {
                let t = &mut tensors[op.inputs[1]];
                if let ([o, kh, kw, i], Some(d)) = (t.shape.as_slice(), t.data.as_mut()) {
                    let (o, kh, kw, i) = (*o, *kh, *kw, *i);
                    let mut out = vec![0.0; d.len()];
                    for oo in 0..o {
                        for k in 0..kh * kw {
                            for ii in 0..i {
                                out[(k * i + ii) * o + oo] = d[(oo * kh * kw + k) * i + ii];
                            }
                        }
                    }
                    *d = out;
                } else {
                    return Err("a convolution without a constant 4-d filter".into());
                }
            }
        }

        let mut last_use = vec![0usize; tensors.len()];
        for (k, op) in ops.iter().enumerate() {
            for t in &op.inputs {
                last_use[*t] = k;
            }
        }
        for t in &outputs {
            last_use[*t] = usize::MAX;
        }
        Ok(Graph { tensors, ops, input, outputs, last_use })
    }

    /// The input's height, width and channels.
    pub fn input_size(&self) -> (usize, usize, usize) {
        self.tensors[self.input].hwc()
    }

    /// Run the graph on one input, laid out HWC, and return each output.
    pub fn run(&self, input: Vec<f32>) -> Vec<Vec<f32>> {
        let mut values: Vec<Option<Vec<f32>>> = vec![None; self.tensors.len()];
        values[self.input] = Some(input);
        for (k, op) in self.ops.iter().enumerate() {
            let out = self.apply(op, &values);
            values[op.outputs[0]] = Some(out);
            for t in &op.inputs {
                if self.last_use[*t] == k {
                    values[*t] = None;
                }
            }
        }
        self.outputs.iter().map(|t| values[*t].take().unwrap_or_default()).collect()
    }

    fn value<'v>(&'v self, values: &'v [Option<Vec<f32>>], t: usize) -> &'v [f32] {
        values[t].as_deref().or(self.tensors[t].data.as_deref()).unwrap_or(&[])
    }

    fn apply(&self, op: &Op, values: &[Option<Vec<f32>>]) -> Vec<f32> {
        let x = self.value(values, op.inputs[0]);
        let input = self.tensors[op.inputs[0]].hwc();
        let output = self.tensors[op.outputs[0]].hwc();
        let p = &op.params;
        match op.kind {
            Kind::Conv => {
                let f = &self.tensors[op.inputs[1]];
                let (kh, kw) = (f.shape[1], f.shape[2]);
                let bias = op.inputs.get(2).map(|t| self.value(values, *t));
                let mut out = conv(x, input, f.data.as_deref().unwrap(), (kh, kw), bias, output,
                                   (p[1] as usize, p[0] as usize), p[2] == 1);
                activate(&mut out, p[3]);
                out
            }
            Kind::Depthwise => {
                let f = &self.tensors[op.inputs[1]];
                let (kh, kw) = (f.shape[1], f.shape[2]);
                let bias = op.inputs.get(2).map(|t| self.value(values, *t));
                let mut out = depthwise(x, input, f.data.as_deref().unwrap(), (kh, kw), bias,
                                        output, (p[1] as usize, p[0] as usize), p[2] == 1);
                activate(&mut out, p[3]);
                out
            }
            Kind::Add => {
                let y = self.value(values, op.inputs[1]);
                let mut out: Vec<f32> = x.iter().zip(y).map(|(a, b)| a + b).collect();
                activate(&mut out, p[0]);
                out
            }
            Kind::Relu => x.iter().map(|v| v.max(0.0)).collect(),
            Kind::Prelu => {
                let alpha = self.value(values, op.inputs[1]);
                let c = alpha.len().max(1);
                x.iter().enumerate()
                    .map(|(i, v)| if *v >= 0.0 { *v } else { *v * alpha[i % c] })
                    .collect()
            }
            Kind::Pad => pad(x, input, output, p),
            Kind::MaxPool => max_pool(x, input, output, (p[1] as usize, p[0] as usize),
                                      (p[3] as usize, p[2] as usize), p[4] == 1),
            Kind::Reshape => x.to_vec(),
            Kind::Concat => {
                // Joined along an axis with nothing but ones before it, which
                // is all the models ask: the second simply follows the first.
                let mut out = x.to_vec();
                for t in &op.inputs[1..] {
                    out.extend_from_slice(self.value(values, *t));
                }
                out
            }
            Kind::Resize => resize(x, input, output, p[0] == 1, p[1] == 1),
            Kind::Logistic => x.iter().map(|v| 1.0 / (1.0 + (-v).exp())).collect(),
        }
    }
}

fn activate(v: &mut [f32], code: i32) {
    match code {
        1 => v.iter_mut().for_each(|x| *x = x.max(0.0)),
        3 => v.iter_mut().for_each(|x| *x = x.clamp(0.0, 6.0)),
        _ => {}
    }
}

/// TensorFlow's padding: "same" keeps `ceil(size / stride)` outputs and puts
/// the odd pixel of padding at the end; "valid" pads nothing.
fn padding(size: usize, out: usize, k: usize, stride: usize, same: bool) -> usize {
    if !same {
        return 0;
    }
    ((out - 1) * stride + k).saturating_sub(size) / 2
}

#[allow(clippy::too_many_arguments)]
fn conv(x: &[f32], (h, w, ci): (usize, usize, usize), f: &[f32], (kh, kw): (usize, usize),
        bias: Option<&[f32]>, (oh, ow, co): (usize, usize, usize), (sy, sx): (usize, usize),
        same: bool) -> Vec<f32> {
    let (py, px) = (padding(h, oh, kh, sy, same), padding(w, ow, kw, sx, same));
    let mut out = vec![0.0f32; oh * ow * co];
    for oy in 0..oh {
        for ox in 0..ow {
            let acc = &mut out[(oy * ow + ox) * co..(oy * ow + ox + 1) * co];
            if let Some(b) = bias {
                acc.copy_from_slice(&b[..co]);
            }
            for ky in 0..kh {
                let iy = (oy * sy + ky) as isize - py as isize;
                if iy < 0 || iy >= h as isize {
                    continue;
                }
                for kx in 0..kw {
                    let ix = (ox * sx + kx) as isize - px as isize;
                    if ix < 0 || ix >= w as isize {
                        continue;
                    }
                    let src = &x[(iy as usize * w + ix as usize) * ci..][..ci];
                    let taps = &f[(ky * kw + kx) * ci * co..][..ci * co];
                    for (v, row) in src.iter().zip(taps.chunks_exact(co)) {
                        if *v != 0.0 {
                            for (a, t) in acc.iter_mut().zip(row) {
                                *a += v * t;
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn depthwise(x: &[f32], (h, w, c): (usize, usize, usize), f: &[f32], (kh, kw): (usize, usize),
             bias: Option<&[f32]>, (oh, ow, _): (usize, usize, usize), (sy, sx): (usize, usize),
             same: bool) -> Vec<f32> {
    let (py, px) = (padding(h, oh, kh, sy, same), padding(w, ow, kw, sx, same));
    let mut out = vec![0.0f32; oh * ow * c];
    for oy in 0..oh {
        for ox in 0..ow {
            let acc = &mut out[(oy * ow + ox) * c..(oy * ow + ox + 1) * c];
            if let Some(b) = bias {
                acc.copy_from_slice(&b[..c]);
            }
            for ky in 0..kh {
                let iy = (oy * sy + ky) as isize - py as isize;
                if iy < 0 || iy >= h as isize {
                    continue;
                }
                for kx in 0..kw {
                    let ix = (ox * sx + kx) as isize - px as isize;
                    if ix < 0 || ix >= w as isize {
                        continue;
                    }
                    let src = &x[(iy as usize * w + ix as usize) * c..][..c];
                    let taps = &f[(ky * kw + kx) * c..][..c];
                    for ((a, v), t) in acc.iter_mut().zip(src).zip(taps) {
                        *a += v * t;
                    }
                }
            }
        }
    }
    out
}

/// Zeros around a tensor: `p` is before and after for each of the four NHWC axes.
fn pad(x: &[f32], (h, w, c): (usize, usize, usize), (oh, ow, oc): (usize, usize, usize),
       p: &[i32]) -> Vec<f32> {
    let (top, left, front) = (p[2] as usize, p[4] as usize, p[6] as usize);
    let mut out = vec![0.0f32; oh * ow * oc];
    for y in 0..h {
        for xx in 0..w {
            let src = &x[(y * w + xx) * c..][..c];
            let dst = ((y + top) * ow + xx + left) * oc + front;
            out[dst..dst + c].copy_from_slice(src);
        }
    }
    out
}

fn max_pool(x: &[f32], (h, w, c): (usize, usize, usize), (oh, ow, _): (usize, usize, usize),
            (kh, kw): (usize, usize), (sy, sx): (usize, usize), same: bool) -> Vec<f32> {
    let (py, px) = (padding(h, oh, kh, sy, same), padding(w, ow, kw, sx, same));
    let mut out = vec![f32::MIN; oh * ow * c];
    for oy in 0..oh {
        for ox in 0..ow {
            let acc = &mut out[(oy * ow + ox) * c..(oy * ow + ox + 1) * c];
            for ky in 0..kh {
                let iy = (oy * sy + ky) as isize - py as isize;
                if iy < 0 || iy >= h as isize {
                    continue;
                }
                for kx in 0..kw {
                    let ix = (ox * sx + kx) as isize - px as isize;
                    if ix < 0 || ix >= w as isize {
                        continue;
                    }
                    let src = &x[(iy as usize * w + ix as usize) * c..][..c];
                    for (a, v) in acc.iter_mut().zip(src) {
                        *a = a.max(*v);
                    }
                }
            }
        }
    }
    out
}

/// TensorFlow's bilinear resize, with its two ways of placing pixels.
fn resize(x: &[f32], (h, w, c): (usize, usize, usize), (oh, ow, _): (usize, usize, usize),
          align_corners: bool, half_pixel: bool) -> Vec<f32> {
    let scale = |i: usize, o: usize| if align_corners && o > 1 {
        (i - 1) as f32 / (o - 1) as f32
    } else {
        i as f32 / o as f32
    };
    let (fy, fx) = (scale(h, oh), scale(w, ow));
    let source = |d: usize, f: f32, size: usize| {
        let s = if half_pixel { (d as f32 + 0.5) * f - 0.5 } else { d as f32 * f };
        let lo = s.floor().max(0.0) as usize;
        let hi = (s.ceil() as usize).min(size - 1);
        (lo.min(size - 1), hi, s - s.floor())
    };
    let mut out = vec![0.0f32; oh * ow * c];
    for oy in 0..oh {
        let (y0, y1, ty) = source(oy, fy, h);
        for ox in 0..ow {
            let (x0, x1, tx) = source(ox, fx, w);
            let at = |y: usize, xx: usize| &x[(y * w + xx) * c..][..c];
            let (a, b, cc, d) = (at(y0, x0), at(y0, x1), at(y1, x0), at(y1, x1));
            let dst = &mut out[(oy * ow + ox) * c..][..c];
            for k in 0..c {
                let top = a[k] + (b[k] - a[k]) * tx;
                let bottom = cc[k] + (d[k] - cc[k]) * tx;
                dst[k] = top + (bottom - top) * ty;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_padding_puts_the_odd_pixel_last() {
        // 4 wide, a 3-tap filter at stride 2: two outputs, one pixel of padding
        // in all, at the end.
        assert_eq!(padding(4, 2, 3, 2, true), 0);
        // 5 wide, stride 1: one each side.
        assert_eq!(padding(5, 5, 3, 1, true), 1);
        assert_eq!(padding(5, 3, 3, 1, false), 0);
    }

    #[test]
    fn a_convolution_reads_its_neighbours() {
        // One channel in, two out: the first sums the 3x3, the second takes the centre.
        let x: Vec<f32> = (0..9).map(|v| v as f32).collect();
        let mut f = vec![0.0f32; 9 * 2];
        for k in 0..9 {
            f[k * 2] = 1.0;
        }
        f[4 * 2 + 1] = 1.0;
        let out = conv(&x, (3, 3, 1), &f, (3, 3), Some(&[0.5, 0.0]), (3, 3, 2), (1, 1), true);
        assert_eq!(out[(4) * 2], 36.5, "the centre sees everything");
        assert_eq!(out[0], 0.0 + 1.0 + 3.0 + 4.0 + 0.5, "the corner sees four");
        assert_eq!(out[4 * 2 + 1], 4.0);
    }

    #[test]
    fn resize_with_half_pixels_doubles_smoothly() {
        let x = vec![0.0f32, 1.0];
        let out = resize(&x, (1, 2, 1), (1, 4, 1), false, true);
        assert_eq!(out, vec![0.0, 0.25, 0.75, 1.0]);
    }

    #[test]
    fn channels_are_padded_after() {
        let x = vec![1.0f32, 2.0];
        let out = pad(&x, (1, 1, 2), (1, 1, 4), &[0, 0, 0, 0, 0, 0, 0, 2]);
        assert_eq!(out, vec![1.0, 2.0, 0.0, 0.0]);
    }
}
