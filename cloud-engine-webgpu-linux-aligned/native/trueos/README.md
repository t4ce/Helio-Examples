# TRUEOS-aligned native kernels

These are the native compatibility target for the prototype.

The browser still renders with WebGPU/WGSL and therefore remains the easiest
Linux visual demo. The files in this directory are shaped around the actual
TRUEOS Shell2 C++ path instead of around a generic OpenCL image API.

## Contract

- Canonical native source language: **C++ for OpenCL** (`.clcpp`).
- AOT path: C++ for OpenCL -> LLVM/SPIR-V -> Intel IGC/ocloc -> reviewed Zebin.
- Required subgroup: SIMD16.
- Required local size: `16 x 1 x 1` for both kernels.
- Simulation volume: two persistent linear `half4` buffers, `96 x 48 x 96`.
- Simulation dispatch: one flattened voxel index per lane, `27,648 x 1 x 1` groups.
- Render dispatch: `ceil(width / 16) x height x 1` groups.
- Render output: packed linear RGBA8 (`0xAABBGGRR` in a little-endian `uint`).
- Parameters: fixed POD `SimParams` (112 B) and `RenderParams` (272 B).

The first compatibility contract uses software trilinear volume sampling. That
is deliberate: it lets TRUEOS reach mathematical/visual parity using the buffer
binding path it already has. Hardware 3D sampler support can replace that helper
later without changing the JSON config format, simulation model, or frame graph.

## Intended frame batch

```text
persistent volume A/B
        |
        v
cloud_simulation.clcpp     SIMD16 flattened voxel walker
        |
        | GPU producer -> consumer dependency
        v
cloud_render.clcpp         SIMD16 2D pixel walker
        |
        v
one final completion marker -> UI4 publish
```

The CPU should not poll between simulation and rendering.

## Static syntax check

```bash
clang -x clcpp -cl-std=CLC++ -fsyntax-only cloud_simulation.clcpp
clang -x clcpp -cl-std=CLC++ -fsyntax-only cloud_render.clcpp
```
