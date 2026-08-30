# Helio Churn TrueOS

`helio_churn_trueos` is the TRUEOS Blueprint port of Helio's Churn benchmark.
It keeps the Churn scene identity and controls while replacing Helio's hosted
WGPU/window stack with TRUEOS UI4 and the VMX vGPU boundary.

From Shell2 apps mode:

```text
start helio_churn_trueos
```

Controls: WASD/Space/Shift move, primary-drag looks, Control boosts, `C`
toggles the collision burst, and `+`/`-` changes the spawn rate. Escape uses
the normal UI4 close policy and terminates this Blueprint instance.

Upstream source and attribution: [Far-Beyond-Pulsar/Helio](https://github.com/Far-Beyond-Pulsar/Helio),
particularly `benchmarks/churn_benchmark.rs`. The Helio MIT license is carried
in `LICENSE-HELIO`.
