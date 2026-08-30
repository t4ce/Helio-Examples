# Helio Portal TrueOS

`helio_portal_trueos` is the TRUEOS Blueprint port of Helio's Portal Rooms
demo. Its complete texture-free room, portal clipping, furniture, and overlay
engine lives in this folder. TRUEOS supplies UI4 and the VMX vGPU presentation
boundary; the upstream Helio engine is not linked into the Blueprint.

From Shell2 apps mode:

```text
start helio_portal_trueos
```

Controls: WASD/Space/Shift move, primary-drag looks, Control boosts, and Tab
toggles the portal overlay. Escape uses the normal UI4 close policy and
terminates this Blueprint instance.

Upstream source and attribution: [Far-Beyond-Pulsar/Helio](https://github.com/Far-Beyond-Pulsar/Helio),
particularly `portals/portal_rooms.rs`. The Helio MIT license is carried in
`LICENSE-HELIO`.
