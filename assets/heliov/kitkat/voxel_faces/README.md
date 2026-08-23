# HelioV Block Bits face layout

This directory is the complete design-time texture input for the first HelioV
voxel palette. It contains exactly eight RGBA PNG files:

| Material index | File | KayKit source model |
|---:|---|---|
| 0 | `grass.png` | `dirt_with_grass.gltf` |
| 1 | `dirt.png` | `dirt.gltf` |
| 2 | `stone.png` | `stone.gltf` |
| 3 | `gold_ore.png` | `stone_with_gold.gltf` |
| 4 | `sand.png` | `sand_A.gltf` |
| 5 | `bricks.png` | `bricks_A.gltf` |
| 6 | `gravel.png` | `gravel.gltf` |
| 7 | `snow.png` | `snow.gltf` |

Every file is **192x64 RGBA**. The only layout is:

```text
x =   0..63   top
x =  64..127  side (used for all four vertical faces)
x = 128..191  bottom
```

The regions are stored exactly as the existing Helio cube UV contract expects.
In particular, the side region is design-time flipped vertically because cube
side UV `v=0` is at the geometric bottom. Runtime code must not flip it again.

These files are baked assets, not a request for runtime model loading. The
voxel renderer continues to draw ordinary one-unit cube faces. It may select a
material and one of the three regions above; it must not reconstruct KayKit
geometry, interpret the shared 1024x1024 source texture, or change meshing.

Regenerate the complete fixed set from the repository root with:

```sh
blender --background --factory-startup --python \
  crates/examples/assets/heliov/kitkat/extract_voxel_faces.py
```

Source: KayKit Block Bits 1.0 by Kay Lousberg, provided under CC0. See the
adjacent `License.txt`.
