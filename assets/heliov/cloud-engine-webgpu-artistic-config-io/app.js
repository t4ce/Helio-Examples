(() => {
  "use strict";

  const $ = (selector) => document.querySelector(selector);
  const canvas = $("#cloudCanvas");
  const panel = $("#controlPanel");
  const gpuStatus = $("#gpuStatus");
  const fpsReadout = $("#fpsReadout");
  const unsupported = $("#unsupported");
  const unsupportedMessage = $("#unsupportedMessage");
  const brushCursor = $("#brushCursor");
  const toastElement = $("#toast");

  const VOLUME_SIZE = [96, 48, 96];
  // The front/lower faces were expanded so every ray in the initial camera view
  // reaches the editable volume. This also makes the brush useful after modest
  // fly-camera movement instead of silently reusing its last valid position.
  const BOUNDS_MIN = [-8.0, -2.0, -1.0];
  const BOUNDS_MAX = [8.0, 9.0, 18.0];
  const WORLD_SIZE = BOUNDS_MAX.map((value, axis) => value - BOUNDS_MIN[axis]);
  const WORLD_UP = [0.0, 1.0, 0.0];
  const INITIAL_CAMERA = {
    position: [0.0, 1.65, -4.6],
    yaw: 0.0,
    pitch: Math.atan(0.085),
    fov: 54,
  };
  const SIMULATION_INTERVAL = 1 / 30;
  const ENGINE_VERSION = "2.1-config-io";
  const CONFIG_FORMAT = "cloud-engine-config";
  const CONFIG_VERSION = 1;
  const MAX_CONFIG_BYTES = 1024 * 1024;
  const MOVEMENT_CODES = new Set([
    "KeyW", "KeyA", "KeyS", "KeyD", "KeyQ", "KeyE",
    "ShiftLeft", "ShiftRight",
  ]);

  const QUALITY = {
    eco: { steps: 48, tier: 0, detail: 0.58, pixelRatioCap: 0.9 },
    live: { steps: 68, tier: 1, detail: 0.78, pixelRatioCap: 1.2 },
    still: { steps: 104, tier: 2, detail: 0.96, pixelRatioCap: 1.5 },
  };

  const PATTERN_NAMES = Object.freeze([
    "Cumulus field",
    "Layered deck",
    "High wisps",
    "Storm towers",
    "Broken cells",
    "Moon scrolls",
    "Wind ribbons",
  ]);

  const ART_PRESETS = {
    verdant: {
      artCloudColor: "#3dad91",
      artShadowColor: "#075653",
      artSkyColor: "#073f42",
      artMoonColor: "#ffe875",
      artCurl: 1.10,
      artRibbon: 0.72,
      artSculpt: 0.78,
      artBands: 5,
      artOutline: 0.62,
      artMoonSize: 0.18,
      artMoonGlow: 1.15,
      artGrain: 0.20,
    },
    porcelain: {
      artCloudColor: "#72b8c9",
      artShadowColor: "#173c58",
      artSkyColor: "#0b2545",
      artMoonColor: "#fff0a7",
      artCurl: 1.28,
      artRibbon: 0.86,
      artSculpt: 0.84,
      artBands: 6,
      artOutline: 0.76,
      artMoonSize: 0.155,
      artMoonGlow: 0.92,
      artGrain: 0.13,
    },
    ember: {
      artCloudColor: "#d98b58",
      artShadowColor: "#5b2937",
      artSkyColor: "#2d1935",
      artMoonColor: "#ffd46c",
      artCurl: 0.96,
      artRibbon: 0.64,
      artSculpt: 0.72,
      artBands: 4,
      artOutline: 0.54,
      artMoonSize: 0.21,
      artMoonGlow: 1.52,
      artGrain: 0.28,
    },
    violet: {
      artCloudColor: "#8976c6",
      artShadowColor: "#30294f",
      artSkyColor: "#17182f",
      artMoonColor: "#f6dc91",
      artCurl: 1.42,
      artRibbon: 0.94,
      artSculpt: 0.82,
      artBands: 5,
      artOutline: 0.88,
      artMoonSize: 0.145,
      artMoonGlow: 1.05,
      artGrain: 0.24,
    },
  };

  const state = {
    mode: "auto",
    pattern: 0,
    amount: 0.58,
    windSpeed: 0.55,
    windDirection: 18,
    turbulence: 0.55,
    tearing: 0.35,
    rotation: 0.12,
    dissipation: 0.08,

    brushSize: 0.09,
    brushStrength: 0.72,
    brushDepth: 0.42,
    brushCenter: [0.5, 0.45, 0.28],
    brushWorldPosition: [0, 0, 0],
    brushActive: false,
    brushValid: false,
    brushSign: 1,
    lastPointer: [0, 0],

    renderStyle: "realistic",
    artModeEntered: false,
    artPreset: "verdant",
    artCloudColor: "#3dad91",
    artShadowColor: "#075653",
    artSkyColor: "#073f42",
    artMoonColor: "#ffe875",
    artCurl: 1.10,
    artRibbon: 0.72,
    artSculpt: 0.78,
    artBands: 5,
    artOutline: 0.62,
    artMoonSize: 0.18,
    artMoonGlow: 1.15,
    artGrain: 0.20,

    sunColor: "#fff0d2",
    sunElevation: 27,
    sunAzimuth: 22,
    sunIntensity: 1.15,
    exposure: 0.95,
    quality: "live",

    cameraPosition: [...INITIAL_CAMERA.position],
    cameraYaw: INITIAL_CAMERA.yaw,
    cameraPitch: INITIAL_CAMERA.pitch,
    cameraFov: INITIAL_CAMERA.fov,
    cameraSpeed: 4.0,
    cameraSensitivity: 0.12,
    pointerLocked: false,

    paused: false,
    seed: 19.37,
    time: 0,
    jitterFrame: 0,
    pendingClear: true,
    capturePending: false,
  };

  const gpu = {
    adapter: null,
    device: null,
    context: null,
    format: null,
    computePipeline: null,
    renderPipeline: null,
    simUniformBuffer: null,
    renderUniformBuffer: null,
    volumes: [],
    volumeViews: [],
    sampler: null,
    simBindGroups: [],
    renderBindGroups: [],
    currentVolume: 0,
    ready: false,
  };

  // Seven vec4 blocks for SimParams and seventeen for RenderParams.
  const simUniformData = new Float32Array(28);
  const renderUniformData = new Float32Array(68);
  const cameraKeys = new Set();

  let animationHandle = 0;
  let previousFrameTime = performance.now();
  let simulationAccumulator = 0;
  let fpsAccumulator = 0;
  let fpsFrames = 0;
  let toastTimer = 0;
  let captureInFlight = false;
  let paintPointerId = null;
  let applyingArtPreset = false;
  let lastCameraReadout = 0;

  function normalize3(vector) {
    const length = Math.hypot(vector[0], vector[1], vector[2]) || 1;
    return [vector[0] / length, vector[1] / length, vector[2] / length];
  }

  function cross3(a, b) {
    return [
      a[1] * b[2] - a[2] * b[1],
      a[2] * b[0] - a[0] * b[2],
      a[0] * b[1] - a[1] * b[0],
    ];
  }

  function dot3(a, b) {
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  }

  function addScaled3(target, vector, scale) {
    target[0] += vector[0] * scale;
    target[1] += vector[1] * scale;
    target[2] += vector[2] * scale;
  }

  function clamp(value, minimum, maximum) {
    return Math.min(maximum, Math.max(minimum, value));
  }

  function mix(a, b, amount) {
    return a + (b - a) * amount;
  }

  function getCameraBasis() {
    const cosPitch = Math.cos(state.cameraPitch);
    const forward = normalize3([
      Math.sin(state.cameraYaw) * cosPitch,
      Math.sin(state.cameraPitch),
      Math.cos(state.cameraYaw) * cosPitch,
    ]);
    const right = normalize3(cross3(WORLD_UP, forward));
    const up = normalize3(cross3(forward, right));
    return { forward, right, up };
  }

  function getTanHalfFov() {
    return Math.tan((state.cameraFov * Math.PI / 180) * 0.5);
  }

  function hexToLinearRgb(hex) {
    const normalized = hex.replace("#", "");
    const numeric = Number.parseInt(normalized, 16);
    const srgb = [
      ((numeric >> 16) & 255) / 255,
      ((numeric >> 8) & 255) / 255,
      (numeric & 255) / 255,
    ];
    return srgb.map((channel) => (
      channel <= 0.04045
        ? channel / 12.92
        : ((channel + 0.055) / 1.055) ** 2.4
    ));
  }

  function showToast(message, duration = 2200) {
    window.clearTimeout(toastTimer);
    toastElement.textContent = message;
    toastElement.classList.add("is-visible");
    toastTimer = window.setTimeout(() => {
      toastElement.classList.remove("is-visible");
    }, duration);
  }

  function showUnsupported(message) {
    gpu.ready = false;
    gpuStatus.textContent = "Unavailable";
    gpuStatus.className = "status-dot status-dot--error";
    unsupportedMessage.textContent = message;
    unsupported.hidden = false;
    if (animationHandle) {
      cancelAnimationFrame(animationHandle);
      animationHandle = 0;
    }
  }

  async function fetchShader(path) {
    const response = await fetch(path, { cache: "no-store" });
    if (!response.ok) {
      throw new Error(`Could not load ${path} (${response.status}).`);
    }
    return response.text();
  }

  async function validateShaderModule(module, label) {
    if (typeof module.getCompilationInfo !== "function") {
      return;
    }
    const info = await module.getCompilationInfo();
    const errors = info.messages.filter((message) => message.type === "error");
    for (const message of info.messages) {
      const method = message.type === "error" ? "error" : "warn";
      console[method](`[${label}] ${message.lineNum}:${message.linePos} ${message.message}`);
    }
    if (errors.length > 0) {
      const first = errors[0];
      throw new Error(`${label} shader error at ${first.lineNum}:${first.linePos}: ${first.message}`);
    }
  }

  async function initializeWebGPU() {
    if (!window.isSecureContext) {
      showUnsupported("WebGPU needs a secure context. Serve this folder from localhost instead of opening index.html directly.");
      return;
    }

    if (!navigator.gpu) {
      showUnsupported("This browser does not expose WebGPU. Open the project in a current WebGPU-capable desktop browser.");
      return;
    }

    try {
      gpu.adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
      if (!gpu.adapter) {
        throw new Error("No compatible GPU adapter was returned by the browser.");
      }

      gpu.device = await gpu.adapter.requestDevice();
      gpu.context = canvas.getContext("webgpu");
      if (!gpu.context) {
        throw new Error("The canvas could not create a WebGPU context.");
      }

      gpu.format = navigator.gpu.getPreferredCanvasFormat();
      configureCanvasContext();

      const [simulationSource, renderSource] = await Promise.all([
        fetchShader("shaders/simulate.wgsl"),
        fetchShader("shaders/render.wgsl"),
      ]);

      const simulationModule = gpu.device.createShaderModule({
        label: "Cloud simulation WGSL",
        code: simulationSource,
      });
      const renderModule = gpu.device.createShaderModule({
        label: "Cloud ray marcher WGSL",
        code: renderSource,
      });

      await Promise.all([
        validateShaderModule(simulationModule, "simulation"),
        validateShaderModule(renderModule, "render"),
      ]);

      [gpu.computePipeline, gpu.renderPipeline] = await Promise.all([
        gpu.device.createComputePipelineAsync({
          label: "Cloud volume simulation pipeline",
          layout: "auto",
          compute: {
            module: simulationModule,
            entryPoint: "main",
          },
        }),
        gpu.device.createRenderPipelineAsync({
          label: "Cloud fullscreen ray-march pipeline",
          layout: "auto",
          vertex: {
            module: renderModule,
            entryPoint: "vs_main",
          },
          fragment: {
            module: renderModule,
            entryPoint: "fs_main",
            targets: [{ format: gpu.format }],
          },
          primitive: {
            topology: "triangle-list",
            cullMode: "none",
          },
        }),
      ]);

      createGpuResources();

      gpu.device.lost.then((info) => {
        const reason = info.message || info.reason || "Unknown device loss";
        showUnsupported(`The GPU device was lost: ${reason}`);
      });

      gpu.device.addEventListener?.("uncapturederror", (event) => {
        console.error("Uncaptured WebGPU error:", event.error);
        showToast(`GPU validation error: ${event.error?.message || "see console"}`, 5000);
      });

      gpu.ready = true;
      gpuStatus.textContent = "WebGPU ready";
      gpuStatus.className = "status-dot status-dot--ready";
      unsupported.hidden = true;
      previousFrameTime = performance.now();
      animationHandle = requestAnimationFrame(renderFrame);
    } catch (error) {
      console.error(error);
      const localHint = location.protocol === "file:"
        ? " Serve this folder from localhost; shader files cannot be fetched from file://."
        : "";
      const baseMessage = String(error.message || error).replace(/\.+$/, "");
      showUnsupported(`${baseMessage}.${localHint}`);
    }
  }

  function configureCanvasContext() {
    if (!gpu.context || !gpu.device || !gpu.format) {
      return;
    }
    gpu.context.configure({
      device: gpu.device,
      format: gpu.format,
      alphaMode: "opaque",
    });
  }

  function createGpuResources() {
    gpu.simUniformBuffer = gpu.device.createBuffer({
      label: "Simulation uniform buffer",
      size: simUniformData.byteLength,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    gpu.renderUniformBuffer = gpu.device.createBuffer({
      label: "Render uniform buffer",
      size: renderUniformData.byteLength,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    const volumeDescriptor = {
      label: "Cloud density volume",
      size: {
        width: VOLUME_SIZE[0],
        height: VOLUME_SIZE[1],
        depthOrArrayLayers: VOLUME_SIZE[2],
      },
      dimension: "3d",
      format: "rgba16float",
      mipLevelCount: 1,
      sampleCount: 1,
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.STORAGE_BINDING,
    };

    gpu.volumes = [
      gpu.device.createTexture({ ...volumeDescriptor, label: "Cloud volume A" }),
      gpu.device.createTexture({ ...volumeDescriptor, label: "Cloud volume B" }),
    ];
    gpu.volumeViews = gpu.volumes.map((texture) => texture.createView({ dimension: "3d" }));

    gpu.sampler = gpu.device.createSampler({
      label: "Cloud trilinear sampler",
      addressModeU: "repeat",
      addressModeV: "clamp-to-edge",
      addressModeW: "repeat",
      magFilter: "linear",
      minFilter: "linear",
      mipmapFilter: "nearest",
    });

    const computeLayout = gpu.computePipeline.getBindGroupLayout(0);
    gpu.simBindGroups = [
      gpu.device.createBindGroup({
        label: "Simulate A into B",
        layout: computeLayout,
        entries: [
          { binding: 0, resource: { buffer: gpu.simUniformBuffer } },
          { binding: 1, resource: gpu.volumeViews[0] },
          { binding: 2, resource: gpu.sampler },
          { binding: 3, resource: gpu.volumeViews[1] },
        ],
      }),
      gpu.device.createBindGroup({
        label: "Simulate B into A",
        layout: computeLayout,
        entries: [
          { binding: 0, resource: { buffer: gpu.simUniformBuffer } },
          { binding: 1, resource: gpu.volumeViews[1] },
          { binding: 2, resource: gpu.sampler },
          { binding: 3, resource: gpu.volumeViews[0] },
        ],
      }),
    ];

    const renderLayout = gpu.renderPipeline.getBindGroupLayout(0);
    gpu.renderBindGroups = gpu.volumeViews.map((view, index) => gpu.device.createBindGroup({
      label: `Render cloud volume ${index === 0 ? "A" : "B"}`,
      layout: renderLayout,
      entries: [
        { binding: 0, resource: { buffer: gpu.renderUniformBuffer } },
        { binding: 1, resource: view },
        { binding: 2, resource: gpu.sampler },
      ],
    }));
  }

  function resizeCanvasIfNeeded() {
    const rect = canvas.getBoundingClientRect();
    const quality = QUALITY[state.quality];
    const requestedRatio = Math.min(window.devicePixelRatio || 1, quality.pixelRatioCap);
    let targetWidth = Math.max(2, Math.floor(rect.width * requestedRatio));
    let targetHeight = Math.max(2, Math.floor(rect.height * requestedRatio));

    const maxDimension = state.quality === "still" ? 2880 : 2200;
    const dimensionScale = Math.min(1, maxDimension / Math.max(targetWidth, targetHeight));
    targetWidth = Math.max(2, Math.floor(targetWidth * dimensionScale));
    targetHeight = Math.max(2, Math.floor(targetHeight * dimensionScale));

    if (canvas.width !== targetWidth || canvas.height !== targetHeight) {
      canvas.width = targetWidth;
      canvas.height = targetHeight;
      configureCanvasContext();
    }
  }

  function updateSimulationUniforms(deltaTime, clearPulse) {
    const windRadians = state.windDirection * Math.PI / 180;
    const wind = [
      Math.sin(windRadians) * state.windSpeed,
      0,
      Math.cos(windRadians) * state.windSpeed,
    ];

    simUniformData.set([
      deltaTime,
      state.time,
      state.amount,
      state.mode === "auto" ? 1 : 0,

      wind[0],
      wind[1],
      wind[2],
      state.turbulence,

      state.brushCenter[0],
      state.brushCenter[1],
      state.brushCenter[2],
      state.brushSize,

      state.brushStrength,
      state.brushActive ? 1 : 0,
      state.brushSign,
      state.pattern,

      state.dissipation,
      state.tearing,
      state.rotation,
      clearPulse ? 1 : 0,

      VOLUME_SIZE[0],
      VOLUME_SIZE[1],
      VOLUME_SIZE[2],
      state.seed,

      state.renderStyle === "artistic" ? 1 : 0,
      state.artCurl,
      state.artRibbon,
      state.artSculpt,
    ]);

    gpu.device.queue.writeBuffer(gpu.simUniformBuffer, 0, simUniformData);
  }

  function updateRenderUniforms() {
    const quality = QUALITY[state.quality];
    const elevation = state.sunElevation * Math.PI / 180;
    const azimuth = state.sunAzimuth * Math.PI / 180;
    const cosElevation = Math.cos(elevation);
    const sunDirection = normalize3([
      Math.sin(azimuth) * cosElevation,
      Math.sin(elevation),
      Math.cos(azimuth) * cosElevation,
    ]);
    const sunColor = hexToLinearRgb(state.sunColor);

    const sunsetFactor = clamp((20 - state.sunElevation) / 18, 0, 1);
    const skyTop = [
      mix(0.16, 0.07, sunsetFactor),
      mix(0.39, 0.16, sunsetFactor),
      mix(0.78, 0.42, sunsetFactor),
    ];
    const neutralHorizon = [0.55, 0.73, 0.86];
    const warmHorizon = [
      Math.max(0.52, sunColor[0] * 1.12),
      Math.max(0.28, sunColor[1] * 0.72),
      Math.max(0.18, sunColor[2] * 0.42),
    ];
    const skyHorizon = neutralHorizon.map((channel, index) => (
      mix(channel, warmHorizon[index], sunsetFactor * 0.52)
    ));

    const camera = getCameraBasis();
    const artCloudColor = hexToLinearRgb(state.artCloudColor);
    const artShadowColor = hexToLinearRgb(state.artShadowColor);
    const artSkyColor = hexToLinearRgb(state.artSkyColor);
    const artMoonColor = hexToLinearRgb(state.artMoonColor);

    renderUniformData.set([
      canvas.width,
      canvas.height,
      state.time,
      state.jitterFrame,

      state.cameraPosition[0],
      state.cameraPosition[1],
      state.cameraPosition[2],
      getTanHalfFov(),

      camera.forward[0],
      camera.forward[1],
      camera.forward[2],
      state.exposure,

      camera.right[0],
      camera.right[1],
      camera.right[2],
      state.renderStyle === "artistic"
        ? Math.max(24, Math.round(quality.steps * 0.68))
        : quality.steps,

      camera.up[0],
      camera.up[1],
      camera.up[2],
      quality.detail,

      sunDirection[0],
      sunDirection[1],
      sunDirection[2],
      state.sunIntensity,

      sunColor[0],
      sunColor[1],
      sunColor[2],
      1.45,

      skyTop[0],
      skyTop[1],
      skyTop[2],
      0.27,

      skyHorizon[0],
      skyHorizon[1],
      skyHorizon[2],
      state.seed,

      BOUNDS_MIN[0],
      BOUNDS_MIN[1],
      BOUNDS_MIN[2],
      1.32,

      BOUNDS_MAX[0],
      BOUNDS_MAX[1],
      BOUNDS_MAX[2],
      1.48,

      state.amount,
      quality.tier,
      state.paused ? 1 : 0,
      0,

      state.renderStyle === "artistic" ? 1 : 0,
      state.artBands,
      state.artOutline,
      state.artSculpt,

      artCloudColor[0],
      artCloudColor[1],
      artCloudColor[2],
      state.artGrain,

      artShadowColor[0],
      artShadowColor[1],
      artShadowColor[2],
      state.artRibbon,

      artSkyColor[0],
      artSkyColor[1],
      artSkyColor[2],
      state.artMoonSize,

      artMoonColor[0],
      artMoonColor[1],
      artMoonColor[2],
      state.artMoonGlow,
    ]);

    gpu.device.queue.writeBuffer(gpu.renderUniformBuffer, 0, renderUniformData);
  }

  function updateFps(deltaTime) {
    fpsAccumulator += deltaTime;
    fpsFrames += 1;
    if (fpsAccumulator >= 0.5) {
      const fps = Math.round(fpsFrames / fpsAccumulator);
      fpsReadout.textContent = `${fps} fps`;
      fpsAccumulator = 0;
      fpsFrames = 0;
    }
  }

  function updateCamera(deltaTime) {
    const basis = getCameraBasis();
    const movement = [0, 0, 0];

    if (cameraKeys.has("KeyW")) addScaled3(movement, basis.forward, 1);
    if (cameraKeys.has("KeyS")) addScaled3(movement, basis.forward, -1);
    if (cameraKeys.has("KeyD")) addScaled3(movement, basis.right, 1);
    if (cameraKeys.has("KeyA")) addScaled3(movement, basis.right, -1);
    if (cameraKeys.has("KeyE")) addScaled3(movement, WORLD_UP, 1);
    if (cameraKeys.has("KeyQ")) addScaled3(movement, WORLD_UP, -1);

    const movementLength = Math.hypot(movement[0], movement[1], movement[2]);
    if (movementLength <= 0.00001) {
      return false;
    }

    const boost = cameraKeys.has("ShiftLeft") || cameraKeys.has("ShiftRight") ? 3.5 : 1;
    const distance = state.cameraSpeed * boost * deltaTime;
    const normalizedMovement = movement.map((component) => component / movementLength);
    addScaled3(state.cameraPosition, normalizedMovement, distance);
    updateCameraReadout();
    return true;
  }

  function updateCameraReadout(force = false) {
    const now = performance.now();
    if (!force && now - lastCameraReadout < 80) {
      return;
    }
    lastCameraReadout = now;
    $("#cameraReadout").textContent = state.cameraPosition
      .map((value) => value.toFixed(1))
      .join(" · ");
  }

  function renderFrame(timestamp) {
    if (!gpu.ready) {
      return;
    }

    const frameDelta = Math.min(Math.max((timestamp - previousFrameTime) / 1000, 0), 0.1);
    previousFrameTime = timestamp;
    updateFps(frameDelta);
    resizeCanvasIfNeeded();

    const cameraMoved = updateCamera(frameDelta);
    if (!state.paused) {
      state.time += frameDelta;
      simulationAccumulator += frameDelta;
    }
    if (!state.paused || cameraMoved) {
      state.jitterFrame += 1;
    }

    let runSimulation = false;
    let simulationDelta = 0;
    const clearPulse = state.pendingClear;

    if (clearPulse || state.brushActive) {
      runSimulation = true;
      simulationDelta = state.paused ? 0 : Math.min(simulationAccumulator, 1 / 15);
      simulationAccumulator = 0;
    } else if (!state.paused && simulationAccumulator >= SIMULATION_INTERVAL) {
      runSimulation = true;
      simulationDelta = Math.min(simulationAccumulator, 1 / 15);
      simulationAccumulator = 0;
    }

    updateRenderUniforms();

    const commandEncoder = gpu.device.createCommandEncoder({ label: "Cloud frame encoder" });
    let renderVolume = gpu.currentVolume;

    if (runSimulation) {
      updateSimulationUniforms(simulationDelta, clearPulse);
      const computePass = commandEncoder.beginComputePass({ label: "Cloud volume update" });
      computePass.setPipeline(gpu.computePipeline);
      computePass.setBindGroup(0, gpu.simBindGroups[gpu.currentVolume]);
      computePass.dispatchWorkgroups(
        Math.ceil(VOLUME_SIZE[0] / 4),
        Math.ceil(VOLUME_SIZE[1] / 4),
        Math.ceil(VOLUME_SIZE[2] / 4),
      );
      computePass.end();
      renderVolume = 1 - gpu.currentVolume;
      state.pendingClear = false;
    }

    const renderPass = commandEncoder.beginRenderPass({
      label: "Cloud fullscreen render",
      colorAttachments: [{
        view: gpu.context.getCurrentTexture().createView(),
        clearValue: { r: 0.02, g: 0.05, b: 0.09, a: 1 },
        loadOp: "clear",
        storeOp: "store",
      }],
    });
    renderPass.setPipeline(gpu.renderPipeline);
    renderPass.setBindGroup(0, gpu.renderBindGroups[renderVolume]);
    renderPass.draw(3, 1, 0, 0);
    renderPass.end();

    gpu.device.queue.submit([commandEncoder.finish()]);
    gpu.currentVolume = renderVolume;

    if (state.capturePending && !captureInFlight) {
      state.capturePending = false;
      captureInFlight = true;
      void captureCurrentFrame();
    }

    animationHandle = requestAnimationFrame(renderFrame);
  }

  async function captureCurrentFrame() {
    try {
      await gpu.device.queue.onSubmittedWorkDone();
      const blob = await new Promise((resolve, reject) => {
        canvas.toBlob((result) => {
          if (result) {
            resolve(result);
          } else {
            reject(new Error("The browser could not encode the canvas."));
          }
        }, "image/png");
      });

      const timestamp = new Date().toISOString().replaceAll(":", "-").replace("T", "_").slice(0, 19);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `cloud-engine_${timestamp}.png`;
      anchor.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 2000);
      showToast(`Captured ${canvas.width} × ${canvas.height} PNG`);
    } catch (error) {
      console.error(error);
      showToast(`Capture failed: ${error.message || error}`, 4000);
    } finally {
      captureInFlight = false;
    }
  }

  function setMode(mode, announce = true) {
    state.mode = mode;
    const autoActive = mode === "auto";
    $("#modeAuto").classList.toggle("is-active", autoActive);
    $("#modeAuto").setAttribute("aria-pressed", String(autoActive));
    $("#modeDraw").classList.toggle("is-active", !autoActive);
    $("#modeDraw").setAttribute("aria-pressed", String(!autoActive));
    $("#drawControls").classList.toggle("is-visible", !autoActive);
    $("#drawControls").setAttribute("aria-hidden", String(autoActive));
    $("#formationHint").textContent = autoActive ? "continuous generation" : "manual volume · air stays live";
    document.body.classList.toggle("is-draw-mode", !autoActive);

    if (autoActive) {
      state.brushActive = false;
      state.brushValid = false;
      paintPointerId = null;
      brushCursor.classList.remove("is-visible", "is-erasing", "is-invalid");
    } else {
      if (state.pointerLocked) {
        document.exitPointerLock?.();
      }
      if (announce) {
        showToast("Draw mode: drag to add, Shift/right-click erases, wheel changes depth.", 3600);
      }
    }
  }

  function setRenderStyle(style, announce = true) {
    state.renderStyle = style;
    $("#styleSelect").value = style;
    const artistic = style === "artistic";
    $("#artControls").classList.toggle("is-visible", artistic);
    $("#artControls").setAttribute("aria-hidden", String(!artistic));
    document.body.classList.toggle("is-artistic", artistic);

    if (artistic && !state.artModeEntered) {
      state.artModeEntered = true;
      if (state.mode === "auto") {
        state.pattern = 5;
        $("#patternSelect").value = "5";
        state.pendingClear = true;
      }
    }

    if (announce) {
      showToast(artistic
        ? "Artistic mode enabled: sculpted palette lighting, ink edges, curl flow, and a procedural moon."
        : "Realistic volumetric lighting restored.", 3800);
    }
  }

  function togglePause(forceValue, announce = true) {
    state.paused = typeof forceValue === "boolean" ? forceValue : !state.paused;
    simulationAccumulator = 0;
    const button = $("#pauseButton");
    button.classList.toggle("is-paused", state.paused);
    button.querySelector("span").textContent = state.paused ? "Resume" : "Pause";
    if (announce) {
      showToast(state.paused ? "Scene paused — the brush and fly camera still work." : "Simulation resumed.");
    }
  }

  function clearVolume() {
    state.pendingClear = true;
    if (state.mode === "auto") {
      showToast("Formation reset from a clean procedural source.");
    } else {
      showToast("Manual cloud volume cleared.");
    }
  }

  function reseedVolume() {
    state.seed = 1 + Math.random() * 97;
    state.pendingClear = true;
    showToast("New procedural seed applied.");
  }

  function resetCamera() {
    state.cameraPosition = [...INITIAL_CAMERA.position];
    state.cameraYaw = INITIAL_CAMERA.yaw;
    state.cameraPitch = INITIAL_CAMERA.pitch;
    state.cameraFov = INITIAL_CAMERA.fov;
    const fovInput = $("#cameraFov");
    fovInput.value = String(INITIAL_CAMERA.fov);
    $("#cameraFovValue").textContent = `${INITIAL_CAMERA.fov}°`;
    setRangeProgress(fovInput);
    updateCameraReadout(true);
    showToast("Camera reset.");
  }

  async function enterFlycam() {
    if (document.pointerLockElement === canvas) {
      return;
    }
    if (typeof canvas.requestPointerLock !== "function") {
      showToast("Pointer lock is not available in this browser.", 3800);
      return;
    }

    state.brushActive = false;
    paintPointerId = null;
    try {
      const result = canvas.requestPointerLock({ unadjustedMovement: true });
      if (result && typeof result.then === "function") {
        await result;
      }
    } catch (firstError) {
      try {
        const fallback = canvas.requestPointerLock();
        if (fallback && typeof fallback.then === "function") {
          await fallback;
        }
      } catch (fallbackError) {
        console.warn("Pointer lock failed", firstError, fallbackError);
        showToast("Flycam could not capture the mouse. Try the F key or the button again.", 4000);
      }
    }
  }

  function toggleFlycam() {
    if (document.pointerLockElement === canvas) {
      document.exitPointerLock?.();
    } else {
      void enterFlycam();
    }
  }

  function handlePointerLockChange() {
    state.pointerLocked = document.pointerLockElement === canvas;
    document.body.classList.toggle("is-fly-mode", state.pointerLocked);
    $("#flyHud").setAttribute("aria-hidden", String(!state.pointerLocked));
    const button = $("#flyButton");
    button.classList.toggle("is-active", state.pointerLocked);
    button.querySelector("span").textContent = state.pointerLocked ? "Exit flycam" : "Enter flycam";
    state.brushActive = false;
    paintPointerId = null;
    brushCursor.classList.remove("is-visible");
    if (state.pointerLocked) {
      showToast("Flycam active — WASD moves, Q/E moves vertically, Shift boosts, Esc releases.", 3600);
    }
  }

  function setRangeProgress(input) {
    const minimum = Number(input.min || 0);
    const maximum = Number(input.max || 100);
    const value = Number(input.value);
    const progress = ((value - minimum) / Math.max(maximum - minimum, Number.EPSILON)) * 100;
    input.style.setProperty("--range-progress", `${progress}%`);
  }

  function bindRange(id, stateKey, formatter, onUserUpdate = null) {
    const input = $(`#${id}`);
    const output = $(`#${id}Value`);
    let initialized = false;
    const update = () => {
      state[stateKey] = Number(input.value);
      output.textContent = formatter(state[stateKey]);
      setRangeProgress(input);
      if (stateKey === "brushSize") {
        updateBrushCursorSize();
      }
      if (initialized && onUserUpdate) {
        onUserUpdate();
      }
      initialized = true;
    };
    input.addEventListener("input", update);
    update();
  }

  function bindColor(id, stateKey, markCustom = false) {
    const input = $(`#${id}`);
    const output = $(`#${id}Value`);
    let initialized = false;
    const update = () => {
      state[stateKey] = input.value;
      output.textContent = input.value.toUpperCase();
      if (initialized && markCustom) {
        markArtCustom();
      }
      initialized = true;
    };
    input.addEventListener("input", update);
    update();
  }

  function markArtCustom() {
    if (applyingArtPreset) {
      return;
    }
    state.artPreset = "custom";
    $("#artPreset").value = "custom";
  }

  function applyArtPreset(name) {
    if (!ART_PRESETS[name]) {
      state.artPreset = "custom";
      return;
    }

    applyingArtPreset = true;
    const preset = ART_PRESETS[name];
    for (const [stateKey, value] of Object.entries(preset)) {
      state[stateKey] = value;
      const input = $(`#${stateKey}`);
      if (!input) {
        continue;
      }
      input.value = String(value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
    applyingArtPreset = false;
    state.artPreset = name;
    $("#artPreset").value = name;
    showToast(`${$("#artPreset").selectedOptions[0].text} palette applied.`);
  }

  function isRecord(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
  }

  function recordOrEmpty(value) {
    return isRecord(value) ? value : {};
  }

  function numberOr(value, fallback) {
    if (value === null || value === "" || typeof value === "boolean") {
      return fallback;
    }
    const numeric = Number(value);
    return Number.isFinite(numeric) ? numeric : fallback;
  }

  function booleanOr(value, fallback) {
    return typeof value === "boolean" ? value : fallback;
  }

  function enumOr(value, allowed, fallback) {
    return typeof value === "string" && allowed.includes(value) ? value : fallback;
  }

  function colorOr(value, fallback) {
    return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value)
      ? value.toLowerCase()
      : fallback;
  }

  function vector3Or(value, fallback) {
    if (!Array.isArray(value) || value.length < 3) {
      return [...fallback];
    }
    const result = value.slice(0, 3).map((component, index) => numberOr(component, fallback[index]));
    return result.every(Number.isFinite) ? result : [...fallback];
  }

  function createConfigurationDocument() {
    return {
      format: CONFIG_FORMAT,
      version: CONFIG_VERSION,
      engineVersion: ENGINE_VERSION,
      exportedAt: new Date().toISOString(),
      configuration: {
        mode: state.mode,
        formation: {
          pattern: state.pattern,
          patternName: PATTERN_NAMES[state.pattern] || `Pattern ${state.pattern}`,
          amount: state.amount,
          seed: state.seed,
        },
        look: {
          renderStyle: state.renderStyle,
          artPreset: state.artPreset,
          artCloudColor: state.artCloudColor,
          artShadowColor: state.artShadowColor,
          artSkyColor: state.artSkyColor,
          artMoonColor: state.artMoonColor,
          artCurl: state.artCurl,
          artRibbon: state.artRibbon,
          artSculpt: state.artSculpt,
          artBands: state.artBands,
          artOutline: state.artOutline,
          artMoonSize: state.artMoonSize,
          artMoonGlow: state.artMoonGlow,
          artGrain: state.artGrain,
        },
        air: {
          windSpeed: state.windSpeed,
          windDirection: state.windDirection,
          turbulence: state.turbulence,
          tearing: state.tearing,
          rotation: state.rotation,
          dissipation: state.dissipation,
        },
        brush: {
          brushSize: state.brushSize,
          brushStrength: state.brushStrength,
          brushDepth: state.brushDepth,
        },
        camera: {
          cameraPosition: [...state.cameraPosition],
          cameraYaw: state.cameraYaw,
          cameraPitch: state.cameraPitch,
          cameraFov: state.cameraFov,
          cameraSpeed: state.cameraSpeed,
          cameraSensitivity: state.cameraSensitivity,
        },
        sunAndRender: {
          sunColor: state.sunColor,
          sunElevation: state.sunElevation,
          sunAzimuth: state.sunAzimuth,
          sunIntensity: state.sunIntensity,
          exposure: state.exposure,
          quality: state.quality,
        },
        playback: {
          paused: state.paused,
        },
      },
    };
  }

  function normalizeConfigurationDocument(configDocument) {
    if (!isRecord(configDocument)) {
      throw new Error("The selected file does not contain a JSON object.");
    }
    if (configDocument.format !== CONFIG_FORMAT) {
      throw new Error(`Expected format \"${CONFIG_FORMAT}\".`);
    }

    const version = Number(configDocument.version);
    if (!Number.isInteger(version) || version !== CONFIG_VERSION) {
      throw new Error(`Configuration version ${String(configDocument.version)} is not supported by this build.`);
    }

    const configuration = recordOrEmpty(configDocument.configuration);
    if (Object.keys(configuration).length === 0) {
      throw new Error("The configuration object is missing or empty.");
    }

    const formation = recordOrEmpty(configuration.formation);
    const look = recordOrEmpty(configuration.look);
    const air = recordOrEmpty(configuration.air);
    const brush = recordOrEmpty(configuration.brush);
    const camera = recordOrEmpty(configuration.camera);
    const sunAndRender = recordOrEmpty(configuration.sunAndRender);
    const playback = recordOrEmpty(configuration.playback);
    const presetNames = [...Object.keys(ART_PRESETS), "custom"];

    return {
      mode: enumOr(configuration.mode, ["auto", "draw"], state.mode),
      pattern: clamp(Math.round(numberOr(formation.pattern, state.pattern)), 0, PATTERN_NAMES.length - 1),
      amount: numberOr(formation.amount, state.amount),
      seed: clamp(numberOr(formation.seed, state.seed), -1000000, 1000000),

      renderStyle: enumOr(look.renderStyle, ["realistic", "artistic"], state.renderStyle),
      artPreset: enumOr(look.artPreset, presetNames, "custom"),
      artCloudColor: colorOr(look.artCloudColor, state.artCloudColor),
      artShadowColor: colorOr(look.artShadowColor, state.artShadowColor),
      artSkyColor: colorOr(look.artSkyColor, state.artSkyColor),
      artMoonColor: colorOr(look.artMoonColor, state.artMoonColor),
      artCurl: numberOr(look.artCurl, state.artCurl),
      artRibbon: numberOr(look.artRibbon, state.artRibbon),
      artSculpt: numberOr(look.artSculpt, state.artSculpt),
      artBands: numberOr(look.artBands, state.artBands),
      artOutline: numberOr(look.artOutline, state.artOutline),
      artMoonSize: numberOr(look.artMoonSize, state.artMoonSize),
      artMoonGlow: numberOr(look.artMoonGlow, state.artMoonGlow),
      artGrain: numberOr(look.artGrain, state.artGrain),

      windSpeed: numberOr(air.windSpeed, state.windSpeed),
      windDirection: numberOr(air.windDirection, state.windDirection),
      turbulence: numberOr(air.turbulence, state.turbulence),
      tearing: numberOr(air.tearing, state.tearing),
      rotation: numberOr(air.rotation, state.rotation),
      dissipation: numberOr(air.dissipation, state.dissipation),

      brushSize: numberOr(brush.brushSize, state.brushSize),
      brushStrength: numberOr(brush.brushStrength, state.brushStrength),
      brushDepth: numberOr(brush.brushDepth, state.brushDepth),

      cameraPosition: vector3Or(camera.cameraPosition, state.cameraPosition)
        .map((component) => clamp(component, -10000, 10000)),
      cameraYaw: clamp(numberOr(camera.cameraYaw, state.cameraYaw), -1000000, 1000000),
      cameraPitch: clamp(numberOr(camera.cameraPitch, state.cameraPitch), -Math.PI * 0.494, Math.PI * 0.494),
      cameraFov: numberOr(camera.cameraFov, state.cameraFov),
      cameraSpeed: numberOr(camera.cameraSpeed, state.cameraSpeed),
      cameraSensitivity: numberOr(camera.cameraSensitivity, state.cameraSensitivity),

      sunColor: colorOr(sunAndRender.sunColor, state.sunColor),
      sunElevation: numberOr(sunAndRender.sunElevation, state.sunElevation),
      sunAzimuth: numberOr(sunAndRender.sunAzimuth, state.sunAzimuth),
      sunIntensity: numberOr(sunAndRender.sunIntensity, state.sunIntensity),
      exposure: numberOr(sunAndRender.exposure, state.exposure),
      quality: enumOr(sunAndRender.quality, Object.keys(QUALITY), state.quality),
      paused: booleanOr(playback.paused, state.paused),
    };
  }

  function setRangeControl(id, value) {
    const input = $(`#${id}`);
    if (!(input instanceof HTMLInputElement)) {
      return;
    }
    const minimum = Number(input.min || -Infinity);
    const maximum = Number(input.max || Infinity);
    input.value = String(clamp(numberOr(value, Number(input.value)), minimum, maximum));
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }

  function setColorControl(id, value) {
    const input = $(`#${id}`);
    if (!(input instanceof HTMLInputElement)) {
      return;
    }
    input.value = colorOr(value, input.value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }

  function applyConfigurationDocument(configDocument, sourceLabel = "configuration") {
    const loaded = normalizeConfigurationDocument(configDocument);

    if (document.pointerLockElement === canvas) {
      document.exitPointerLock?.();
    }
    cameraKeys.clear();
    paintPointerId = null;
    state.brushActive = false;
    state.brushValid = false;
    brushCursor.classList.remove("is-visible", "is-erasing", "is-invalid");

    state.pattern = loaded.pattern;
    $("#patternSelect").value = String(loaded.pattern);
    setRangeControl("cloudAmount", loaded.amount);
    state.seed = loaded.seed;

    setMode(loaded.mode, false);
    if (loaded.renderStyle === "artistic") {
      state.artModeEntered = true;
    }
    setRenderStyle(loaded.renderStyle, false);

    applyingArtPreset = true;
    setColorControl("artCloudColor", loaded.artCloudColor);
    setColorControl("artShadowColor", loaded.artShadowColor);
    setColorControl("artSkyColor", loaded.artSkyColor);
    setColorControl("artMoonColor", loaded.artMoonColor);
    setRangeControl("artCurl", loaded.artCurl);
    setRangeControl("artRibbon", loaded.artRibbon);
    setRangeControl("artSculpt", loaded.artSculpt);
    setRangeControl("artBands", loaded.artBands);
    setRangeControl("artOutline", loaded.artOutline);
    setRangeControl("artMoonSize", loaded.artMoonSize);
    setRangeControl("artMoonGlow", loaded.artMoonGlow);
    setRangeControl("artGrain", loaded.artGrain);
    applyingArtPreset = false;
    state.artPreset = loaded.artPreset;
    $("#artPreset").value = loaded.artPreset;

    setRangeControl("windSpeed", loaded.windSpeed);
    setRangeControl("windDirection", loaded.windDirection);
    setRangeControl("turbulence", loaded.turbulence);
    setRangeControl("tearing", loaded.tearing);
    setRangeControl("rotation", loaded.rotation);
    setRangeControl("dissipation", loaded.dissipation);

    setRangeControl("brushSize", loaded.brushSize);
    setRangeControl("brushStrength", loaded.brushStrength);
    setRangeControl("brushDepth", loaded.brushDepth);

    state.cameraPosition = [...loaded.cameraPosition];
    state.cameraYaw = loaded.cameraYaw;
    state.cameraPitch = loaded.cameraPitch;
    setRangeControl("cameraFov", loaded.cameraFov);
    setRangeControl("cameraSpeed", loaded.cameraSpeed);
    setRangeControl("cameraSensitivity", loaded.cameraSensitivity);

    setColorControl("sunColor", loaded.sunColor);
    setRangeControl("sunElevation", loaded.sunElevation);
    setRangeControl("sunAzimuth", loaded.sunAzimuth);
    setRangeControl("sunIntensity", loaded.sunIntensity);
    setRangeControl("exposure", loaded.exposure);
    state.quality = loaded.quality;
    $("#qualitySelect").value = loaded.quality;
    togglePause(loaded.paused, false);

    state.time = 0;
    state.jitterFrame = 0;
    simulationAccumulator = 0;
    state.pendingClear = true;
    resizeCanvasIfNeeded();
    updateBrushCursorSize();
    updateCameraReadout(true);
    showToast(`Loaded ${sourceLabel}. Settings were validated and the volume was restarted from the saved seed.`, 4300);
    return loaded;
  }

  function exportConfiguration() {
    const configDocument = createConfigurationDocument();
    const json = `${JSON.stringify(configDocument, null, 2)}\n`;
    const blob = new Blob([json], { type: "application/json" });
    const timestamp = new Date().toISOString().replaceAll(":", "-").replace("T", "_").slice(0, 19);
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `cloud-engine-config_${timestamp}.json`;
    anchor.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 2000);
    showToast("Exported every panel value, camera pose, pause state, and procedural seed as JSON.", 3600);
    return configDocument;
  }

  async function importConfigurationFile(file) {
    try {
      if (!(file instanceof File)) {
        throw new Error("No configuration file was selected.");
      }
      if (file.size > MAX_CONFIG_BYTES) {
        throw new Error("The configuration file is larger than 1 MiB.");
      }
      const text = await file.text();
      let configDocument;
      try {
        configDocument = JSON.parse(text);
      } catch (error) {
        throw new Error(`Invalid JSON: ${error.message || error}`);
      }
      applyConfigurationDocument(configDocument, file.name || "configuration file");
    } catch (error) {
      console.error("Configuration load failed", error);
      showToast(`Configuration load failed: ${error.message || error}`, 5200);
    }
  }

  function updateBrushCursorSize(distanceFromCamera = null) {
    const rect = canvas.getBoundingClientRect();
    let diameter;
    if (distanceFromCamera && distanceFromCamera > 0.01 && rect.height > 0) {
      const radiusWorld = state.brushSize * WORLD_SIZE[0];
      const radiusPixels = radiusWorld / (distanceFromCamera * getTanHalfFov()) * rect.height * 0.5;
      diameter = clamp(radiusPixels * 2, 18, Math.min(320, rect.height * 0.72));
    } else {
      diameter = clamp(state.brushSize * Math.min(window.innerWidth, window.innerHeight) * 1.38, 26, 180);
    }
    brushCursor.style.width = `${diameter}px`;
    brushCursor.style.height = `${diameter}px`;
  }

  function rayFromScreen(clientX, clientY) {
    const rect = canvas.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      return null;
    }
    const u = (clientX - rect.left) / rect.width;
    const v = (clientY - rect.top) / rect.height;
    if (u < 0 || u > 1 || v < 0 || v > 1) {
      return null;
    }
    const x = u * 2 - 1;
    const y = (1 - v) * 2 - 1;
    const aspect = rect.width / rect.height;
    const camera = getCameraBasis();
    const tanHalfFov = getTanHalfFov();
    return normalize3([
      camera.forward[0] + camera.right[0] * x * aspect * tanHalfFov + camera.up[0] * y * tanHalfFov,
      camera.forward[1] + camera.right[1] * x * aspect * tanHalfFov + camera.up[1] * y * tanHalfFov,
      camera.forward[2] + camera.right[2] * x * aspect * tanHalfFov + camera.up[2] * y * tanHalfFov,
    ]);
  }

  function intersectRayBox(origin, direction) {
    let nearDistance = -Infinity;
    let farDistance = Infinity;
    for (let axis = 0; axis < 3; axis += 1) {
      if (Math.abs(direction[axis]) < 1e-6) {
        if (origin[axis] < BOUNDS_MIN[axis] || origin[axis] > BOUNDS_MAX[axis]) {
          return null;
        }
        continue;
      }
      const inverse = 1 / direction[axis];
      let nearAxis = (BOUNDS_MIN[axis] - origin[axis]) * inverse;
      let farAxis = (BOUNDS_MAX[axis] - origin[axis]) * inverse;
      if (nearAxis > farAxis) {
        [nearAxis, farAxis] = [farAxis, nearAxis];
      }
      nearDistance = Math.max(nearDistance, nearAxis);
      farDistance = Math.min(farDistance, farAxis);
      if (nearDistance > farDistance) {
        return null;
      }
    }
    if (farDistance < 0) {
      return null;
    }
    return [Math.max(nearDistance, 0), farDistance];
  }

  function projectWorldToScreen(worldPosition) {
    const rect = canvas.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      return null;
    }
    const camera = getCameraBasis();
    const relative = worldPosition.map((value, axis) => value - state.cameraPosition[axis]);
    const depth = dot3(relative, camera.forward);
    if (depth <= 0.0001) {
      return null;
    }
    const tanHalfFov = getTanHalfFov();
    const aspect = rect.width / rect.height;
    const ndcX = dot3(relative, camera.right) / (depth * tanHalfFov * aspect);
    const ndcY = dot3(relative, camera.up) / (depth * tanHalfFov);
    const u = ndcX * 0.5 + 0.5;
    const v = 0.5 - ndcY * 0.5;
    return {
      clientX: rect.left + u * rect.width,
      clientY: rect.top + v * rect.height,
      ndcX,
      ndcY,
      depth,
    };
  }

  function updateBrushFromPointer(event) {
    state.lastPointer = [event.clientX, event.clientY];
    brushCursor.style.left = `${event.clientX}px`;
    brushCursor.style.top = `${event.clientY}px`;
    brushCursor.classList.add("is-visible");

    if (state.pointerLocked) {
      state.brushValid = false;
      return false;
    }

    const erase = event.shiftKey || event.button === 2 || (event.buttons & 2) !== 0;
    state.brushSign = erase ? -1 : 1;
    brushCursor.classList.toggle("is-erasing", erase);

    const direction = rayFromScreen(event.clientX, event.clientY);
    const hit = direction ? intersectRayBox(state.cameraPosition, direction) : null;
    if (!direction || !hit) {
      state.brushValid = false;
      brushCursor.classList.add("is-invalid");
      return false;
    }

    const distance = hit[0] + (hit[1] - hit[0]) * state.brushDepth;
    const worldPosition = [
      state.cameraPosition[0] + direction[0] * distance,
      state.cameraPosition[1] + direction[1] * distance,
      state.cameraPosition[2] + direction[2] * distance,
    ];
    state.brushWorldPosition = worldPosition;
    state.brushCenter = worldPosition.map((component, axis) => (
      clamp((component - BOUNDS_MIN[axis]) / WORLD_SIZE[axis], 0.002, 0.998)
    ));
    state.brushValid = true;
    brushCursor.classList.remove("is-invalid");
    updateBrushCursorSize(distance);
    return true;
  }

  function bindPointerControls() {
    canvas.addEventListener("pointerenter", (event) => {
      if (state.mode === "draw" && !state.pointerLocked) {
        updateBrushFromPointer(event);
      }
    });

    canvas.addEventListener("pointermove", (event) => {
      if (state.mode !== "draw" || state.pointerLocked) {
        return;
      }
      const valid = updateBrushFromPointer(event);
      if (paintPointerId === event.pointerId) {
        const paintButtonHeld = (event.buttons & 3) !== 0;
        state.brushActive = paintButtonHeld && valid;
      }
    });

    canvas.addEventListener("pointerleave", () => {
      if (!state.brushActive) {
        brushCursor.classList.remove("is-visible");
      }
    });

    canvas.addEventListener("pointerdown", (event) => {
      if (state.mode !== "draw" || state.pointerLocked || (event.button !== 0 && event.button !== 2)) {
        return;
      }
      event.preventDefault();
      canvas.setPointerCapture?.(event.pointerId);
      paintPointerId = event.pointerId;
      const valid = updateBrushFromPointer(event);
      state.brushActive = valid;
    });

    const releaseBrush = (event) => {
      if (event && canvas.hasPointerCapture?.(event.pointerId)) {
        canvas.releasePointerCapture(event.pointerId);
      }
      if (!event || paintPointerId === event.pointerId) {
        paintPointerId = null;
        state.brushActive = false;
      }
    };

    canvas.addEventListener("pointerup", releaseBrush);
    canvas.addEventListener("pointercancel", releaseBrush);
    window.addEventListener("blur", () => {
      paintPointerId = null;
      state.brushActive = false;
      cameraKeys.clear();
    });
    canvas.addEventListener("contextmenu", (event) => {
      if (state.mode === "draw") {
        event.preventDefault();
      }
    });

    canvas.addEventListener("wheel", (event) => {
      if (state.mode !== "draw" || state.pointerLocked) {
        return;
      }
      event.preventDefault();
      const direction = Math.sign(event.deltaY);
      state.brushDepth = clamp(state.brushDepth + direction * 0.035, 0.04, 0.96);
      const depthInput = $("#brushDepth");
      depthInput.value = state.brushDepth.toFixed(2);
      $("#brushDepthValue").textContent = `${Math.round(state.brushDepth * 100)}%`;
      setRangeProgress(depthInput);
      updateBrushFromPointer(event);
    }, { passive: false });

    canvas.addEventListener("dblclick", (event) => {
      if (state.mode === "auto" && !state.pointerLocked) {
        event.preventDefault();
        void enterFlycam();
      }
    });

    document.addEventListener("mousemove", (event) => {
      if (!state.pointerLocked) {
        return;
      }
      const radiansPerPixel = state.cameraSensitivity * Math.PI / 180;
      state.cameraYaw += event.movementX * radiansPerPixel;
      state.cameraPitch = clamp(
        state.cameraPitch - event.movementY * radiansPerPixel,
        -Math.PI * 0.494,
        Math.PI * 0.494,
      );
    });

    document.addEventListener("pointerlockchange", handlePointerLockChange);
    document.addEventListener("pointerlockerror", () => {
      showToast("The browser rejected pointer lock for the fly camera.", 3800);
    });
  }

  function bindUi() {
    bindRange("cloudAmount", "amount", (value) => `${Math.round(value * 100)}%`);
    bindRange("windSpeed", "windSpeed", (value) => value.toFixed(2));
    bindRange("windDirection", "windDirection", (value) => `${Math.round(value)}°`);
    bindRange("turbulence", "turbulence", (value) => `${Math.round(value / 1.5 * 100)}%`);
    bindRange("tearing", "tearing", (value) => `${Math.round(value / 1.5 * 100)}%`);
    bindRange("rotation", "rotation", (value) => `${Math.round(value * 100)}%`);
    bindRange("dissipation", "dissipation", (value) => `${Math.round(value / 0.7 * 100)}%`);
    bindRange("brushSize", "brushSize", (value) => `${Math.round(value * 100)}%`);
    bindRange("brushStrength", "brushStrength", (value) => `${Math.round(value * 100)}%`);
    bindRange("brushDepth", "brushDepth", (value) => `${Math.round(value * 100)}%`);
    bindRange("sunElevation", "sunElevation", (value) => `${Math.round(value)}°`);
    bindRange("sunAzimuth", "sunAzimuth", (value) => `${Math.round(value)}°`);
    bindRange("sunIntensity", "sunIntensity", (value) => value.toFixed(2));
    bindRange("exposure", "exposure", (value) => value.toFixed(2));
    bindRange("cameraFov", "cameraFov", (value) => `${Math.round(value)}°`);
    bindRange("cameraSpeed", "cameraSpeed", (value) => value.toFixed(2));
    bindRange("cameraSensitivity", "cameraSensitivity", (value) => value.toFixed(2));

    const artUpdate = () => markArtCustom();
    bindRange("artCurl", "artCurl", (value) => value.toFixed(2), artUpdate);
    bindRange("artRibbon", "artRibbon", (value) => `${Math.round(value / 1.5 * 100)}%`, artUpdate);
    bindRange("artSculpt", "artSculpt", (value) => `${Math.round(value * 100)}%`, artUpdate);
    bindRange("artBands", "artBands", (value) => String(Math.round(value)), artUpdate);
    bindRange("artOutline", "artOutline", (value) => `${Math.round(value / 1.5 * 100)}%`, artUpdate);
    bindRange("artMoonSize", "artMoonSize", (value) => `${(value * 180 / Math.PI).toFixed(1)}°`, artUpdate);
    bindRange("artMoonGlow", "artMoonGlow", (value) => value.toFixed(2), artUpdate);
    bindRange("artGrain", "artGrain", (value) => `${Math.round(value * 100)}%`, artUpdate);

    bindColor("sunColor", "sunColor");
    bindColor("artCloudColor", "artCloudColor", true);
    bindColor("artShadowColor", "artShadowColor", true);
    bindColor("artSkyColor", "artSkyColor", true);
    bindColor("artMoonColor", "artMoonColor", true);

    $("#patternSelect").addEventListener("change", (event) => {
      state.pattern = Number(event.target.value);
      if (state.mode === "auto") {
        state.pendingClear = true;
        showToast(`Formation changed to ${event.target.selectedOptions[0].text}.`);
      }
    });

    $("#styleSelect").addEventListener("change", (event) => {
      setRenderStyle(event.target.value);
    });

    $("#artPreset").addEventListener("change", (event) => {
      if (event.target.value !== "custom") {
        applyArtPreset(event.target.value);
      } else {
        state.artPreset = "custom";
      }
    });

    $("#qualitySelect").addEventListener("change", (event) => {
      state.quality = event.target.value;
      resizeCanvasIfNeeded();
      showToast(`${event.target.selectedOptions[0].text} quality selected.`);
    });

    $("#modeAuto").addEventListener("click", () => setMode("auto"));
    $("#modeDraw").addEventListener("click", () => setMode("draw"));
    $("#pauseButton").addEventListener("click", () => togglePause());
    $("#clearButton").addEventListener("click", clearVolume);
    $("#reseedButton").addEventListener("click", reseedVolume);
    $("#flyButton").addEventListener("click", toggleFlycam);
    $("#resetCameraButton").addEventListener("click", resetCamera);
    $("#captureButton").addEventListener("click", () => {
      if (captureInFlight || state.capturePending) {
        return;
      }
      state.capturePending = true;
      showToast(state.quality === "still" ? "Capturing still-quality frame…" : "Capturing current frame…");
    });

    $("#exportConfigButton").addEventListener("click", exportConfiguration);
    $("#loadConfigButton").addEventListener("click", () => {
      $("#configFileInput").click();
    });
    $("#configFileInput").addEventListener("change", (event) => {
      const file = event.target.files?.[0] || null;
      event.target.value = "";
      if (file) {
        void importConfigurationFile(file);
      }
    });

    $("#collapsePanel").addEventListener("click", () => {
      const collapsed = panel.classList.toggle("is-collapsed");
      $("#collapsePanel").setAttribute("aria-expanded", String(!collapsed));
      $("#collapsePanel").setAttribute("aria-label", collapsed ? "Expand controls" : "Collapse controls");
    });

    window.addEventListener("keydown", (event) => {
      const target = event.target;
      const isEditing = target instanceof HTMLInputElement || target instanceof HTMLSelectElement || target instanceof HTMLTextAreaElement;
      if (isEditing) {
        return;
      }

      if (MOVEMENT_CODES.has(event.code)) {
        cameraKeys.add(event.code);
        event.preventDefault();
        return;
      }

      if (event.repeat) {
        return;
      }
      if (event.code === "Space") {
        event.preventDefault();
        togglePause();
      } else if (event.key.toLowerCase() === "h") {
        $("#collapsePanel").click();
      } else if (event.key.toLowerCase() === "c") {
        clearVolume();
      } else if (event.key.toLowerCase() === "f") {
        event.preventDefault();
        toggleFlycam();
      }
    });

    window.addEventListener("keyup", (event) => {
      cameraKeys.delete(event.code);
    });

    window.addEventListener("resize", () => {
      updateBrushCursorSize();
      updateCameraReadout(true);
    }, { passive: true });

    bindPointerControls();
    setMode("auto", false);
    setRenderStyle("realistic", false);
    updateBrushCursorSize();
    updateCameraReadout(true);
  }

  window.__cloudEngineDebug = {
    version: ENGINE_VERSION,
    getState() {
      return {
        mode: state.mode,
        renderStyle: state.renderStyle,
        pattern: state.pattern,
        paused: state.paused,
        pointerLocked: state.pointerLocked,
        cameraPosition: [...state.cameraPosition],
        cameraYaw: state.cameraYaw,
        cameraPitch: state.cameraPitch,
        brushCenter: [...state.brushCenter],
        brushWorldPosition: [...state.brushWorldPosition],
        brushValid: state.brushValid,
        brushActive: state.brushActive,
        lastPointer: [...state.lastPointer],
      };
    },
    getConfiguration() {
      return createConfigurationDocument();
    },
    loadConfiguration(configDocument) {
      return applyConfigurationDocument(configDocument, "debug configuration");
    },
    getBrushProjection() {
      const projected = state.brushValid ? projectWorldToScreen(state.brushWorldPosition) : null;
      return {
        valid: state.brushValid,
        pointer: [...state.lastPointer],
        projected,
        delta: projected ? [
          projected.clientX - state.lastPointer[0],
          projected.clientY - state.lastPointer[1],
        ] : null,
      };
    },
    rayFromScreen,
    projectWorldToScreen,
  };

  bindUi();
  void initializeWebGPU();
})();
