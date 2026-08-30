//! The general-purpose half of the pipeline.

pub mod color;
pub mod filter;
pub mod image;
pub mod math;
pub mod matrix;
pub mod report;

pub use color::*;
pub use filter::*;
pub use image::{Image, Plane};
pub use math::*;
pub use matrix::{Mat3, Mat3d, Matrix3};
pub use report::{round_to, with, Report};

#[cfg(test)]
mod tests;
