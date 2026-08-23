//! Native HelioV-style voxel world.
//!
//! This is the Linux-native continuation of the original HelioV product target:
//! a deterministic, face-culled voxel world owned by Helio + SceneDB.  It does
//! not use the TRUEOS VMX/UI4 backend, so it is runnable with the ordinary
//! winit/wgpu desktop path.
//!
//! Controls: click to capture the mouse; WASD to fly; Space/Shift vertically;
//! mouse wheel selects a Brixel block; left click places it on the targeted
//! voxel face; I/K look up/down and J/L look left/right without a mouse; Tab
//! toggles world axes; Escape releases the mouse, then exits.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod v3_demo_common;

use glam::{Mat4, Vec2, Vec3};
use helio::{
    portal_pose_facing, required_wgpu_limits, Camera, DebugDrawState, FlyCamera, FlyCameraConfig,
    GpuLight, GpuMaterial, GroupMask, LightType, MaterialAsset, MaterialId, MaterialTextureRef,
    MaterialTextures, MeshId, ObjectDescriptor, ObjectId, PackedVertex, PerspectiveLens,
    PortalDescriptor, PortalId, Renderer, RendererConfig, Scene, SceneActor, TextureSamplerDesc,
    TextureTransform, TextureUpload,
};
use helio_controls::WinitFlyInput;
use helio_default_graphs::build_default_graph;
use helio_pass_debug_overlay::DebugOverlayState;
use microfont::{stamp_text, FHEIGHT};
use v3_demo_common::{box_mesh, make_material};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

// Matches the retired upstream VoxelTerrain generator's dense 64³ source
// volume. We retain its world rules but emit ordinary face-culled cubes.
const WORLD_SIDE: i32 = 64;
const WORLD_HEIGHT: i32 = 64;
const WORLD_SEED: u32 = 1;
// Keep the endpoints well inside f32 precision trouble, yet much farther than
// the demo world. These are ordinary scene-space lines, not a screen overlay.
const AXIS_REACH: f32 = 1_024.0;
// The debug renderer has line primitives rather than a width control. A close
// parallel world-space line makes each axis visibly heavier without another pass.
const AXIS_COMPANION_OFFSET: f32 = 0.025;
const FPS_TEXTURE_WIDTH: u32 = 64;
const FPS_TEXTURE_HEIGHT: u32 = 16;
const FPS_SCALE: f32 = 2.0;
const STATUS_TEXTURE_WIDTH: u32 = 96;
const STATUS_TEXTURE_HEIGHT: u32 = 44;
const STATUS_SCALE: f32 = 2.0;
const BLOCK_PREVIEW_SIZE: u32 = 100;
const BLOCK_PREVIEW_MARGIN: f32 = 50.0;
const CHUNK_WORLD_SIZE: f32 = WORLD_SIDE as f32;
const FINAL_CHUNK_COUNT: usize = 64;
const FINAL_CHUNK_COLUMNS: usize = 8;
const RECYCLED_CHUNK_MOVABILITY: helio::Movability = helio::Movability::Movable;
const VOXEL_FACE_TILE_SIZE: u32 = 64;
const VOXEL_FACE_ATLAS_WIDTH: u32 = VOXEL_FACE_TILE_SIZE * 3;
// Complete 64 -> 1 chain. The atlas stays three tiles wide at every level.
const VOXEL_FACE_MIP_LEVELS: u32 = 7;
const VOXEL_BLOCK_MATERIALS: usize = 8;
const VOXEL_FACE_SLOTS: usize = 3;
const VOXEL_DRAW_MATERIALS: usize = VOXEL_BLOCK_MATERIALS * VOXEL_FACE_SLOTS;
// Mirrors `UV_WRAP_BEFORE_TRANSFORM` in the G-buffer shader. It preserves
// per-block texture repeats across a greedily merged voxel face before its
// material transform selects an atlas tile.
const VOXEL_ATLAS_REPEAT_UV: u32 = 0x8000_0000;
const VOXEL_BEVEL_RADIUS: f32 = 0.018;
const VOXEL_BEVEL_SEGMENTS: usize = 3;
const ACTIVE_VOXEL_BEVEL_SEGMENTS: usize = 0;
const CHUNK_LOOK_AHEAD: f32 = CHUNK_WORLD_SIZE * 0.75;
const PORTAL_HALF_EXTENT: Vec2 = Vec2::new(2.2, 3.2);
const PORTAL_CENTER_Y: f32 = 35.0;
const PORTAL_CENTER_Z: f32 = 32.0;
const PORTAL_LEFT_X: f32 = 10.0;
const PORTAL_RIGHT_X: f32 = 54.0;
const PORTAL_NEAR_Z: f32 = 10.0;
const PORTAL_FAR_Z: f32 = 54.0;
// Keep the authored portal pairs, projection passes, and fly-camera crossings
// active in this showcase.
const ENABLE_PORTAL_EXPERIMENT: bool = true;
const BUILD_REACH: f32 = 10.0;
const SELECTION_OUTLINE_INSET: f32 = 0.015;

const FPS_OVERLAY_SHADER: &str = r#"
struct Rect { ndc: vec4<f32> }
@group(0) @binding(0) var font_tex: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;
@group(0) @binding(2) var<uniform> rect: Rect;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[index];
    var out: VertexOut;
    out.position = vec4<f32>(mix(rect.ndc.xy, rect.ndc.zw, corner), 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(font_tex, font_sampler, input.uv);
}
"#;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
    Ore,
    Sand,
    Bricks,
    Gravel,
    Snow,
}

impl Block {
    fn material_index(self) -> Option<usize> {
        match self {
            Self::Grass => Some(0),
            Self::Dirt => Some(1),
            Self::Stone => Some(2),
            Self::Ore => Some(3),
            Self::Sand => Some(4),
            Self::Bricks => Some(5),
            Self::Gravel => Some(6),
            Self::Snow => Some(7),
            Self::Air => None,
        }
    }
}

struct VoxelWorld {
    blocks: Vec<Block>,
}

impl VoxelWorld {
    fn empty() -> Self {
        Self {
            blocks: vec![Block::Air; (WORLD_SIDE * WORLD_SIDE * WORLD_HEIGHT) as usize],
        }
    }

    fn new() -> Self {
        let mut world = Self::empty();

        // This is the upstream `VoxelTerrain::generate(seed)` logic, kept
        // deliberately byte-for-byte equivalent in its noise parameters and
        // material thresholds. Only the final meshing backend is different.
        let base_height = WORLD_HEIGHT as f32 * 0.45;
        let amplitude = WORLD_HEIGHT as f32 * 0.22;
        let frequency = 1.0 / 18.0;
        for z in 0..WORLD_SIDE {
            for x in 0..WORLD_SIDE {
                let height = base_height
                    + fbm2(x as f32 * frequency, z as f32 * frequency, WORLD_SEED, 4) * amplitude;
                // Broad material patches break up the repeated height field
                // without adding another renderer path. Keep the frequency
                // well below the height noise so sand/earth/stone read as
                // terrain regions rather than per-voxel confetti.
                let biome = fbm2(
                    x as f32 / 42.0,
                    z as f32 / 42.0,
                    WORLD_SEED ^ 0x6b8b_4567,
                    3,
                );
                for y in 0..WORLD_HEIGHT {
                    let depth = height - y as f32;
                    if depth < 0.0 {
                        continue;
                    }
                    let mut block = if biome < -0.38 && depth < 3.0 {
                        Block::Sand
                    } else if biome > 0.48 && depth < 1.0 {
                        Block::Stone
                    } else if biome > 0.18 && biome < 0.32 && depth < 1.0 {
                        Block::Dirt
                    } else if depth < 1.0 {
                        Block::Grass
                    } else if depth < 4.0 {
                        Block::Dirt
                    } else {
                        Block::Stone
                    };
                    if block == Block::Stone
                        && terrain_hash(x, y, z, WORLD_SEED ^ 0x1234_5678) > 0.985
                    {
                        block = Block::Ore;
                    }
                    world.set(x, y, z, block);
                }
            }
        }
        world
    }

    fn index(x: i32, y: i32, z: i32) -> Option<usize> {
        if (0..WORLD_SIDE).contains(&x)
            && (0..WORLD_HEIGHT).contains(&y)
            && (0..WORLD_SIDE).contains(&z)
        {
            Some((x + WORLD_SIDE * (z + WORLD_SIDE * y)) as usize)
        } else {
            None
        }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> Block {
        Self::index(x, y, z)
            .and_then(|index| self.blocks.get(index).copied())
            .unwrap_or(Block::Air)
    }

    fn set(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if let Some(index) = Self::index(x, y, z) {
            self.blocks[index] = block;
        }
    }

    /// Neighbor occupancy used only while building the repeating chunk mesh.
    /// X/Z wrap because every recycled chunk is the same 64x64 terrain tile:
    /// an edge face is needed only when the voxel across the corresponding
    /// opposite edge is actually air. Below the terrain allocation is treated
    /// as solid, eliminating the enormous invisible underside sheet; above it
    /// remains air so peaks keep their top faces.
    fn meshing_neighbor_is_air(&self, x: i32, y: i32, z: i32) -> bool {
        if y < 0 {
            return false;
        }
        if y >= WORLD_HEIGHT {
            return true;
        }
        self.get(x.rem_euclid(WORLD_SIDE), y, z.rem_euclid(WORLD_SIDE)) == Block::Air
    }

    fn meshes(&self, bevel_segments: usize) -> [helio::MeshUpload; VOXEL_DRAW_MATERIALS] {
        let mut meshes: [helio::MeshUpload; VOXEL_DRAW_MATERIALS] =
            std::array::from_fn(|_| helio::MeshUpload {
                vertices: Vec::new(),
                indices: Vec::new(),
            });
        for y in 0..WORLD_HEIGHT {
            for z in 0..WORLD_SIDE {
                for x in 0..WORLD_SIDE {
                    let block = self.get(x, y, z);
                    let Some(material) = block.material_index() else {
                        continue;
                    };
                    let mut exposed = FACES.map(|face| {
                        self.meshing_neighbor_is_air(
                            x + face.neighbor[0],
                            y + face.neighbor[1],
                            z + face.neighbor[2],
                        )
                    });
                    // Give the visible terrain shell two healthy sides. A
                    // voxel that owns an exposed top also owns one downward
                    // quad, even when solid terrain continues below it. Pure
                    // wall voxels do not gain bottoms, and all shared side
                    // faces remain culled.
                    if exposed[0] {
                        exposed[1] = true;
                    }
                    if exposed.iter().any(|&value| value) {
                        if bevel_segments == 0 {
                            add_sharp_voxel(&mut meshes, material, x, y, z, exposed);
                        } else {
                            add_beveled_voxel(
                                &mut meshes,
                                material,
                                x,
                                y,
                                z,
                                exposed,
                                bevel_segments,
                            );
                        }
                    }
                }
            }
        }
        meshes
    }
}

fn add_sharp_voxel(
    meshes: &mut [helio::MeshUpload; VOXEL_DRAW_MATERIALS],
    material: usize,
    x: i32,
    y: i32,
    z: i32,
    exposed: [bool; 6],
) {
    let offset = Vec3::new(
        x as f32 - WORLD_SIDE as f32 * 0.5,
        y as f32 - WORLD_HEIGHT as f32 * 0.5,
        z as f32 - WORLD_SIDE as f32 * 0.5,
    );
    for (face_index, face) in FACES.into_iter().enumerate() {
        if !exposed[face_index] {
            continue;
        }
        let normal = Vec3::from_array(face.normal);
        add_bevel_quad(
            &mut meshes[material * 3 + face.texture_slot],
            offset,
            face_index,
            face.corners
                .map(|corner| (Vec3::from_array(corner), normal)),
        );
    }
}

// Copied from the old upstream `VoxelTerrain` source. Keep the integer
// wrapping and octave seed offsets intact: they define the world, not just an
// aesthetic approximation of it.
fn terrain_hash(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut hash = (x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((y as u32).wrapping_mul(668_265_263))
        .wrapping_add((z as u32).wrapping_mul(2_654_435_761))
        .wrapping_add(seed.wrapping_mul(2_246_822_519));
    hash = (hash ^ (hash >> 15)).wrapping_mul(2_246_822_519);
    hash = (hash ^ (hash >> 13)).wrapping_mul(3_266_489_917);
    hash ^= hash >> 16;
    (hash as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn value_noise2(x: f32, z: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let sx = smoothstep(x - x0 as f32);
    let sz = smoothstep(z - z0 as f32);
    let n00 = terrain_hash(x0, 0, z0, seed);
    let n10 = terrain_hash(x0 + 1, 0, z0, seed);
    let n01 = terrain_hash(x0, 0, z0 + 1, seed);
    let n11 = terrain_hash(x0 + 1, 0, z0 + 1, seed);
    let top = n00 + (n10 - n00) * sx;
    let bottom = n01 + (n11 - n01) * sx;
    top + (bottom - top) * sz
}

fn fbm2(x: f32, z: f32, seed: u32, octaves: u32) -> f32 {
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut sum = 0.0;
    let mut normalization = 0.0;
    for octave in 0..octaves {
        sum += value_noise2(
            x * frequency,
            z * frequency,
            seed.wrapping_add(octave * 101),
        ) * amplitude;
        normalization += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    sum / normalization
}

#[derive(Clone, Copy)]
struct Face {
    neighbor: [i32; 3],
    texture_slot: usize,
    normal: [f32; 3],
    tangent: [f32; 3],
    corners: [[f32; 3]; 4],
}

const FACES: [Face; 6] = [
    Face {
        neighbor: [0, 1, 0],
        texture_slot: 0,
        normal: [0.0, 1.0, 0.0],
        tangent: [1.0, 0.0, 0.0],
        corners: [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]],
    },
    Face {
        neighbor: [0, -1, 0],
        texture_slot: 2,
        normal: [0.0, -1.0, 0.0],
        tangent: [1.0, 0.0, 0.0],
        corners: [[0., 0., 1.], [0., 0., 0.], [1., 0., 0.], [1., 0., 1.]],
    },
    Face {
        neighbor: [0, 0, 1],
        texture_slot: 1,
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        corners: [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]],
    },
    Face {
        neighbor: [0, 0, -1],
        texture_slot: 1,
        normal: [0.0, 0.0, -1.0],
        tangent: [-1.0, 0.0, 0.0],
        corners: [[1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [1., 1., 0.]],
    },
    Face {
        neighbor: [1, 0, 0],
        texture_slot: 1,
        normal: [1.0, 0.0, 0.0],
        tangent: [0.0, 0.0, -1.0],
        corners: [[1., 0., 1.], [1., 0., 0.], [1., 1., 0.], [1., 1., 1.]],
    },
    Face {
        neighbor: [-1, 0, 0],
        texture_slot: 1,
        normal: [-1.0, 0.0, 0.0],
        tangent: [0.0, 0.0, 1.0],
        corners: [[0., 0., 0.], [0., 0., 1.], [0., 1., 1.], [0., 1., 0.]],
    },
];

fn face_for_axis(axis: usize, positive: bool) -> usize {
    match (axis, positive) {
        (0, true) => 4,
        (0, false) => 5,
        (1, true) => 0,
        (1, false) => 1,
        (2, true) => 2,
        (2, false) => 3,
        _ => unreachable!(),
    }
}

fn bevel_coordinate(positive: bool, inset: f32) -> f32 {
    if positive {
        1.0 - inset
    } else {
        inset
    }
}

fn bevel_uv(position: Vec3, face_index: usize) -> [f32; 2] {
    match face_index {
        0 | 1 => [position.x, position.z],
        2 => [position.x, position.y],
        3 => [1.0 - position.x, position.y],
        4 => [1.0 - position.z, position.y],
        5 => [position.z, position.y],
        _ => unreachable!(),
    }
}

fn add_bevel_triangle(
    mesh: &mut helio::MeshUpload,
    offset: Vec3,
    face_index: usize,
    mut points: [(Vec3, Vec3); 3],
) {
    if (points[1].0 - points[0].0)
        .cross(points[2].0 - points[0].0)
        .dot(points[0].1 + points[1].1 + points[2].1)
        < 0.0
    {
        points.swap(1, 2);
    }
    let base = mesh.vertices.len() as u32;
    for (position, normal) in points {
        mesh.vertices.push(PackedVertex::from_components(
            (offset + position).to_array(),
            normal.normalize().to_array(),
            bevel_uv(position, face_index),
            FACES[face_index].tangent,
            1.0,
        ));
    }
    mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
}

fn add_bevel_quad(
    mesh: &mut helio::MeshUpload,
    offset: Vec3,
    face_index: usize,
    points: [(Vec3, Vec3); 4],
) {
    add_bevel_triangle(mesh, offset, face_index, [points[0], points[1], points[2]]);
    add_bevel_triangle(mesh, offset, face_index, [points[0], points[2], points[3]]);
}

fn add_beveled_voxel(
    meshes: &mut [helio::MeshUpload; VOXEL_DRAW_MATERIALS],
    material: usize,
    x: i32,
    y: i32,
    z: i32,
    exposed: [bool; 6],
    bevel_segments: usize,
) {
    let offset = Vec3::new(
        x as f32 - WORLD_SIDE as f32 * 0.5,
        y as f32 - WORLD_HEIGHT as f32 * 0.5,
        z as f32 - WORLD_SIDE as f32 * 0.5,
    );
    let r = VOXEL_BEVEL_RADIUS;

    // Emit center quads only where the voxel actually touches air. Earlier
    // versions retained full shared faces as backing caps for the bevel
    // groove, but those invisible quads made every surface voxel pay for
    // neighbour-facing geometry as well.
    for (face_index, face) in FACES.into_iter().enumerate() {
        if !exposed[face_index] {
            continue;
        }
        let mut points = [(Vec3::ZERO, Vec3::ZERO); 4];
        for (index, corner) in face.corners.into_iter().enumerate() {
            let mut p = Vec3::from_array(corner);
            for axis in 0..3 {
                if face.neighbor[axis] == 0 {
                    p[axis] = if p[axis] == 0.0 { r } else { 1.0 - r };
                }
            }
            points[index] = (p, Vec3::from_array(face.normal));
        }
        add_bevel_quad(
            &mut meshes[material * 3 + face.texture_slot],
            offset,
            face_index,
            points,
        );
    }

    // Twelve rounded edges. Keep an edge whenever either incident face is
    // visible; this is what forms the shallow groove between adjacent blocks.
    for direction in 0..3 {
        let cross = match direction {
            0 => [1, 2],
            1 => [0, 2],
            _ => [0, 1],
        };
        for positive_a in [false, true] {
            for positive_b in [false, true] {
                let face_a = face_for_axis(cross[0], positive_a);
                let face_b = face_for_axis(cross[1], positive_b);
                let Some(owner) = [face_a, face_b].into_iter().find(|&face| exposed[face]) else {
                    continue;
                };
                let mesh_index = material * 3 + FACES[owner].texture_slot;
                let mut rings = vec![[(Vec3::ZERO, Vec3::ZERO); 2]; bevel_segments + 1];
                for (segment, ring) in rings.iter_mut().enumerate() {
                    let angle =
                        std::f32::consts::FRAC_PI_2 * segment as f32 / bevel_segments as f32;
                    for (end, along) in [r, 1.0 - r].into_iter().enumerate() {
                        let mut p = Vec3::splat(0.0);
                        p[direction] = along;
                        p[cross[0]] = bevel_coordinate(positive_a, r * (1.0 - angle.cos()));
                        p[cross[1]] = bevel_coordinate(positive_b, r * (1.0 - angle.sin()));
                        let mut n = Vec3::ZERO;
                        n[cross[0]] = if positive_a {
                            angle.cos()
                        } else {
                            -angle.cos()
                        };
                        n[cross[1]] = if positive_b {
                            angle.sin()
                        } else {
                            -angle.sin()
                        };
                        ring[end] = (p, n);
                    }
                }
                for segment in 0..bevel_segments {
                    add_bevel_quad(
                        &mut meshes[mesh_index],
                        offset,
                        owner,
                        [
                            rings[segment][0],
                            rings[segment][1],
                            rings[segment + 1][1],
                            rings[segment + 1][0],
                        ],
                    );
                }
            }
        }
    }

    // A spherical-octant fan closes each visible corner. Its perimeter uses
    // exactly the same three-segment arcs as the incident edge bands.
    for px in [false, true] {
        for py in [false, true] {
            for pz in [false, true] {
                let faces = [
                    face_for_axis(0, px),
                    face_for_axis(1, py),
                    face_for_axis(2, pz),
                ];
                let Some(owner) = faces.into_iter().find(|&face| exposed[face]) else {
                    continue;
                };
                let signs = Vec3::new(
                    if px { 1.0 } else { -1.0 },
                    if py { 1.0 } else { -1.0 },
                    if pz { 1.0 } else { -1.0 },
                );
                let boundary = Vec3::new(
                    if px { 1.0 } else { 0.0 },
                    if py { 1.0 } else { 0.0 },
                    if pz { 1.0 } else { 0.0 },
                );
                let center_normal = signs.normalize();
                let center = boundary - signs * (r * (Vec3::ONE - center_normal.abs()));
                let mut perimeter = Vec::with_capacity(bevel_segments * 3);
                for (axis_a, axis_b, fixed_axis) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)] {
                    for segment in 0..bevel_segments {
                        let angle =
                            std::f32::consts::FRAC_PI_2 * segment as f32 / bevel_segments as f32;
                        let mut n = Vec3::ZERO;
                        n[axis_a] = signs[axis_a] * angle.cos();
                        n[axis_b] = signs[axis_b] * angle.sin();
                        let mut p = boundary - signs * r;
                        // This is the longitudinal endpoint of the matching
                        // edge band, hence one radius inward rather than the
                        // original sharp-cube corner plane.
                        p[fixed_axis] = boundary[fixed_axis] - signs[fixed_axis] * r;
                        p[axis_a] = bevel_coordinate(signs[axis_a] > 0.0, r * (1.0 - angle.cos()));
                        p[axis_b] = bevel_coordinate(signs[axis_b] > 0.0, r * (1.0 - angle.sin()));
                        perimeter.push((p, n));
                    }
                }
                let mesh_index = material * 3 + FACES[owner].texture_slot;
                for index in 0..perimeter.len() {
                    add_bevel_triangle(
                        &mut meshes[mesh_index],
                        offset,
                        owner,
                        [
                            (center, center_normal),
                            perimeter[index],
                            perimeter[(index + 1) % perimeter.len()],
                        ],
                    );
                }
            }
        }
    }
}

/// Eight color-only Brixel face atlases forged at design time. Each PNG
/// contains top, side, and bottom views in that order.
const VOXEL_TEXTURES: [&[u8]; VOXEL_BLOCK_MATERIALS] = [
    include_bytes!("assets/heliov/brixel/voxel_faces/grass.png"),
    include_bytes!("assets/heliov/brixel/voxel_faces/dirt.png"),
    include_bytes!("assets/heliov/brixel/voxel_faces/stone.png"),
    include_bytes!("assets/heliov/brixel/voxel_faces/coal_ore.png"),
    include_bytes!("assets/heliov/brixel/voxel_faces/sand.png"),
    include_bytes!("assets/heliov/brixel/voxel_faces/stonebrick.png"),
    include_bytes!("assets/heliov/brixel/voxel_faces/gravel.png"),
    include_bytes!("assets/heliov/brixel/voxel_faces/snow.png"),
];
const VOXEL_NORMAL_TEXTURES: [&[u8]; VOXEL_BLOCK_MATERIALS] = [
    include_bytes!("assets/heliov/brixel/voxel_normals/grass.png"),
    include_bytes!("assets/heliov/brixel/voxel_normals/dirt.png"),
    include_bytes!("assets/heliov/brixel/voxel_normals/stone.png"),
    include_bytes!("assets/heliov/brixel/voxel_normals/coal_ore.png"),
    include_bytes!("assets/heliov/brixel/voxel_normals/sand.png"),
    include_bytes!("assets/heliov/brixel/voxel_normals/stonebrick.png"),
    include_bytes!("assets/heliov/brixel/voxel_normals/gravel.png"),
    include_bytes!("assets/heliov/brixel/voxel_normals/snow.png"),
];
const VOXEL_MATERIAL_TEXTURES: [&[u8]; VOXEL_BLOCK_MATERIALS] = [
    include_bytes!("assets/heliov/brixel/voxel_materials/grass.png"),
    include_bytes!("assets/heliov/brixel/voxel_materials/dirt.png"),
    include_bytes!("assets/heliov/brixel/voxel_materials/stone.png"),
    include_bytes!("assets/heliov/brixel/voxel_materials/coal_ore.png"),
    include_bytes!("assets/heliov/brixel/voxel_materials/sand.png"),
    include_bytes!("assets/heliov/brixel/voxel_materials/stonebrick.png"),
    include_bytes!("assets/heliov/brixel/voxel_materials/gravel.png"),
    include_bytes!("assets/heliov/brixel/voxel_materials/snow.png"),
];
const SHOWCASE_COLOR: &[u8] = include_bytes!("assets/heliov/brixel/showcase/color.png");
const SHOWCASE_NORMAL: &[u8] = include_bytes!("assets/heliov/brixel/showcase/normal.png");
const SHOWCASE_MATERIAL: &[u8] = include_bytes!("assets/heliov/brixel/showcase/material.png");
const SHOWCASE_BLOCK_COUNT: &str = include_str!("assets/heliov/brixel/showcase/count.txt");
const SHOWCASE_MANIFEST: &str = include_str!("assets/heliov/brixel/showcase/manifest.json");
const SHOWCASE_TILE_SIZE: u32 = 64;
const SHOWCASE_BLOCK_SPACING: f32 = 2.0;

fn crossed_plant_mesh() -> helio::MeshUpload {
    let mut mesh = helio::MeshUpload {
        vertices: Vec::with_capacity(8),
        indices: Vec::with_capacity(12),
    };
    append_crossed_plant(&mut mesh, Vec3::ZERO);
    mesh
}

fn append_crossed_plant(mesh: &mut helio::MeshUpload, center: Vec3) {
    let first_vertex = mesh.vertices.len() as u32;
    for (a, b, normal) in [
        (
            Vec3::new(-0.5, 0.0, -0.5),
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(-1.0, 0.0, 1.0).normalize(),
        ),
        (
            Vec3::new(-0.5, 0.0, 0.5),
            Vec3::new(0.5, 0.0, -0.5),
            Vec3::new(1.0, 0.0, 1.0).normalize(),
        ),
    ] {
        for (position, uv) in [
            (center + a + Vec3::NEG_Y * 0.5, [0.0, 1.0]),
            (center + a + Vec3::Y * 0.5, [0.0, 0.0]),
            (center + b + Vec3::Y * 0.5, [1.0, 0.0]),
            (center + b + Vec3::NEG_Y * 0.5, [1.0, 1.0]),
        ] {
            mesh.vertices.push(PackedVertex::from_components(
                position.to_array(),
                normal.to_array(),
                uv,
                (b - a).normalize().to_array(),
                1.0,
            ));
        }
    }
    mesh.indices.extend([
        first_vertex,
        first_vertex + 1,
        first_vertex + 2,
        first_vertex,
        first_vertex + 2,
        first_vertex + 3,
        first_vertex + 4,
        first_vertex + 5,
        first_vertex + 6,
        first_vertex + 4,
        first_vertex + 6,
        first_vertex + 7,
    ]);
}

fn terrain_flower_mesh(world: &VoxelWorld) -> helio::MeshUpload {
    let mut mesh = helio::MeshUpload {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for z in 0..WORLD_SIDE {
        for x in 0..WORLD_SIDE {
            let Some(y) = (0..WORLD_HEIGHT)
                .rev()
                .find(|&y| world.get(x, y, z) != Block::Air)
            else {
                continue;
            };
            if world.get(x, y, z) != Block::Grass
                || terrain_hash(x, y, z, WORLD_SEED ^ 0xf10f_e123) < 0.975
            {
                continue;
            }
            append_crossed_plant(
                &mut mesh,
                Vec3::new(
                    x as f32 - WORLD_SIDE as f32 * 0.5 + 0.5,
                    y as f32 - WORLD_HEIGHT as f32 * 0.5 + 1.5,
                    z as f32 - WORLD_SIDE as f32 * 0.5 + 0.5,
                ),
            );
        }
    }
    mesh
}

fn is_crossed_plant(name: &str) -> bool {
    name.starts_with("flower_")
        || name.starts_with("sapling_")
        || name.starts_with("beetroots_stage_")
        || name.starts_with("double_plant_")
        || matches!(
            name,
            "azalea_plant"
                | "bamboo_sapling"
                | "cherry_sapling"
                | "crimson_roots"
                | "deadbush"
                | "fern"
                | "fern_carried"
                | "grass_carried"
                | "mushroom_brown"
                | "mushroom_red"
                | "nether_sprouts"
                | "seagrass_carried"
                | "tallgrass"
                | "warped_roots"
        )
}

fn showcase_names() -> Vec<&'static str> {
    let names = SHOWCASE_MANIFEST
        .split_once("\"ordered_names\": [")
        .expect("showcase manifest has ordered_names")
        .1
        .split_once(']')
        .expect("showcase ordered_names is closed")
        .0;
    names
        .lines()
        .filter_map(|line| line.trim().trim_end_matches(',').strip_prefix('"'))
        .map(|line| line.strip_suffix('"').expect("showcase name closes quote"))
        .collect()
}

fn voxel_face_texture_mips(encoded: &[u8]) -> Vec<Vec<u8>> {
    embedded_texture_mips(
        encoded,
        VOXEL_FACE_ATLAS_WIDTH,
        VOXEL_FACE_TILE_SIZE,
        VOXEL_FACE_MIP_LEVELS,
    )
}

fn embedded_texture_mips(encoded: &[u8], width: u32, height: u32, levels: u32) -> Vec<Vec<u8>> {
    let source = image::load_from_memory(encoded)
        .expect("embedded Brixel atlas is valid PNG")
        .into_rgba8();
    assert_eq!(source.dimensions(), (width, height));
    (0..levels)
        .map(|level| {
            image::imageops::resize(
                &source,
                (width >> level).max(1),
                (height >> level).max(1),
                image::imageops::FilterType::Triangle,
            )
            .into_raw()
        })
        .collect()
}

fn material(
    texture: helio::TextureId,
    normal_texture: helio::TextureId,
    material_texture: helio::TextureId,
    face: usize,
    roughness: f32,
) -> MaterialAsset {
    let mut textures = MaterialTextures::default();
    let atlas_ref = |texture| MaterialTextureRef {
        texture,
        uv_channel: VOXEL_ATLAS_REPEAT_UV,
        transform: TextureTransform {
            offset: [face as f32 / 3.0, 0.0],
            scale: [1.0 / 3.0, 1.0],
            rotation_radians: 0.0,
        },
    };
    textures.base_color = Some(atlas_ref(texture));
    textures.normal = Some(atlas_ref(normal_texture));
    textures.roughness_metallic = Some(atlas_ref(material_texture));
    MaterialAsset {
        gpu: GpuMaterial {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0, 0.0, 0.0, 0.0],
            // Multipliers for the forged Brixel texture's G=roughness and
            // B=metalness channels. Non-metal terrain pixels remain zero.
            roughness_metallic: [roughness, 1.0, 1.5, 0.5],
            // SceneDB resolves the real residency index from `textures` during
            // flush; this merely marks the base-colour slot as present.
            tex_base_color: 0,
            tex_normal: 0,
            tex_roughness: 0,
            tex_emissive: GpuMaterial::NO_TEXTURE,
            tex_occlusion: GpuMaterial::NO_TEXTURE,
            workflow: 0,
            flags: libhelio::FLAG_HAS_NORMAL_MAP,
            material_class: 0,
            class_params: [0.0; 4],
        },
        textures,
    }
}

fn sun() -> GpuLight {
    GpuLight {
        position_range: [0.0, 0.0, 0.0, f32::MAX],
        direction_outer: [-0.35, -0.8, -0.25, 0.0],
        color_intensity: [1.0, 0.92, 0.78, 3.0],
        // The current directional CSM footprint is visibly finite in this
        // recycled-chunk scene: as the camera moves, its cascade projection
        // paints a cool rectangular island over otherwise warm-lit terrain.
        // Keep the directional sun, but leave it unshadowed until the cascade
        // coverage/blending path is made seamless for moving chunk worlds.
        shadow_index: u32::MAX,
        light_type: LightType::Directional as u32,
        inner_angle: 0.0,
        _pad: 0,
        ..Default::default()
    }
}

fn add_world_axes(renderer: &mut Renderer) {
    renderer.debug_batch(|debug| {
        debug.line(
            [-AXIS_REACH, 0.0, 0.0],
            [AXIS_REACH, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.5],
        );
        debug.line(
            [-AXIS_REACH, 0.0, AXIS_COMPANION_OFFSET],
            [AXIS_REACH, 0.0, AXIS_COMPANION_OFFSET],
            [1.0, 0.0, 0.0, 0.5],
        );
        debug.line(
            [0.0, -AXIS_REACH, 0.0],
            [0.0, AXIS_REACH, 0.0],
            [0.0, 1.0, 0.0, 0.5],
        );
        debug.line(
            [AXIS_COMPANION_OFFSET, -AXIS_REACH, 0.0],
            [AXIS_COMPANION_OFFSET, AXIS_REACH, 0.0],
            [0.0, 1.0, 0.0, 0.5],
        );
        debug.line(
            [0.0, 0.0, -AXIS_REACH],
            [0.0, 0.0, AXIS_REACH],
            [0.0, 0.4, 1.0, 0.5],
        );
        debug.line(
            [AXIS_COMPANION_OFFSET, 0.0, -AXIS_REACH],
            [AXIS_COMPANION_OFFSET, 0.0, AXIS_REACH],
            [0.0, 0.4, 1.0, 0.5],
        );
    });
}

/// A deliberately tiny CPU overlay: MicroFont stamps into a 64x16 RGBA image
/// four times per second, then one six-vertex draw places it over the surface.
struct FpsOverlay {
    _texture: wgpu::Texture,
    pixels: Vec<u32>,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    rect_buffer: wgpu::Buffer,
    elapsed: f32,
    frames: u32,
}

struct BlockPreviewOverlay {
    _texture: wgpu::Texture,
    atlas: image::RgbaImage,
    atlas_columns: usize,
    base_pixels: Vec<u8>,
    angle: f32,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    rect_buffer: wgpu::Buffer,
}

impl BlockPreviewOverlay {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let atlas = image::load_from_memory(SHOWCASE_COLOR)
            .expect("embedded Brixel showcase color atlas is valid PNG")
            .into_rgba8();
        let atlas_columns = (atlas.width() / SHOWCASE_TILE_SIZE) as usize;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HelioV selected block preview"),
            size: wgpu::Extent3d {
                width: BLOCK_PREVIEW_SIZE,
                height: BLOCK_PREVIEW_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let rect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HelioV selected block preview rect"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HelioV selected block preview layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HelioV selected block preview bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: rect_buffer.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HelioV selected block preview shader"),
            source: wgpu::ShaderSource::Wgsl(FPS_OVERLAY_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HelioV selected block preview pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("HelioV selected block preview pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let mut overlay = Self {
            _texture: texture,
            atlas,
            atlas_columns,
            base_pixels: vec![0; (BLOCK_PREVIEW_SIZE * BLOCK_PREVIEW_SIZE * 4) as usize],
            angle: 0.0,
            pipeline,
            bind_group,
            rect_buffer,
        };
        overlay.select(queue, 0);
        overlay
    }

    fn select(&mut self, queue: &wgpu::Queue, selected: usize) {
        let tile_x = (selected % self.atlas_columns) as u32 * SHOWCASE_TILE_SIZE;
        let tile_y = (selected / self.atlas_columns) as u32 * SHOWCASE_TILE_SIZE;
        let mut pixels = vec![0u8; (BLOCK_PREVIEW_SIZE * BLOCK_PREVIEW_SIZE * 4) as usize];
        let faces = [
            ([50.0, 15.0], [35.0, 17.5], [-35.0, 17.5], 1.0),
            ([15.0, 32.5], [35.0, 17.5], [0.0, 35.0], 0.72),
            ([50.0, 50.0], [35.0, -17.5], [0.0, 35.0], 0.88),
        ];
        for (origin, axis_u, axis_v, shade) in faces {
            for v in 0..SHOWCASE_TILE_SIZE {
                for u in 0..SHOWCASE_TILE_SIZE {
                    let fu = (u as f32 + 0.5) / SHOWCASE_TILE_SIZE as f32;
                    let fv = (v as f32 + 0.5) / SHOWCASE_TILE_SIZE as f32;
                    let x = (origin[0] + axis_u[0] * fu + axis_v[0] * fv).round() as i32;
                    let y = (origin[1] + axis_u[1] * fu + axis_v[1] * fv).round() as i32;
                    if !(0..BLOCK_PREVIEW_SIZE as i32).contains(&x)
                        || !(0..BLOCK_PREVIEW_SIZE as i32).contains(&y)
                    {
                        continue;
                    }
                    let source = self.atlas.get_pixel(tile_x + u, tile_y + v).0;
                    let target = (x as u32 + y as u32 * BLOCK_PREVIEW_SIZE) as usize * 4;
                    pixels[target] = (source[0] as f32 * shade) as u8;
                    pixels[target + 1] = (source[1] as f32 * shade) as u8;
                    pixels[target + 2] = (source[2] as f32 * shade) as u8;
                    pixels[target + 3] = source[3];
                }
            }
        }
        self.base_pixels = pixels;
        self.upload_rotated(queue);
    }

    fn tick(&mut self, queue: &wgpu::Queue, delta_seconds: f32) {
        self.angle = (self.angle + delta_seconds * 0.35) % std::f32::consts::TAU;
        self.upload_rotated(queue);
    }

    fn upload_rotated(&self, queue: &wgpu::Queue) {
        let mut pixels = vec![0u8; self.base_pixels.len()];
        let center = (BLOCK_PREVIEW_SIZE as f32 - 1.0) * 0.5;
        let (sin, cos) = self.angle.sin_cos();
        for y in 0..BLOCK_PREVIEW_SIZE {
            for x in 0..BLOCK_PREVIEW_SIZE {
                let dx = x as f32 - center;
                let dy = y as f32 - center;
                let source_x = (center + dx * cos + dy * sin).round() as i32;
                let source_y = (center - dx * sin + dy * cos).round() as i32;
                if !(0..BLOCK_PREVIEW_SIZE as i32).contains(&source_x)
                    || !(0..BLOCK_PREVIEW_SIZE as i32).contains(&source_y)
                {
                    continue;
                }
                let source = (source_x as u32 + source_y as u32 * BLOCK_PREVIEW_SIZE) as usize * 4;
                let target = (x + y * BLOCK_PREVIEW_SIZE) as usize * 4;
                pixels[target..target + 4].copy_from_slice(&self.base_pixels[source..source + 4]);
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self._texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(BLOCK_PREVIEW_SIZE * 4),
                rows_per_image: Some(BLOCK_PREVIEW_SIZE),
            },
            wgpu::Extent3d {
                width: BLOCK_PREVIEW_SIZE,
                height: BLOCK_PREVIEW_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let left = width - BLOCK_PREVIEW_MARGIN - BLOCK_PREVIEW_SIZE as f32;
        let top = BLOCK_PREVIEW_MARGIN;
        let rect = [
            left / width * 2.0 - 1.0,
            1.0 - (top + BLOCK_PREVIEW_SIZE as f32) / height * 2.0,
            (left + BLOCK_PREVIEW_SIZE as f32) / width * 2.0 - 1.0,
            1.0 - top / height * 2.0,
        ];
        queue.write_buffer(&self.rect_buffer, 0, bytemuck::cast_slice(&rect));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("HelioV selected block preview encoder"),
        });
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("HelioV selected block preview pass"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
}

impl FpsOverlay {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HelioV MicroFont FPS texture"),
            size: wgpu::Extent3d {
                width: FPS_TEXTURE_WIDTH,
                height: FPS_TEXTURE_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("HelioV MicroFont nearest sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let rect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HelioV FPS rectangle"),
            size: std::mem::size_of::<[f32; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HelioV MicroFont FPS layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HelioV MicroFont FPS bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: rect_buffer.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HelioV MicroFont FPS shader"),
            source: wgpu::ShaderSource::Wgsl(FPS_OVERLAY_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HelioV MicroFont FPS pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("HelioV MicroFont FPS pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let mut overlay = Self {
            _texture: texture,
            pixels: vec![0; (FPS_TEXTURE_WIDTH * FPS_TEXTURE_HEIGHT) as usize],
            pipeline,
            bind_group,
            rect_buffer,
            elapsed: 0.0,
            frames: 0,
        };
        overlay.upload_text(queue, 0.0);
        overlay
    }

    fn upload_text(&mut self, queue: &wgpu::Queue, fps: f32) {
        self.pixels.fill(0);
        let text = format!("FPS {:3}", fps.round() as u32);
        stamp_text(
            &mut self.pixels,
            FPS_TEXTURE_WIDTH as usize,
            FPS_TEXTURE_HEIGHT as usize,
            0,
            ((FPS_TEXTURE_HEIGHT as usize - FHEIGHT) / 2) as i32,
            &text,
            u32::MAX,
        )
        .expect("MicroFont FPS texture dimensions");
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self._texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&self.pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(FPS_TEXTURE_WIDTH * 4),
                rows_per_image: Some(FPS_TEXTURE_HEIGHT),
            },
            wgpu::Extent3d {
                width: FPS_TEXTURE_WIDTH,
                height: FPS_TEXTURE_HEIGHT,
                depth_or_array_layers: 1,
            },
        );
    }

    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        output_size: winit::dpi::PhysicalSize<u32>,
        delta_seconds: f32,
    ) {
        self.elapsed += delta_seconds.max(0.0);
        self.frames += 1;
        if self.elapsed >= 0.25 {
            self.upload_text(queue, self.frames as f32 / self.elapsed);
            self.elapsed = 0.0;
            self.frames = 0;
        }
        let width = output_size.width.max(1) as f32;
        let height = output_size.height.max(1) as f32;
        let overlay_width = FPS_TEXTURE_WIDTH as f32 * FPS_SCALE;
        let overlay_height = FPS_TEXTURE_HEIGHT as f32 * FPS_SCALE;
        let margin = 8.0;
        let rect = [
            1.0 - 2.0 * (margin + overlay_width) / width,
            -1.0 + 2.0 * margin / height,
            1.0 - 2.0 * margin / width,
            -1.0 + 2.0 * (margin + overlay_height) / height,
        ];
        queue.write_buffer(&self.rect_buffer, 0, bytemuck::cast_slice(&rect));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("HelioV MicroFont FPS encoder"),
        });
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("HelioV MicroFont FPS pass"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
}

/// Compact, decisive game-state readout. It intentionally shares the tiny
/// MicroFont path with the FPS counter rather than adding a general UI layer.
struct StatusOverlay {
    texture: wgpu::Texture,
    pixels: Vec<u32>,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    rect_buffer: wgpu::Buffer,
    elapsed: f32,
}

impl StatusOverlay {
    fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HelioV MicroFont status texture"),
            size: wgpu::Extent3d {
                width: STATUS_TEXTURE_WIDTH,
                height: STATUS_TEXTURE_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("HelioV MicroFont status sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let rect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HelioV status rectangle"),
            size: std::mem::size_of::<[f32; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HelioV MicroFont status layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HelioV MicroFont status bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: rect_buffer.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HelioV MicroFont status shader"),
            source: wgpu::ShaderSource::Wgsl(FPS_OVERLAY_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HelioV MicroFont status pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("HelioV MicroFont status pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            texture,
            pixels: vec![0; (STATUS_TEXTURE_WIDTH * STATUS_TEXTURE_HEIGHT) as usize],
            pipeline,
            bind_group,
            rect_buffer,
            elapsed: 0.25,
        }
    }

    fn upload(&mut self, queue: &wgpu::Queue, lines: &[String; 4]) {
        self.pixels.fill(0);
        for (line, text) in lines.iter().enumerate() {
            stamp_text(
                &mut self.pixels,
                STATUS_TEXTURE_WIDTH as usize,
                STATUS_TEXTURE_HEIGHT as usize,
                0,
                (line * FHEIGHT) as i32,
                text,
                u32::MAX,
            )
            .expect("MicroFont status texture dimensions");
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&self.pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(STATUS_TEXTURE_WIDTH * 4),
                rows_per_image: Some(STATUS_TEXTURE_HEIGHT),
            },
            wgpu::Extent3d {
                width: STATUS_TEXTURE_WIDTH,
                height: STATUS_TEXTURE_HEIGHT,
                depth_or_array_layers: 1,
            },
        );
    }

    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        output_size: winit::dpi::PhysicalSize<u32>,
        delta_seconds: f32,
        lines: &[String; 4],
    ) {
        self.elapsed += delta_seconds.max(0.0);
        if self.elapsed >= 0.25 {
            self.upload(queue, lines);
            self.elapsed = 0.0;
        }
        let width = output_size.width.max(1) as f32;
        let height = output_size.height.max(1) as f32;
        let panel_width = STATUS_TEXTURE_WIDTH as f32 * STATUS_SCALE;
        let panel_height = STATUS_TEXTURE_HEIGHT as f32 * STATUS_SCALE;
        let margin = 8.0;
        let rect = [
            -1.0 + 2.0 * margin / width,
            -1.0 + 2.0 * margin / height,
            -1.0 + 2.0 * (margin + panel_width) / width,
            -1.0 + 2.0 * (margin + panel_height) / height,
        ];
        queue.write_buffer(&self.rect_buffer, 0, bytemuck::cast_slice(&rect));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("HelioV MicroFont status encoder"),
        });
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("HelioV MicroFont status pass"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
}

struct ChunkInstance {
    grid: [i32; 2],
    transform: Mat4,
    center: Vec3,
    objects: Vec<ObjectId>,
}

fn recycled_chunk_slot(grid_x: i32, grid_z: i32) -> usize {
    grid_x.rem_euclid(FINAL_CHUNK_COLUMNS as i32) as usize
        + grid_z.rem_euclid(FINAL_CHUNK_COLUMNS as i32) as usize * FINAL_CHUNK_COLUMNS
}

fn terrain_occupies_cell(state: &AppState, cell: [i32; 3]) -> bool {
    let center = Vec3::new(
        cell[0] as f32 + 0.5,
        cell[1] as f32 + 0.5,
        cell[2] as f32 + 0.5,
    );
    state.chunks.iter().any(|chunk| {
        let local = chunk.transform.inverse().transform_point3(center);
        if !(-32.0..32.0).contains(&local.x) || !(-32.0..32.0).contains(&local.z) {
            return false;
        }
        let x = (local.x + WORLD_SIDE as f32 * 0.5).floor() as i32;
        let y = (local.y + WORLD_HEIGHT as f32 * 0.5).floor() as i32;
        let z = (local.z + WORLD_SIDE as f32 * 0.5).floor() as i32;
        state.world.get(x, y, z) != Block::Air
    })
}

fn cell_is_occupied(state: &AppState, cell: [i32; 3]) -> bool {
    state.placed_cells.contains(&cell) || terrain_occupies_cell(state, cell)
}

/// Return the last empty grid cell before the fly-camera ray enters a solid
/// voxel. This is the cube that would be attached to the face under the
/// crosshair.
fn targeted_build_cell(state: &AppState) -> Option<[i32; 3]> {
    let origin = state.camera.position();
    let direction = state.camera.basis().forward.normalize_or_zero();
    if direction == Vec3::ZERO {
        return None;
    }
    let mut cell = origin.floor().as_ivec3();
    let step = direction.signum().as_ivec3();
    let mut side_distance = Vec3::ZERO;
    let mut delta_distance = Vec3::ZERO;
    for axis in 0..3 {
        if direction[axis].abs() < 1.0e-6 {
            side_distance[axis] = f32::INFINITY;
            delta_distance[axis] = f32::INFINITY;
        } else {
            delta_distance[axis] = 1.0 / direction[axis].abs();
            let boundary = if step[axis] > 0 {
                cell[axis] as f32 + 1.0
            } else {
                cell[axis] as f32
            };
            side_distance[axis] = (boundary - origin[axis]) / direction[axis];
        }
    }

    let mut previous_empty = None;
    loop {
        let current = cell.to_array();
        if cell_is_occupied(state, current) {
            return previous_empty;
        }
        previous_empty = Some(current);
        let axis = if side_distance.x <= side_distance.y && side_distance.x <= side_distance.z {
            0
        } else if side_distance.y <= side_distance.z {
            1
        } else {
            2
        };
        if side_distance[axis] > BUILD_REACH {
            return None;
        }
        cell[axis] += step[axis];
        side_distance[axis] += delta_distance[axis];
    }
}

fn place_selected_block(state: &mut AppState) {
    let Some(cell) = targeted_build_cell(state) else {
        return;
    };
    if !state.placed_cells.insert(cell) {
        return;
    }
    let center = Vec3::new(
        cell[0] as f32 + 0.5,
        cell[1] as f32 + 0.5,
        cell[2] as f32 + 0.5,
    );
    state
        .renderer
        .scene_mut()
        .insert_object(ObjectDescriptor {
            mesh: state.build_cube_mesh,
            material: state.build_materials[state.selected_build_material],
            transform: Mat4::from_translation(center),
            bounds: [center.x, center.y, center.z, 0.87],
            flags: 0b11,
            groups: GroupMask::NONE,
            movability: None,
            user_tag: u64::MAX - 2,
        })
        .expect("insert player-built voxel");
}

fn update_build_selection_title(state: &AppState) {
    state.window.set_title(&format!(
        "HelioV — {} ({}/{})",
        state.build_material_names[state.selected_build_material],
        state.selected_build_material + 1,
        state.build_materials.len(),
    ));
}

fn status_lines(state: &AppState, graph_rebuild_visible: bool) -> [String; 4] {
    let name = state.build_material_names[state.selected_build_material]
        .chars()
        .take(8)
        .collect::<String>();
    let target = targeted_build_cell(state)
        .map(|[x, y, z]| format!("A{:>3} {:>3} {:>3}", x, y, z))
        .unwrap_or_else(|| "AIM ---".to_owned());
    [
        format!("B{:03} {name}", state.selected_build_material + 1),
        target,
        format!("PLACED {:04}", state.placed_cells.len()),
        format!(
            "P{:02} G:{}",
            state.portal_ids.len(),
            if graph_rebuild_visible { "RE" } else { "OK" }
        ),
    ]
}

fn refresh_build_outline(state: &mut AppState) {
    state.renderer.debug_clear();
    if state.world_axes_visible {
        add_world_axes(&mut state.renderer);
    }
    let Some(cell) = targeted_build_cell(state) else {
        return;
    };
    let min = Vec3::new(cell[0] as f32, cell[1] as f32, cell[2] as f32)
        - Vec3::splat(SELECTION_OUTLINE_INSET);
    let max = min + Vec3::splat(1.0 + SELECTION_OUTLINE_INSET * 2.0);
    let color = [1.0, 0.86, 0.18, 1.0];
    state.renderer.debug_batch(|debug| {
        for (a, b) in [
            (0, 1),
            (1, 3),
            (3, 2),
            (2, 0),
            (4, 5),
            (5, 7),
            (7, 6),
            (6, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ] {
            let corner = |index: usize| {
                Vec3::new(
                    if index & 1 == 0 { min.x } else { max.x },
                    if index & 4 == 0 { min.y } else { max.y },
                    if index & 2 == 0 { min.z } else { max.z },
                )
                .to_array()
            };
            debug.line(corner(a), corner(b), color);
        }
    });
}

fn refresh_recycled_chunks(state: &mut AppState) {
    let camera = state.camera.position();
    let forward = state.camera.basis().forward;
    let horizontal_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let window_center = camera + horizontal_forward * CHUNK_LOOK_AHEAD;
    let anchor_x = (window_center.x / CHUNK_WORLD_SIZE).round() as i32;
    let anchor_z = (window_center.z / CHUNK_WORLD_SIZE).round() as i32;
    let first_x = anchor_x - 3;
    let first_z = anchor_z - 3;

    for grid_z in first_z..first_z + FINAL_CHUNK_COLUMNS as i32 {
        for grid_x in first_x..first_x + FINAL_CHUNK_COLUMNS as i32 {
            let slot = recycled_chunk_slot(grid_x, grid_z);
            let chunk = &mut state.chunks[slot];
            if chunk.grid == [grid_x, grid_z] {
                continue;
            }
            let center = Vec3::new(
                grid_x as f32 * CHUNK_WORLD_SIZE,
                0.0,
                grid_z as f32 * CHUNK_WORLD_SIZE,
            );
            let quarter_turn = (grid_x.wrapping_mul(31) ^ grid_z.wrapping_mul(17)).rem_euclid(4);
            let transform = Mat4::from_translation(center)
                * Mat4::from_rotation_y(quarter_turn as f32 * std::f32::consts::FRAC_PI_2);
            for &object in &chunk.objects {
                state
                    .renderer
                    .scene_mut()
                    .update_object_transform(object, transform)
                    .expect("move recycled voxel chunk");
                state
                    .renderer
                    .scene_mut()
                    .update_object_bounds(object, [center.x, 0.0, center.z, 64.0])
                    .expect("move recycled voxel chunk bounds");
            }
            chunk.grid = [grid_x, grid_z];
            chunk.center = center;
            chunk.transform = transform;
        }
    }
}

fn add_final_chunk_stage(state: &mut AppState) {
    debug_assert!(state.chunks.is_empty());
    for index in 0..FINAL_CHUNK_COUNT {
        let mut objects = Vec::with_capacity(state.chunk_meshes.len());
        for &(mesh, material) in &state.chunk_meshes {
            objects.push(
                state
                    .renderer
                    .scene_mut()
                    .insert_object(ObjectDescriptor {
                        mesh,
                        material,
                        transform: Mat4::IDENTITY,
                        bounds: [0.0, 0.0, 0.0, 64.0],
                        flags: 0b11,
                        groups: GroupMask::NONE,
                        movability: Some(RECYCLED_CHUNK_MOVABILITY),
                        user_tag: index as u64,
                    })
                    .expect("insert recycled voxel chunk"),
            );
        }
        state.chunks.push(ChunkInstance {
            grid: [i32::MIN, i32::MIN],
            transform: Mat4::IDENTITY,
            center: Vec3::ZERO,
            objects,
        });
    }
    refresh_recycled_chunks(state);
    log::info!(
        "HelioV final stage: {FINAL_CHUNK_COUNT} instances ({} scene objects)",
        state
            .chunks
            .iter()
            .map(|chunk| chunk.objects.len())
            .sum::<usize>()
    );
}

struct BlockShowcase {
    cube_mesh: MeshId,
    materials: Vec<MaterialId>,
    names: Vec<String>,
    terrain_flower_material: MaterialId,
}

fn add_block_showcase(renderer: &mut Renderer) -> BlockShowcase {
    let color_image = image::load_from_memory(SHOWCASE_COLOR)
        .expect("embedded Brixel showcase color atlas is valid PNG")
        .into_rgba8();
    let (showcase_width, showcase_height) = color_image.dimensions();
    assert_eq!(showcase_width % SHOWCASE_TILE_SIZE, 0);
    assert_eq!(showcase_height % SHOWCASE_TILE_SIZE, 0);
    let showcase_columns = (showcase_width / SHOWCASE_TILE_SIZE) as usize;
    let showcase_rows = (showcase_height / SHOWCASE_TILE_SIZE) as usize;
    let showcase_blocks = SHOWCASE_BLOCK_COUNT
        .trim()
        .parse::<usize>()
        .expect("Brixel showcase count is a decimal integer");
    assert!(showcase_blocks <= showcase_columns * showcase_rows);
    let showcase_names = showcase_names();
    assert_eq!(showcase_names.len(), showcase_blocks);
    // Stop once each 64x64 tile reaches one texel. Continuing down based on
    // the long atlas dimension would blend neighboring block swatches.
    let showcase_mip_levels = SHOWCASE_TILE_SIZE.ilog2() + 1;

    let mut upload = |source, label, srgb| {
        let mut mips =
            embedded_texture_mips(source, showcase_width, showcase_height, showcase_mip_levels);
        let base = mips.remove(0);
        renderer
            .scene_mut()
            .insert_texture(
                TextureUpload::rgba8(
                    label,
                    showcase_width,
                    showcase_height,
                    srgb,
                    base,
                    TextureSamplerDesc {
                        address_mode_u: wgpu::AddressMode::ClampToEdge,
                        address_mode_v: wgpu::AddressMode::ClampToEdge,
                        address_mode_w: wgpu::AddressMode::ClampToEdge,
                        mag_filter: wgpu::FilterMode::Nearest,
                        min_filter: wgpu::FilterMode::Linear,
                        mipmap_filter: wgpu::MipmapFilterMode::Linear,
                    },
                )
                .with_mip_data(mips),
            )
            .expect("upload Brixel showcase atlas")
    };
    let color = upload(SHOWCASE_COLOR, "Brixel catalog showcase color", true);
    let normal = upload(SHOWCASE_NORMAL, "Brixel catalog showcase normal", false);
    let surface = upload(SHOWCASE_MATERIAL, "Brixel catalog showcase material", false);
    let cube = renderer
        .scene_mut()
        .insert_actor(SceneActor::mesh(box_mesh([0.0; 3], [0.5; 3])))
        .as_mesh()
        .expect("showcase cube mesh");
    let crossed_plant = renderer
        .scene_mut()
        .insert_actor(SceneActor::mesh(crossed_plant_mesh()))
        .as_mesh()
        .expect("showcase crossed-plant mesh");
    let mut crossed_plant_count = 0usize;
    let mut terrain_flower_material = None;
    let mut materials = Vec::with_capacity(showcase_blocks);

    // A material carries the per-tile UV transform, while every swatch shares
    // the same three resident atlas textures and the same cube mesh. This
    // keeps a 600+ entry catalog at three texture slots rather than consuming
    // one texture slot per block.
    for index in 0..showcase_blocks {
        let use_crossed_plant = is_crossed_plant(showcase_names[index]);
        crossed_plant_count += usize::from(use_crossed_plant);
        let column = index % showcase_columns;
        let row = index / showcase_columns;
        let transform = TextureTransform {
            offset: [
                column as f32 / showcase_columns as f32,
                row as f32 / showcase_rows as f32,
            ],
            scale: [1.0 / showcase_columns as f32, 1.0 / showcase_rows as f32],
            rotation_radians: 0.0,
        };
        let texture_ref = |texture| MaterialTextureRef {
            texture,
            uv_channel: VOXEL_ATLAS_REPEAT_UV,
            transform,
        };
        let mut textures = MaterialTextures::default();
        textures.base_color = Some(texture_ref(color));
        textures.normal = Some(texture_ref(normal));
        textures.roughness_metallic = Some(texture_ref(surface));
        let material = renderer
            .scene_mut()
            .insert_material_asset(MaterialAsset {
                gpu: GpuMaterial {
                    base_color: [1.0; 4],
                    emissive: [0.0; 4],
                    roughness_metallic: [1.0, 1.0, 1.5, 0.5],
                    tex_base_color: 0,
                    tex_normal: 0,
                    tex_roughness: 0,
                    tex_emissive: GpuMaterial::NO_TEXTURE,
                    tex_occlusion: GpuMaterial::NO_TEXTURE,
                    workflow: 0,
                    flags: libhelio::FLAG_HAS_NORMAL_MAP
                        | if use_crossed_plant {
                            libhelio::FLAG_ALPHA_TEST | libhelio::FLAG_DOUBLE_SIDED
                        } else {
                            0
                        },
                    material_class: 0,
                    class_params: [0.0; 4],
                },
                textures,
            })
            .expect("register Brixel showcase material");
        materials.push(material);
        if showcase_names[index] == "flower_dandelion" {
            terrain_flower_material = Some(material);
        }
        let position = Vec3::new(
            -27.5 + column as f32 * SHOWCASE_BLOCK_SPACING,
            30.5,
            -27.5 + row as f32 * SHOWCASE_BLOCK_SPACING,
        );
        renderer
            .scene_mut()
            .insert_object(ObjectDescriptor {
                mesh: if use_crossed_plant {
                    crossed_plant
                } else {
                    cube
                },
                material,
                transform: Mat4::from_translation(position),
                bounds: [position.x, position.y, position.z, 0.87],
                flags: 0b11,
                groups: GroupMask::NONE,
                movability: None,
                user_tag: u64::MAX - 1,
            })
            .expect("insert static Brixel showcase block");
    }
    log::info!(
        "Brixel showcase: {showcase_blocks} static cullable blocks in a {showcase_columns}x{showcase_rows} grid ({crossed_plant_count} crossed-quad plants)"
    );
    BlockShowcase {
        cube_mesh: cube,
        materials,
        names: showcase_names
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        terrain_flower_material: terrain_flower_material
            .expect("Brixel showcase contains flower_dandelion"),
    }
}

/// Register one permanent two-way portal pair in the source/origin chunk.
/// These objects deliberately live outside `chunks`, so the 8x8 terrain
/// stage does not clone the showcase across all 64 instances.
fn add_origin_portal_showcase(renderer: &mut Renderer) -> Vec<PortalId> {
    if !ENABLE_PORTAL_EXPERIMENT {
        return Vec::new();
    }
    let unit_box = renderer
        .scene_mut()
        .insert_actor(SceneActor::mesh(box_mesh([0.0; 3], [1.0; 3])))
        .as_mesh()
        .expect("portal frame mesh");
    let left_material = renderer.scene_mut().insert_material(make_material(
        [0.08, 0.65, 0.95, 1.0],
        0.3,
        0.1,
        [0.02, 0.45, 1.0],
        2.0,
    ));
    let right_material = renderer.scene_mut().insert_material(make_material(
        [1.0, 0.35, 0.08, 1.0],
        0.3,
        0.1,
        [1.0, 0.12, 0.01],
        2.0,
    ));
    let near_material = renderer.scene_mut().insert_material(make_material(
        [0.18, 0.95, 0.42, 1.0],
        0.3,
        0.1,
        [0.02, 1.0, 0.22],
        2.0,
    ));
    let far_material = renderer.scene_mut().insert_material(make_material(
        [0.72, 0.22, 1.0, 1.0],
        0.3,
        0.1,
        [0.55, 0.03, 1.0],
        2.0,
    ));

    for (center, forward, material) in [
        (
            Vec3::new(PORTAL_LEFT_X, PORTAL_CENTER_Y, PORTAL_CENTER_Z),
            Vec3::NEG_X,
            left_material,
        ),
        (
            Vec3::new(PORTAL_RIGHT_X, PORTAL_CENTER_Y, PORTAL_CENTER_Z),
            Vec3::X,
            right_material,
        ),
        (
            Vec3::new(32.0, PORTAL_CENTER_Y, PORTAL_NEAR_Z),
            Vec3::Z,
            near_material,
        ),
        (
            Vec3::new(32.0, PORTAL_CENTER_Y, PORTAL_FAR_Z),
            Vec3::NEG_Z,
            far_material,
        ),
    ] {
        let horizontal = Vec3::Y.cross(forward);
        let frame_rotation = glam::Quat::from_rotation_arc(Vec3::Z, horizontal);
        let pieces = [
            (
                Vec3::new(0.16, PORTAL_HALF_EXTENT.y + 0.28, 0.16),
                center - horizontal * (PORTAL_HALF_EXTENT.x + 0.16),
            ),
            (
                Vec3::new(0.16, PORTAL_HALF_EXTENT.y + 0.28, 0.16),
                center + horizontal * (PORTAL_HALF_EXTENT.x + 0.16),
            ),
            (
                Vec3::new(0.16, 0.16, PORTAL_HALF_EXTENT.x + 0.32),
                center + Vec3::Y * (PORTAL_HALF_EXTENT.y + 0.16),
            ),
            (
                Vec3::new(0.16, 0.16, PORTAL_HALF_EXTENT.x + 0.32),
                center - Vec3::Y * (PORTAL_HALF_EXTENT.y + 0.16),
            ),
        ];
        for (scale, position) in pieces {
            renderer
                .scene_mut()
                .insert_object(ObjectDescriptor {
                    mesh: unit_box,
                    material,
                    transform: Mat4::from_scale_rotation_translation(
                        scale,
                        frame_rotation,
                        position,
                    ),
                    bounds: [position.x, position.y, position.z, scale.length()],
                    flags: 0b11,
                    groups: GroupMask::NONE,
                    movability: None,
                    user_tag: u64::MAX,
                })
                .expect("insert portal frame");
        }
    }

    let left_to_right = renderer
        .scene_mut()
        .add_portal(PortalDescriptor {
            a: portal_pose_facing(
                Vec3::new(PORTAL_LEFT_X, PORTAL_CENTER_Y, PORTAL_CENTER_Z),
                Vec3::NEG_X,
                Vec3::Y,
            ),
            b: portal_pose_facing(
                Vec3::new(PORTAL_RIGHT_X, PORTAL_CENTER_Y, PORTAL_CENTER_Z),
                Vec3::NEG_X,
                Vec3::Y,
            ),
            half_extent: PORTAL_HALF_EXTENT,
        })
        .expect("add left origin portal");
    let right_to_left = renderer
        .scene_mut()
        .add_portal(PortalDescriptor {
            a: portal_pose_facing(
                Vec3::new(PORTAL_RIGHT_X, PORTAL_CENTER_Y, PORTAL_CENTER_Z),
                Vec3::X,
                Vec3::Y,
            ),
            b: portal_pose_facing(
                Vec3::new(PORTAL_LEFT_X, PORTAL_CENTER_Y, PORTAL_CENTER_Z),
                Vec3::X,
                Vec3::Y,
            ),
            half_extent: PORTAL_HALF_EXTENT,
        })
        .expect("add right origin portal");
    let near_to_far = renderer
        .scene_mut()
        .add_portal(PortalDescriptor {
            a: portal_pose_facing(
                Vec3::new(32.0, PORTAL_CENTER_Y, PORTAL_NEAR_Z),
                Vec3::Z,
                Vec3::Y,
            ),
            b: portal_pose_facing(
                Vec3::new(32.0, PORTAL_CENTER_Y, PORTAL_FAR_Z),
                Vec3::Z,
                Vec3::Y,
            ),
            half_extent: PORTAL_HALF_EXTENT,
        })
        .expect("add near origin portal");
    let far_to_near = renderer
        .scene_mut()
        .add_portal(PortalDescriptor {
            a: portal_pose_facing(
                Vec3::new(32.0, PORTAL_CENTER_Y, PORTAL_FAR_Z),
                Vec3::NEG_Z,
                Vec3::Y,
            ),
            b: portal_pose_facing(
                Vec3::new(32.0, PORTAL_CENTER_Y, PORTAL_NEAR_Z),
                Vec3::NEG_Z,
                Vec3::Y,
            ),
            half_extent: PORTAL_HALF_EXTENT,
        })
        .expect("add far origin portal");
    vec![left_to_right, right_to_left, near_to_far, far_to_near]
}

struct App {
    state: Option<AppState>,
}

struct AppState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface_format: wgpu::TextureFormat,
    renderer: Renderer,
    chunk_meshes: Vec<(MeshId, MaterialId)>,
    world: VoxelWorld,
    chunks: Vec<ChunkInstance>,
    build_cube_mesh: MeshId,
    build_materials: Vec<MaterialId>,
    build_material_names: Vec<String>,
    selected_build_material: usize,
    placed_cells: HashSet<[i32; 3]>,
    portal_ids: Vec<PortalId>,
    fps_overlay: FpsOverlay,
    status_overlay: StatusOverlay,
    block_preview: BlockPreviewOverlay,
    camera: FlyCamera,
    input: WinitFlyInput,
    world_axes_visible: bool,
    last_frame: Instant,
    /// Deliberately alarming: driven only by the renderer's structural graph
    /// replacement counter, never by ordinary window resize events.
    graph_rebuild_flash_until: Option<Instant>,
    observed_graph_rebuild_generation: u64,
    graph_rebuild_overlay: Arc<std::sync::Mutex<DebugOverlayState>>,
}

impl App {
    fn new() -> Self {
        Self { state: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("HelioV — Native Voxel Flycam")
                        .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
                )
                .expect("window"),
        );
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("GPU adapter");
        // Request the portable baseline only.  Opting into every optional
        // adapter capability makes a demo's behaviour driver-dependent.
        let required_features = wgpu::Features::INDIRECT_FIRST_INSTANCE;
        assert!(
            adapter.features().contains(required_features),
            "HelioV native baseline requires INDIRECT_FIRST_INSTANCE; adapter={:?}",
            adapter.get_info()
        );
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features,
            required_limits: required_wgpu_limits(adapter.limits()),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            ..Default::default()
        }))
        .expect("GPU device");
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .find(|format| format.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        surface.configure(
            &device,
            &surface_config(surface_format, caps.alpha_modes[0], size),
        );

        let mut config = RendererConfig::new(size.width, size.height, surface_format);
        // Keep this visual-reference demo at native resolution. The default
        // 0.75 render scale makes close voxel bevels pass through an upscale
        // before FXAA, obscuring whether an artifact is geometry or aliasing.
        config.render_scale = 1.0;
        // The default radiance-cascade GI occupies a camera-centred 3D box and
        // feeds sky-coloured miss rays back onto opaque geometry.  Its moving
        // volume boundary is especially obvious above the static block grid as
        // a rectangular blue/cloud-like wash.  This voxel baseline only needs
        // the stable hemisphere ambient plus direct light.
        config.gi_config = helio::GiConfig::ambient_only();
        // Enable the complete authored portal path: SceneDB pairs, projection
        // passes, portal instances, and CPU fly-camera crossings.
        config.enable_portals = ENABLE_PORTAL_EXPERIMENT;
        let mut scene = Scene::new(device.clone(), queue.clone());
        scene.insert_actor(SceneActor::sky(
            helio::SkyActor::new().with_sky_color([0.16, 0.30, 0.52]),
        ));
        let debug_camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Debug camera"),
            size: std::mem::size_of::<helio::DebugCameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cull_stats_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cull stats"),
            size: 32,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let debug_state = Arc::new(std::sync::Mutex::new(DebugDrawState::default()));
        let graph_rebuild_overlay = DebugOverlayState::new();
        {
            let mut overlay = graph_rebuild_overlay.lock().expect("rebuild overlay");
            overlay.populate = Some(Box::new(|overlay| {
                // This is intentionally not a subtle status chip. If a
                // structural change replaces the graph, the whole frame turns red.
                overlay.add_bar(0.0, 0.0, 65_536.0, 65_536.0, 0.95, 0.0, 0.0, 0.50);
                let label = "!!! FULL RENDER GRAPH REBUILD !!!";
                let column = overlay
                    .grid_cols()
                    .saturating_sub(label.chars().count() as u32)
                    / 2;
                let row = overlay.grid_rows() / 2;
                overlay.write_text(column, row, label);
                overlay.write_text(
                    column,
                    row.saturating_add(1),
                    "structural graph replacement",
                );
            }));
        }
        let graph = build_default_graph(
            &device,
            &queue,
            &scene,
            config,
            debug_state.clone(),
            &debug_camera_buf,
            &cull_stats_buf,
            Some(&graph_rebuild_overlay),
        );
        let mut renderer = Renderer::new(
            device.clone(),
            queue.clone(),
            config.surface_format,
            config.width,
            config.height,
            config.render_scale,
            config,
            scene,
            graph,
            debug_state,
            debug_camera_buf,
            cull_stats_buf,
        );
        // Neutral sky fill keeps faces turned away from the directional sun
        // readable without weakening its directional highlights and shadows.
        renderer.set_ambient([0.18, 0.20, 0.24], 1.0);
        // No editor grid. These three world-space lines render after the scene
        // with depth disabled, giving a small, always-readable orientation cue.
        add_world_axes(&mut renderer);
        let textures = VOXEL_TEXTURES.map(|source| {
            let mut mips = voxel_face_texture_mips(source);
            let base = mips.remove(0);
            renderer
                .scene_mut()
                .insert_texture(
                    TextureUpload::rgba8(
                        "HelioV Brixel color face atlas",
                        VOXEL_FACE_ATLAS_WIDTH,
                        VOXEL_FACE_TILE_SIZE,
                        true,
                        base,
                        TextureSamplerDesc {
                            address_mode_u: wgpu::AddressMode::ClampToEdge,
                            address_mode_v: wgpu::AddressMode::ClampToEdge,
                            address_mode_w: wgpu::AddressMode::ClampToEdge,
                            mag_filter: wgpu::FilterMode::Nearest,
                            min_filter: wgpu::FilterMode::Linear,
                            mipmap_filter: wgpu::MipmapFilterMode::Linear,
                        },
                    )
                    .with_mip_data(mips),
                )
                .expect("upload HelioV Brixel color atlas")
        });
        let normal_textures = VOXEL_NORMAL_TEXTURES.map(|source| {
            let mut mips = voxel_face_texture_mips(source);
            let base = mips.remove(0);
            renderer
                .scene_mut()
                .insert_texture(
                    TextureUpload::rgba8(
                        "HelioV Brixel normal face atlas",
                        VOXEL_FACE_ATLAS_WIDTH,
                        VOXEL_FACE_TILE_SIZE,
                        false,
                        base,
                        TextureSamplerDesc {
                            address_mode_u: wgpu::AddressMode::ClampToEdge,
                            address_mode_v: wgpu::AddressMode::ClampToEdge,
                            address_mode_w: wgpu::AddressMode::ClampToEdge,
                            mag_filter: wgpu::FilterMode::Nearest,
                            min_filter: wgpu::FilterMode::Linear,
                            mipmap_filter: wgpu::MipmapFilterMode::Linear,
                        },
                    )
                    .with_mip_data(mips),
                )
                .expect("upload HelioV Brixel normal atlas")
        });
        let material_textures = VOXEL_MATERIAL_TEXTURES.map(|source| {
            let mut mips = voxel_face_texture_mips(source);
            let base = mips.remove(0);
            renderer
                .scene_mut()
                .insert_texture(
                    TextureUpload::rgba8(
                        "HelioV Brixel roughness-metallic face atlas",
                        VOXEL_FACE_ATLAS_WIDTH,
                        VOXEL_FACE_TILE_SIZE,
                        false,
                        base,
                        TextureSamplerDesc {
                            address_mode_u: wgpu::AddressMode::ClampToEdge,
                            address_mode_v: wgpu::AddressMode::ClampToEdge,
                            address_mode_w: wgpu::AddressMode::ClampToEdge,
                            mag_filter: wgpu::FilterMode::Nearest,
                            min_filter: wgpu::FilterMode::Linear,
                            mipmap_filter: wgpu::MipmapFilterMode::Linear,
                        },
                    )
                    .with_mip_data(mips),
                )
                .expect("upload HelioV Brixel roughness-metallic atlas")
        });
        let roughness = [0.9, 1.0, 0.95, 0.85, 1.0, 0.9, 1.0, 0.95];
        let materials: [MaterialId; VOXEL_DRAW_MATERIALS] = std::array::from_fn(|index| {
            let block = index / VOXEL_FACE_SLOTS;
            let face = index % VOXEL_FACE_SLOTS;
            renderer
                .scene_mut()
                .insert_material_asset(material(
                    textures[block],
                    normal_textures[block],
                    material_textures[block],
                    face,
                    roughness[block],
                ))
                .expect("register HelioV Brixel color material")
        });
        let world = VoxelWorld::new();
        let meshes = world.meshes(ACTIVE_VOXEL_BEVEL_SEGMENTS);
        let triangle_count = meshes
            .iter()
            .map(|mesh| mesh.indices.len() / 3)
            .sum::<usize>();
        let mut sharp_meshes = Vec::new();
        for (mesh, material) in meshes.into_iter().zip(materials) {
            if mesh.indices.is_empty() {
                continue;
            }
            let mesh_id = renderer
                .scene_mut()
                .insert_actor(SceneActor::mesh(mesh))
                .as_mesh()
                .expect("mesh");
            sharp_meshes.push((mesh_id, material));
        }
        renderer.scene_mut().insert_actor(SceneActor::light(sun()));
        let showcase = add_block_showcase(&mut renderer);
        let flower_material = showcase.terrain_flower_material;
        let flower_mesh = terrain_flower_mesh(&world);
        let flower_count = flower_mesh.indices.len() / 12;
        if !flower_mesh.indices.is_empty() {
            let flower_mesh = renderer
                .scene_mut()
                .insert_actor(SceneActor::mesh(flower_mesh))
                .as_mesh()
                .expect("terrain flower mesh");
            sharp_meshes.push((flower_mesh, flower_material));
        }
        let portal_ids = add_origin_portal_showcase(&mut renderer);
        log::info!(
            "HelioV sharp face-culled geometry: {triangle_count} terrain triangles and {flower_count} flowers per chunk"
        );
        let observed_graph_rebuild_generation = renderer.graph_rebuild_generation();
        let fps_overlay = FpsOverlay::new(&device, &queue, surface_format);
        let status_overlay = StatusOverlay::new(&device, surface_format);
        let block_preview = BlockPreviewOverlay::new(&device, &queue, surface_format);
        let mut state = AppState {
            window,
            surface,
            device,
            queue,
            surface_format,
            renderer,
            chunk_meshes: sharp_meshes,
            world,
            chunks: Vec::new(),
            build_cube_mesh: showcase.cube_mesh,
            build_materials: showcase.materials,
            build_material_names: showcase.names,
            selected_build_material: 0,
            placed_cells: HashSet::new(),
            portal_ids,
            fps_overlay,
            status_overlay,
            block_preview,
            last_frame: Instant::now(),
            camera: FlyCamera::new(
                Vec3::new(0.0, 30.0, 45.0),
                0.0,
                -0.5,
                FlyCameraConfig::default(),
            ),
            input: WinitFlyInput::new(),
            world_axes_visible: true,
            graph_rebuild_flash_until: None,
            observed_graph_rebuild_generation,
            graph_rebuild_overlay,
        };
        add_final_chunk_stage(&mut state);
        update_build_selection_title(&state);
        self.state = Some(state);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        ..
                    },
                ..
            } => {
                if !state.input.release_cursor(&state.window) {
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        physical_key: PhysicalKey::Code(KeyCode::Tab),
                        repeat: false,
                        ..
                    },
                ..
            } => {
                state.world_axes_visible = !state.world_axes_visible;
                state.renderer.debug_clear();
                if state.world_axes_visible {
                    add_world_axes(&mut state.renderer);
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: key_state,
                        physical_key: PhysicalKey::Code(key),
                        ..
                    },
                ..
            } => {
                state.input.set_key(key, key_state == ElementState::Pressed);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if state.input.cursor_grabbed() {
                    place_selected_block(state);
                } else {
                    state.input.grab_cursor(&state.window);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.round() as i32,
                    MouseScrollDelta::PixelDelta(position) => position.y.signum() as i32,
                };
                if steps != 0 && !state.build_materials.is_empty() {
                    let count = state.build_materials.len() as i32;
                    state.selected_build_material =
                        (state.selected_build_material as i32 - steps).rem_euclid(count) as usize;
                    state
                        .block_preview
                        .select(&state.queue, state.selected_build_material);
                    update_build_selection_title(state);
                }
            }
            WindowEvent::Focused(focused) => state.input.set_window_focused(&state.window, focused),
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                state.surface.configure(
                    &state.device,
                    &surface_config(state.surface_format, wgpu::CompositeAlphaMode::Auto, size),
                );
                // Ordinary resize stays inside the current graph; no red alarm.
                state.renderer.set_render_size(size.width, size.height);
            }
            WindowEvent::RedrawRequested => render(state),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: winit::event::DeviceId, event: DeviceEvent) {
        if let (Some(state), DeviceEvent::MouseMotion { delta: (x, y) }) = (&mut self.state, event)
        {
            state.input.add_mouse_motion(x, y);
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

fn surface_config(
    format: wgpu::TextureFormat,
    alpha_mode: wgpu::CompositeAlphaMode,
    size: winit::dpi::PhysicalSize<u32>,
) -> wgpu::SurfaceConfiguration {
    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    }
}

fn render(state: &mut AppState) {
    let now = Instant::now();
    let dt = (now - state.last_frame).as_secs_f32();
    state.last_frame = now;
    let graph_rebuild_visible = state
        .graph_rebuild_flash_until
        .is_some_and(|deadline| now < deadline);
    state
        .graph_rebuild_overlay
        .lock()
        .expect("rebuild overlay")
        .enabled = graph_rebuild_visible;
    let previous_camera_position = state.camera.position();
    state.camera.update(state.input.take_input(), dt);
    let moved_camera_position = state.camera.position();
    let moved_camera_forward = state.camera.basis().forward;
    for portal_id in state.portal_ids.iter().copied() {
        let Some(pair) = state.renderer.scene().portal_pair(portal_id) else {
            continue;
        };
        if helio::crossing_detected(
            previous_camera_position,
            moved_camera_position,
            &pair.a,
            PORTAL_HALF_EXTENT,
        ) {
            let (position, forward) =
                pair.teleport_ray(moved_camera_position, moved_camera_forward);
            state.camera = FlyCamera::new(
                position,
                forward.x.atan2(-forward.z),
                forward.y.clamp(-1.0, 1.0).asin(),
                state.camera.config(),
            );
            break;
        }
    }
    refresh_recycled_chunks(state);
    refresh_build_outline(state);
    let size = state.window.inner_size();
    let camera = Camera::from_fly(
        &state.camera,
        size.width as f32 / size.height.max(1) as f32,
        PerspectiveLens {
            fov_y_radians: std::f32::consts::FRAC_PI_4,
            near: 0.01,
            far: 250.0,
        },
    );
    let output = match state.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(output)
        | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
        _ => return,
    };
    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    if let Err(error) = state.renderer.render(&camera, &view) {
        log::error!("render error: {error:?}");
    }
    state
        .fps_overlay
        .draw(&state.device, &state.queue, &view, size, dt);
    let status = status_lines(state, graph_rebuild_visible);
    state
        .status_overlay
        .draw(&state.device, &state.queue, &view, size, dt, &status);
    state.block_preview.tick(&state.queue, dt);
    state
        .block_preview
        .draw(&state.device, &state.queue, &view, size);
    let generation = state.renderer.graph_rebuild_generation();
    if generation != state.observed_graph_rebuild_generation {
        state.observed_graph_rebuild_generation = generation;
        state.graph_rebuild_flash_until = Some(Instant::now() + Duration::from_secs(2));
    }
    state.queue.present(output);
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    let event_loop = EventLoop::new().expect("event loop");
    event_loop
        .run_app(&mut App::new())
        .expect("event loop error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn quantized_position(position: [f32; 3]) -> [i32; 3] {
        position.map(|value| (value * 1_000_000.0).round() as i32)
    }

    #[test]
    fn showcase_crosses_only_obvious_cutout_plants() {
        let names = showcase_names();
        assert_eq!(names.len(), 622);
        assert!(is_crossed_plant("tallgrass"));
        assert!(is_crossed_plant("flower_dandelion"));
        assert!(is_crossed_plant("sapling_oak"));
        assert!(!is_crossed_plant("grass_top"));
        assert!(!is_crossed_plant("grass_side"));
        assert!(!is_crossed_plant("mushroom_block_skin_red"));

        let mesh = crossed_plant_mesh();
        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.indices.len() / 3, 4);
    }

    #[test]
    fn voxel_face_winding_matches_declared_outward_normal() {
        for face in FACES {
            let a = Vec3::from_array(face.corners[0]);
            let b = Vec3::from_array(face.corners[1]);
            let c = Vec3::from_array(face.corners[2]);
            let emitted_normal = (b - a).cross(c - a).normalize();
            assert!(
                emitted_normal.dot(Vec3::from_array(face.normal)) > 0.999,
                "face {:?} has inward winding",
                face.neighbor
            );
        }
    }

    #[test]
    fn upstream_terrain_generation_keeps_its_material_layers() {
        let world = VoxelWorld::new();
        assert!(world.blocks.contains(&Block::Air));
        assert!(world.blocks.contains(&Block::Grass));
        assert!(world.blocks.contains(&Block::Dirt));
        assert!(world.blocks.contains(&Block::Stone));
        assert!(world.blocks.contains(&Block::Ore));
    }

    #[test]
    fn repeated_chunk_meshing_removes_shell_but_keeps_real_boundary_gaps() {
        let mut world = VoxelWorld::empty();
        world.set(0, 0, 7, Block::Stone);
        world.set(WORLD_SIDE - 1, 0, 7, Block::Stone);

        // The repeated neighbor across -X is solid and the allocation below
        // Y=0 continues as solid, so neither creates an artificial wall.
        assert!(!world.meshing_neighbor_is_air(-1, 0, 7));
        assert!(!world.meshing_neighbor_is_air(0, -1, 7));

        // A genuine empty cell across a wrapped boundary must still expose the
        // adjacent voxel face, otherwise the repeated chunks develop a hole.
        assert!(world.meshing_neighbor_is_air(-1, 0, 8));
        assert!(world.meshing_neighbor_is_air(0, WORLD_HEIGHT, 7));
    }

    #[test]
    fn top_surface_voxel_also_emits_one_bottom_quad() {
        let mut world = VoxelWorld::empty();
        world.set(0, 0, 0, Block::Stone);
        world.set(0, 1, 0, Block::Stone);

        let meshes = world.meshes(0);
        let stone_bottom = &meshes[Block::Stone.material_index().unwrap() * 3 + 2];
        assert_eq!(stone_bottom.indices.len(), 6);
    }

    #[test]
    fn beveled_meshing_culls_fully_internal_voxels() {
        let mut world = VoxelWorld::empty();
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    world.set(x, y, z, Block::Grass);
                }
            }
        }
        let meshes = world.meshes(VOXEL_BEVEL_SEGMENTS);
        // All eight cells touch the surface, but their six internal center
        // faces are absent. Every emitted primitive is an indexed triangle.
        let triangle_count = meshes
            .iter()
            .map(|mesh| mesh.indices.len() / 3)
            .sum::<usize>();
        assert!(triangle_count < 8 * 156);
        assert!(meshes.iter().all(|mesh| mesh.indices.len() % 3 == 0));
    }

    #[test]
    fn isolated_three_band_bevel_has_expected_topology() {
        let mut world = VoxelWorld::empty();
        world.set(1, 1, 1, Block::Grass);
        let meshes = world.meshes(VOXEL_BEVEL_SEGMENTS);
        // 6 center quads (12), 12 * 3 edge quads (72), and eight
        // three-arc corner fans (8 * 9 = 72).
        assert_eq!(
            meshes
                .iter()
                .map(|mesh| mesh.indices.len() / 3)
                .sum::<usize>(),
            156
        );

        // Material/normal seams duplicate vertices, so validate the manifold
        // after welding equal positions. Every geometric edge of this closed
        // isolated block must belong to exactly two triangles.
        let mut edge_counts = BTreeMap::new();
        for mesh in &meshes {
            for triangle in mesh.indices.chunks_exact(3) {
                let points = [
                    quantized_position(mesh.vertices[triangle[0] as usize].position),
                    quantized_position(mesh.vertices[triangle[1] as usize].position),
                    quantized_position(mesh.vertices[triangle[2] as usize].position),
                ];
                assert_ne!(points[0], points[1]);
                assert_ne!(points[1], points[2]);
                assert_ne!(points[2], points[0]);
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    let edge = if points[a] < points[b] {
                        (points[a], points[b])
                    } else {
                        (points[b], points[a])
                    };
                    *edge_counts.entry(edge).or_insert(0usize) += 1;
                }
            }
        }
        assert!(edge_counts.values().all(|&count| count == 2));
    }

    #[test]
    fn adjacent_beveled_voxels_omit_both_shared_faces() {
        let mut world = VoxelWorld::empty();
        world.set(1, 1, 1, Block::Grass);
        world.set(2, 1, 1, Block::Grass);
        let meshes = world.meshes(VOXEL_BEVEL_SEGMENTS);
        let triangle_count = meshes
            .iter()
            .map(|mesh| mesh.indices.len() / 3)
            .sum::<usize>();
        // Two isolated voxels cost 312 triangles. The two orientations of
        // their shared face are now culled (two triangles per orientation).
        assert_eq!(triangle_count, 308);
    }

    #[test]
    fn geometry_lods_reduce_isolated_block_cost_monotonically() {
        let mut world = VoxelWorld::empty();
        world.set(1, 1, 1, Block::Grass);
        let counts = [3, 2, 1, 0].map(|segments| {
            world
                .meshes(segments)
                .iter()
                .map(|mesh| mesh.indices.len() / 3)
                .sum::<usize>()
        });
        assert_eq!(counts, [156, 108, 60, 12]);
    }

    #[test]
    fn recycled_window_maps_each_grid_cell_to_one_stable_slot() {
        let mut slots = std::collections::BTreeSet::new();
        for z in -3..5 {
            for x in -3..5 {
                slots.insert(recycled_chunk_slot(x, z));
            }
        }
        assert_eq!(slots.len(), FINAL_CHUNK_COUNT);
        assert_eq!(RECYCLED_CHUNK_MOVABILITY, helio::Movability::Movable);
    }

    #[test]
    fn brixel_face_atlases_have_three_mipped_faces_per_block_type() {
        for source in VOXEL_TEXTURES {
            let mips = voxel_face_texture_mips(source);
            assert_eq!(
                mips[0].len(),
                (VOXEL_FACE_ATLAS_WIDTH * VOXEL_FACE_TILE_SIZE * 4) as usize
            );
            assert_eq!(mips.len(), VOXEL_FACE_MIP_LEVELS as usize);
        }
    }

    #[test]
    fn brixel_showcase_contract_covers_the_full_catalog_without_padding_objects() {
        let image = image::load_from_memory(SHOWCASE_COLOR)
            .expect("showcase color PNG")
            .into_rgba8();
        let (width, height) = image.dimensions();
        assert_eq!(width % SHOWCASE_TILE_SIZE, 0);
        assert_eq!(height % SHOWCASE_TILE_SIZE, 0);
        let atlas_cells = (width / SHOWCASE_TILE_SIZE * height / SHOWCASE_TILE_SIZE) as usize;
        let block_count = SHOWCASE_BLOCK_COUNT.trim().parse::<usize>().unwrap();
        assert!(block_count > 600);
        assert!(block_count <= atlas_cells);
    }
}
