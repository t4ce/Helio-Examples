# Re-run the Linux browser demo

The visible WebGPU demo is intentionally unchanged by the TRUEOS alignment work.
That means you can use the same Linux/Chrome test as before and compare the same
cloud look, controls, JSON presets, flycam, drawing, and performance.

```bash
cd cloud-engine-webgpu-linux-aligned
python3 serve.py --open
```

Open `http://localhost:8080/` in a WebGPU-capable browser.

Recommended parity test:

1. Load the same exported JSON preset you used before.
2. Select the same quality tier (for example the 64-step/low-cost tier).
3. Confirm animation, pause, drawing, flycam, and still rendering are unchanged.
4. Export the JSON again if you want the exact authored state for the TRUEOS port.

The important change is not the browser image. It is the native handoff under
`native/trueos/`: C++ for OpenCL, SIMD16 `16x1x1`, flattened persistent volume
buffers, linear RGBA8 output, and a two-stage dispatch plan matching the TRUEOS
Shell2 C++ lineage.

Run the static native checks with:

```bash
./verify.sh
```
