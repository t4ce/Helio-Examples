#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
CXX=${CXX:-clang++}
CLANG=${CLANG:-clang}
BUILD_DIR=${TMPDIR:-/tmp}/cloud-engine-verify

printf '%s\n' '[1/6] JavaScript syntax'
node --check "$ROOT/app.js"

printf '%s\n' '[2/6] Python helper syntax'
python3 -m py_compile "$ROOT/serve.py" "$ROOT/tests/smoke_test.py" "$ROOT/tests/config_io_test.py"

printf '%s\n' '[3/6] TRUEOS C++ for OpenCL simulation syntax'
"$CLANG" -x clcpp -cl-std=CLC++ -fsyntax-only "$ROOT/native/trueos/cloud_simulation.clcpp"

printf '%s\n' '[4/6] TRUEOS C++ for OpenCL renderer syntax'
"$CLANG" -x clcpp -cl-std=CLC++ -fsyntax-only "$ROOT/native/trueos/cloud_render.clcpp"

printf '%s\n' '[5/6] Historical OpenCL C image-reference syntax'
"$CLANG" -x cl -cl-std=CL2.0 -fsyntax-only "$ROOT/native/reference_opencl_c/cloud_simulation_image3d.cl"
"$CLANG" -x cl -cl-std=CL2.0 -fsyntax-only "$ROOT/native/reference_opencl_c/cloud_render_image3d.cl"

printf '%s\n' '[6/6] C++ ABI + dispatch contract'
mkdir -p "$BUILD_DIR"
"$CXX" -std=c++20 -Wall -Wextra -Wpedantic -Werror \
  "$ROOT/native/abi_check.cpp" -o "$BUILD_DIR/cloud_abi_check"
"$BUILD_DIR/cloud_abi_check"

printf '%s\n' 'PASS: static verification complete.'
