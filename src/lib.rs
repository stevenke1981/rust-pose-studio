//! Rust Pose Studio: a posable 3D drawing mannequin for figure-drawing reference.
//!
//! * [`skeleton`] – joint hierarchy, proportions, poses, forward kinematics
//! * [`body`] – capsule / ellipsoid body volumes and floor snapping
//! * [`ik`], [`posing`] – two-bone IK and pose editing operations
//! * [`presets`] – the 25 built-in poses
//! * [`render`] – camera, projection and the backend-independent drawing list
//! * [`raster`], [`export`] – tiny-skia rasteriser, PNG and contact-sheet export
//! * [`pose_io`] – pose JSON files
//! * `app` – the egui front-end (binary only)

pub mod body;
pub mod cli;
pub mod export;
pub mod ik;
pub mod lineart;
pub mod math;
pub mod pose_io;
pub mod posing;
pub mod presets;
pub mod raster;
pub mod render;
pub mod skeleton;
