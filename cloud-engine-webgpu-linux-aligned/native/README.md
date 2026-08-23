# Native handoff: TRUEOS Shell2 C++ shape

This directory now distinguishes two things that were previously mixed together:

1. the **browser-math reference**, and
2. the **native execution contract we actually want for TRUEOS**.

The Linux browser demo is unchanged visually. The native side is now shaped for
the same C++ for OpenCL + IGC + direct-RCS lineage used by TRUEOS Shell2 `cpp`.

## Canonical native files

- `trueos/cloud_simulation.clcpp` — flattened SIMD16 simulation over a persistent linear `half4` volume.
- `trueos/cloud_render.clcpp` — SIMD16 row renderer into a packed linear RGBA8 destination.
- `cloud_params.hpp` — fixed 16-byte-aligned parameter ABI shared with the browser model.
- `cloud_dispatch_plan.hpp` — the intended two-walker frame graph and exact dispatch geometry.
- `abi_check.cpp` — compile-time/runtime ABI and dispatch assertions.

The old image-object OpenCL C twins are retained under `reference_opencl_c/` only
as a readable mathematical cross-check. They are **not** the preferred TRUEOS
port target anymore.

## Why this changed

TRUEOS's current Shell2 C++ demo path already does the important work we need:
reviewed AOT C++ for OpenCL artifacts, generated IGC contracts, PPGTT mapping,
SIMD16 payload setup, a 2D GPGPU walker, completion markers, and UI4 publication.
The right move is therefore to extend that path to a two-stage retained cloud
workload rather than model the port around a generic OpenCL image API or the
narrow ShaderToy ABI.

## Static validation

```bash
clang -x clcpp -cl-std=CLC++ -fsyntax-only trueos/cloud_simulation.clcpp
clang -x clcpp -cl-std=CLC++ -fsyntax-only trueos/cloud_render.clcpp

cmake -S . -B build
cmake --build build
./build/cloud_abi_check
```

Expected key output:

```text
SimParams: 112 bytes
RenderParams: 272 bytes
TRUEOS simulation groups: 27648 x 1 x 1 (local 16x1x1)
TRUEOS 1920x1080 render groups: 120 x 1080 x 1 (local 16x1x1)
Ping-pong volume bytes: 7077888
```

See `trueos/README.md` and the project-level `PORTING.md` for the frame graph and
bake/runtime boundary.
