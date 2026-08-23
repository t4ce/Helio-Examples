# Native compute handoff

This folder contains a deliberately small native contract for the browser cloud engine.

## Files

- `cloud_params.hpp` — C++20 POD layouts matching both shader backends.
- `cloud_simulation.cl` — OpenCL C compute twin of the WebGPU simulation.
- `cloud_render.cl` — OpenCL C compute version of the browser ray marcher.
- `abi_check.cpp` — compile-time and runtime layout checks.
- `CMakeLists.txt` — builds the ABI checker only; it does not assume a specific Intel runtime install.

## Quick local validation

```bash
clang -x cl -cl-std=CL2.0 -fsyntax-only cloud_simulation.cl
clang -x cl -cl-std=CL2.0 -fsyntax-only cloud_render.cl

cmake -S . -B build
cmake --build build
./build/cloud_abi_check
```

Expected ABI output:

```text
SimParams: 112 bytes
RenderParams: 272 bytes
Simulation dispatch: 24 x 12 x 24
```

## Kernel entry points

```text
cloud_simulation
cloud_render
```

Both kernels use required workgroup-size attributes. The native renderer writes one output pixel per work-item, so it does not need a graphics pipeline or scene geometry.

See the project-level [`PORTING.md`](../PORTING.md) for SPIR-V, IGC, Level Zero, and OpenCL integration notes.
