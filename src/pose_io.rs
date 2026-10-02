//! Pose JSON files.
//!
//! ```json
//! {
//!   "format": "rust-pose-studio/pose",
//!   "version": 1,
//!   "name": "Seiza",
//!   "root": [0.0, 0.52, 0.0],
//!   "joints": { "pelvis": [0, 0, 0], "thigh_l": [-90, 0, 0], ... },
//!   "proportions": { "height": 1.7, ... },
//!   "camera": { "yaw": 30, ... }
//! }
//! ```
//! Unknown joints are ignored, missing joints default to zero rotation.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::math::Vec3;
use crate::render::Camera;
use crate::skeleton::{JOINT_COUNT, Joint, Pose, Proportions};

pub const FORMAT: &str = "rust-pose-studio/pose";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PoseFile {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    pub root: [f32; 3],
    pub joints: BTreeMap<String, [f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proportions: Option<Proportions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<Camera>,
}

fn default_format() -> String {
    FORMAT.to_string()
}
fn default_version() -> u32 {
    1
}

impl PoseFile {
    pub fn from_pose(name: &str, pose: &Pose, proportions: Option<Proportions>, camera: Option<Camera>) -> PoseFile {
        let round = |v: f32| (v * 1000.0).round() / 1000.0;
        PoseFile {
            format: FORMAT.to_string(),
            version: 1,
            name: name.to_string(),
            root: pose.root.to_array().map(round),
            joints: Joint::ALL.iter().map(|j| (j.key().to_string(), pose.get(*j).map(round))).collect(),
            proportions,
            camera,
        }
    }

    pub fn to_pose(&self) -> Result<Pose, String> {
        if self.format != FORMAT {
            return Err(format!("not a pose file (format \"{}\")", self.format));
        }
        let mut pose = Pose { root: Vec3::from_array(self.root), rot: [[0.0; 3]; JOINT_COUNT] };
        for (k, v) in &self.joints {
            if let Some(j) = Joint::from_key(k) {
                pose.set(j, *v);
            }
        }
        if !pose.is_finite() {
            return Err("pose contains invalid numbers".into());
        }
        Ok(pose)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Result<PoseFile, String> {
        serde_json::from_str(s).map_err(|e| format!("invalid pose JSON: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_json()).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    pub fn load(path: &Path) -> Result<PoseFile, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        PoseFile::from_json(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{BodyType, Skeleton};

    #[test]
    fn json_round_trip() {
        let skel = Skeleton::new(Proportions::default());
        let mut pose = Pose::rest(&skel);
        pose.set(Joint::UpperArmL, [-45.5, 12.25, 80.0]);
        pose.set(Joint::ShinR, [95.0, 0.0, 0.0]);
        pose.root = Vec3::from_array([0.1, 0.5, -0.2]);
        let props = Proportions { height: 1.6, body_type: BodyType::Curvy, ..Proportions::default() };
        let file = PoseFile::from_pose("Test", &pose, Some(props), Some(Camera::default()));
        let json = file.to_json();
        assert!(json.contains("\"upper_arm_l\""));
        let back = PoseFile::from_json(&json).unwrap();
        assert_eq!(back, file);
        assert_eq!(back.to_pose().unwrap(), pose);
        assert_eq!(back.proportions, Some(props));

        // Save/load through the file system.
        let path = std::env::temp_dir().join(format!("rps-test-{}.json", std::process::id()));
        file.save(&path).unwrap();
        let loaded = PoseFile::load(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded.to_pose().unwrap(), pose);
    }

    #[test]
    fn rejects_garbage() {
        assert!(PoseFile::from_json("{\"hello\": 1}").is_err());
        let f = PoseFile::from_json("{\"format\":\"other\",\"root\":[0,0,0],\"joints\":{}}").unwrap();
        assert!(f.to_pose().is_err());
        // Missing optional fields are fine; unknown joints are ignored.
        let f = PoseFile::from_json("{\"root\":[0,1,0],\"joints\":{\"head\":[10,0,0],\"tail\":[1,2,3]}}").unwrap();
        let p = f.to_pose().unwrap();
        assert_eq!(p.get(Joint::Head), [10.0, 0.0, 0.0]);
    }
}
