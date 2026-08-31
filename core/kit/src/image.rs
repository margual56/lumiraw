//! The two buffers everything works on.

#[derive(Clone, Debug)]
pub struct Plane {
    pub w: usize,
    pub h: usize,
    pub d: Vec<f32>,
}

impl Plane {
    pub fn new(w: usize, h: usize) -> Self {
        Plane { w, h, d: vec![0.0; w * h] }
    }
    pub fn filled(w: usize, h: usize, v: f32) -> Self {
        Plane { w, h, d: vec![v; w * h] }
    }
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.d[y * self.w + x]
    }
    pub fn len(&self) -> usize {
        self.d.len()
    }
    /// Sub-rectangle copy (used by `stats_view` and centre crops).
    pub fn crop(&self, x0: usize, y0: usize, w: usize, h: usize) -> Plane {
        let mut out = Plane::new(w, h);
        for y in 0..h {
            let src = (y0 + y) * self.w + x0;
            out.d[y * w..(y + 1) * w].copy_from_slice(&self.d[src..src + w]);
        }
        out
    }
}

/// Interleaved RGB, linear light.
#[derive(Clone, Debug)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub d: Vec<f32>, // len = w*h*3
}

impl Image {
    pub fn new(w: usize, h: usize) -> Self {
        Image { w, h, d: vec![0.0; w * h * 3] }
    }
    /// Every pixel the same colour.
    pub fn filled(w: usize, h: usize, px: [f32; 3]) -> Self {
        let mut img = Image::new(w, h);
        img.px_mut().fill(px);
        img
    }
    /// The pixels as RGB triples, in row order.
    #[inline]
    pub fn px(&self) -> &[[f32; 3]] {
        self.d.as_chunks::<3>().0
    }
    #[inline]
    pub fn px_mut(&mut self) -> &mut [[f32; 3]] {
        self.d.as_chunks_mut::<3>().0
    }
    pub fn plane(&self, c: usize) -> Plane {
        let mut p = Plane::new(self.w, self.h);
        for (v, px) in p.d.iter_mut().zip(self.px()) {
            *v = px[c];
        }
        p
    }
    pub fn set_plane(&mut self, c: usize, p: &Plane) {
        for (px, v) in self.px_mut().iter_mut().zip(&p.d) {
            px[c] = *v;
        }
    }
    pub fn from_planes(r: &Plane, g: &Plane, b: &Plane) -> Image {
        let mut img = Image::new(r.w, r.h);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            *px = [r.d[i], g.d[i], b.d[i]];
        }
        img
    }
    pub fn crop(&self, x0: usize, y0: usize, w: usize, h: usize) -> Image {
        let mut out = Image::new(w, h);
        for y in 0..h {
            let src = ((y0 + y) * self.w + x0) * 3;
            out.d[y * w * 3..(y + 1) * w * 3].copy_from_slice(&self.d[src..src + w * 3]);
        }
        out
    }
    pub fn scale_channels(&mut self, g: [f32; 3]) {
        for px in self.px_mut() {
            px[0] *= g[0];
            px[1] *= g[1];
            px[2] *= g[2];
        }
    }
    pub fn scale(&mut self, g: f32) {
        for v in self.d.iter_mut() {
            *v *= g;
        }
    }
}
