#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
CXX=${CXX:-clang++}
CLANG=${CLANG:-clang}
BUILD_DIR=${TMPDIR:-/tmp}/cloud-engine-verify

printf '%s\n' '[1/5] JavaScript syntax'
node --check "$ROOT/app.js"

printf '%s\n' '[2/5] Python helper syntax'
python3 -m py_compile "$ROOT/serve.py" "$ROOT/tests/smoke_test.py" "$ROOT/tests/config_io_test.py"

printf '%s\n' '[3/5] OpenCL simulation syntax'
"$CLANG" -x cl -cl-std=CL2.0 -fsyntax-only "$ROOT/native/cloud_simulation.cl"

printf '%s\n' '[4/5] OpenCL renderer syntax'
"$CLANG" -x cl -cl-std=CL2.0 -fsyntax-only "$ROOT/native/cloud_render.cl"

printf '%s\n' '[5/5] C++ ABI contract'
mkdir -p "$BUILD_DIR"
"$CXX" -std=c++20 -Wall -Wextra -Wpedantic -Werror \
  "$ROOT/native/abi_check.cpp" -o "$BUILD_DIR/cloud_abi_check"
"$BUILD_DIR/cloud_abi_check"

printf '%s\n' 'PASS: static verification complete.'
