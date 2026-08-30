//! 3x3 matrices, for colour (in f32) and for the framing homography (in f64).

use std::ops::Index;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix3<T>(pub [[T; 3]; 3]);

/// Colour matrices.
pub type Mat3 = Matrix3<f32>;
/// The framing homography, which needs the extra precision at the corners.
pub type Mat3d = Matrix3<f64>;

impl<T> Index<usize> for Matrix3<T> {
    type Output = [T; 3];
    fn index(&self, row: usize) -> &[T; 3] {
        &self.0[row]
    }
}

macro_rules! matrix3 {
    ($t:ty) => {
        impl Matrix3<$t> {
            pub const IDENTITY: Self = Matrix3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
            pub const ZERO: Self = Matrix3([[0.0; 3]; 3]);

            /// `self * other`.
            pub fn mul(&self, other: &Self) -> Self {
                let (a, b) = (&self.0, &other.0);
                let mut o = [[0.0; 3]; 3];
                for i in 0..3 {
                    for j in 0..3 {
                        o[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
                    }
                }
                Matrix3(o)
            }

            /// The inverse, or `None` when the determinant's size is below
            /// `singular_below` (pass 0.0 for a matrix known to be invertible).
            pub fn inverse(&self, singular_below: $t) -> Option<Self> {
                let m = &self.0;
                let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                    - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                    + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
                if det.abs() < singular_below {
                    return None;
                }
                let id = 1.0 / det;
                Some(Matrix3([
                    [(m[1][1] * m[2][2] - m[1][2] * m[2][1]) * id,
                     (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * id,
                     (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * id],
                    [(m[1][2] * m[2][0] - m[1][0] * m[2][2]) * id,
                     (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * id,
                     (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * id],
                    [(m[1][0] * m[2][1] - m[1][1] * m[2][0]) * id,
                     (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * id,
                     (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * id],
                ]))
            }

            /// `self * v`.
            #[inline]
            pub fn apply(&self, v: [$t; 3]) -> [$t; 3] {
                let m = &self.0;
                [m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
                 m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
                 m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2]]
            }

            /// Each entry `(1 - t)` of the way from `self` and `t` of the way
            /// to `other`.
            pub fn lerp(&self, other: &Self, t: $t) -> Self {
                let mut o = [[0.0; 3]; 3];
                for i in 0..3 {
                    for j in 0..3 {
                        o[i][j] = self.0[i][j] * (1.0 - t) + other.0[i][j] * t;
                    }
                }
                Matrix3(o)
            }
        }
    };
}

matrix3!(f32);
matrix3!(f64);

impl Mat3 {
    /// Every pixel of an interleaved RGB buffer through this matrix.
    pub fn apply_to(&self, pixels: &mut [f32]) {
        for px in pixels.chunks_exact_mut(3) {
            let out = self.apply([px[0], px[1], px[2]]);
            px.copy_from_slice(&out);
        }
    }
}
