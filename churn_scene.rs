//! Shared, build-time-serializable definition of the churn benchmark scene.

pub const SECTION_NAME: &str = "scene/churn-v1.bin";
pub const MAGIC: [u8; 8] = *b"HCHURN\0\0";
pub const VERSION: u16 = 1;
pub const ENCODED_LEN: usize = 320;

pub const MAX_DYNAMIC_OBJECTS: usize = 2_200;
pub const START_SPAWN_RATE: usize = 8;
pub const MIN_SPAWN_RATE: usize = 1;
pub const MAX_SPAWN_RATE: usize = 64;
pub const SPAWN_INTERVAL_FRAMES: u32 = 2;
pub const RNG_SEED: u64 = 0x59A0_D3E4_B2CA_1897;

pub const CAMERA_POSITION: [f32; 3] = [0.0, 7.0, 25.0];
pub const CAMERA_YAW: f32 = 0.0;
pub const CAMERA_PITCH: f32 = -0.26;
pub const CAMERA_FOV: f32 = std::f32::consts::FRAC_PI_4;
pub const CAMERA_NEAR: f32 = 0.1;
pub const CAMERA_FAR: f32 = 300.0;
pub const CLEAR_RGBA: [f32; 4] = [0.01, 0.01, 0.02, 1.0];
// Brighter than the original 0.04 after the physical/local readability check.
pub const AMBIENT_RGB: [f32; 3] = [0.12, 0.12, 0.14];
pub const FLOOR_EXTENT: f32 = 40.0;
pub const FLOOR_RGBA: [f32; 4] = [0.25, 0.25, 0.30, 1.0];

#[derive(Clone, Copy)]
pub struct MaterialSpec {
    pub rgba: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
}

pub const MATERIALS: [MaterialSpec; 4] = [
    MaterialSpec {
        rgba: [0.85, 0.12, 0.12, 1.0],
        roughness: 0.65,
        metallic: 0.0,
    },
    MaterialSpec {
        rgba: [0.17, 0.82, 0.28, 1.0],
        roughness: 0.60,
        metallic: 0.0,
    },
    MaterialSpec {
        rgba: [0.16, 0.40, 0.90, 1.0],
        roughness: 0.70,
        metallic: 0.0,
    },
    MaterialSpec {
        rgba: [0.70, 0.70, 0.75, 1.0],
        roughness: 0.15,
        metallic: 0.80,
    },
];

pub const SHAPE_HALF_EXTENTS: [[f32; 3]; 3] =
    [[0.35, 0.35, 0.35], [0.15, 0.65, 0.15], [0.60, 0.20, 0.20]];

pub const TIME_STEP: f32 = 0.01;
pub const ORBIT_RADIUS: f32 = 8.0;
pub const ORBIT_RADIUS_AMPLITUDE: f32 = 2.0;
pub const HEIGHT_BASE: f32 = 0.5;
pub const HEIGHT_AMPLITUDE: f32 = 0.8;
pub const RADIUS_PHASE_SCALE: f32 = 0.25;
pub const HEIGHT_PHASE_SCALE: f32 = 1.3;
pub const ROTATION_SCALE: f32 = 1.37;
pub const SPAWN_RADIUS_RANGE: [f32; 2] = [5.0, 20.0];
pub const SPAWN_HEIGHT_RANGE: [f32; 2] = [0.35, 2.1];
pub const SCALE_RANGE: [f32; 2] = [0.25, 1.0];
pub const SPEED_RANGE: [f32; 2] = [0.4, 1.6];

pub fn encode() -> Vec<u8> {
    let mut out = vec![0; ENCODED_LEN];
    out[..8].copy_from_slice(&MAGIC);
    put_u16(&mut out, 8, VERSION);
    put_u16(&mut out, 10, ENCODED_LEN as u16);
    put_u32(&mut out, 12, ENCODED_LEN as u32);
    put_u32(&mut out, 16, MAX_DYNAMIC_OBJECTS as u32);
    put_u32(&mut out, 20, START_SPAWN_RATE as u32);
    put_u32(&mut out, 24, SPAWN_INTERVAL_FRAMES);
    put_u32(&mut out, 28, MATERIALS.len() as u32);
    put_u32(&mut out, 32, SHAPE_HALF_EXTENTS.len() as u32);
    out[40..48].copy_from_slice(&RNG_SEED.to_le_bytes());
    put_f32s(&mut out, 48, &CAMERA_POSITION);
    put_f32(&mut out, 60, CAMERA_YAW);
    put_f32(&mut out, 64, CAMERA_PITCH);
    put_f32(&mut out, 68, CAMERA_FOV);
    put_f32(&mut out, 72, CAMERA_NEAR);
    put_f32(&mut out, 76, CAMERA_FAR);
    put_f32s(&mut out, 80, &CLEAR_RGBA);
    put_f32s(&mut out, 96, &AMBIENT_RGB);
    put_f32(&mut out, 108, FLOOR_EXTENT);
    put_f32s(&mut out, 112, &FLOOR_RGBA);
    for (index, material) in MATERIALS.iter().enumerate() {
        put_f32s(&mut out, 128 + index * 16, &material.rgba);
    }
    for (index, extents) in SHAPE_HALF_EXTENTS.iter().enumerate() {
        put_f32s(&mut out, 192 + index * 12, extents);
    }
    for (offset, value) in [
        (228, TIME_STEP),
        (232, ORBIT_RADIUS),
        (236, ORBIT_RADIUS_AMPLITUDE),
        (240, HEIGHT_BASE),
        (244, HEIGHT_AMPLITUDE),
        (248, RADIUS_PHASE_SCALE),
        (252, HEIGHT_PHASE_SCALE),
        (256, ROTATION_SCALE),
        (260, SPAWN_RADIUS_RANGE[0]),
        (264, SPAWN_RADIUS_RANGE[1]),
        (268, SPAWN_HEIGHT_RANGE[0]),
        (272, SPAWN_HEIGHT_RANGE[1]),
        (276, SCALE_RANGE[0]),
        (280, SCALE_RANGE[1]),
        (284, SPEED_RANGE[0]),
        (288, SPEED_RANGE[1]),
    ] {
        put_f32(&mut out, offset, value);
    }
    out
}

fn put_u16(out: &mut [u8], offset: usize, value: u16) {
    out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn put_u32(out: &mut [u8], offset: usize, value: u32) {
    out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_f32(out: &mut [u8], offset: usize, value: f32) {
    out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_f32s(out: &mut [u8], offset: usize, values: &[f32]) {
    for (index, value) in values.iter().copied().enumerate() {
        put_f32(out, offset + index * 4, value);
    }
}
