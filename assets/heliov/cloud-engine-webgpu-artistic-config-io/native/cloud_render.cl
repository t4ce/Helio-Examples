// Native compute twin of shaders/render.wgsl.
// The web build uses a fragment shader only to reach the swapchain. This kernel
// runs the same ray-march math as one compute work-item per output pixel.


typedef struct {
  float4 resolution_time;
  float4 camera_position_tan_fov;
  float4 camera_forward_exposure;
  float4 camera_right_steps;
  float4 camera_up_detail;
  float4 sun_direction_intensity;
  float4 sun_color_extinction;
  float4 sky_top_ambient;
  float4 sky_horizon_seed;
  float4 bounds_min_density;
  float4 bounds_max_shadow;
  float4 options;
  float4 art_style;
  float4 art_cloud_color;
  float4 art_shadow_color;
  float4 art_sky_color;
  float4 art_moon_color;
} RenderParams;

#define CLOUD_PI 3.14159265358979323846f
#define MAX_PRIMARY_STEPS 112u
#define MAX_LIGHT_STEPS 8u

inline float fract1(const float value) {
  return value - floor(value);
}

inline float hash12(const float2 p) {
  const float h = dot(p, (float2)(127.1f, 311.7f));
  return fract1(sin(h) * 43758.5453123f);
}

inline float soft_circle(
    const float2 uv,
    const float2 center,
    const float radius) {
  return 1.0f - smoothstep(radius * 0.68f, radius, length(uv - center));
}

inline float2 intersect_box(
    const float3 ray_origin,
    const float3 ray_direction,
    const float3 bounds_min,
    const float3 bounds_max) {
  const float3 safe_direction = (float3)(
      fabs(ray_direction.x) > 0.00001f ? ray_direction.x : -0.00001f,
      fabs(ray_direction.y) > 0.00001f ? ray_direction.y : -0.00001f,
      fabs(ray_direction.z) > 0.00001f ? ray_direction.z : -0.00001f);
  const float3 inverse_direction = (float3)(1.0f) / safe_direction;
  const float3 t0 = (bounds_min - ray_origin) * inverse_direction;
  const float3 t1 = (bounds_max - ray_origin) * inverse_direction;
  const float3 near_values = fmin(t0, t1);
  const float3 far_values = fmax(t0, t1);
  const float near_distance = fmax(fmax(near_values.x, near_values.y), near_values.z);
  const float far_distance = fmin(fmin(far_values.x, far_values.y), far_values.z);
  return (float2)(near_distance, far_distance);
}

inline float2 sample_cloud(
    const float3 world_position,
    read_only image3d_t cloud_volume,
    sampler_t volume_sampler,
    constant RenderParams* params) {
  const float3 bounds_min = params->bounds_min_density.xyz;
  const float3 bounds_max = params->bounds_max_shadow.xyz;
  const float3 uv = (world_position - bounds_min) / (bounds_max - bounds_min);

  if (any(uv < (float3)(0.0f)) || any(uv > (float3)(1.0f))) {
    return (float2)(0.0f);
  }

  const float4 sample_value = read_imagef(
      cloud_volume,
      volume_sampler,
      (float4)(uv.x, uv.y, uv.z, 0.0f));
  const float base_density = sample_value.x;
  const float fine_noise = sample_value.y;
  const float edge_weight = clamp(4.0f * base_density * (1.0f - base_density), 0.0f, 1.0f);
  const float erosion = (0.53f - fine_noise) * params->camera_up_detail.w * edge_weight * 0.44f;
  const float shaped_density = fmax(0.0f, base_density - erosion);
  return (float2)(shaped_density * params->bounds_min_density.w, fine_noise);
}

inline float3 cloud_gradient(
    const float3 world_position,
    const float center_density,
    read_only image3d_t cloud_volume,
    sampler_t volume_sampler,
    constant RenderParams* params) {
  const float quality = clamp(params->options.y, 0.0f, 2.0f);
  const float epsilon = mix(0.16f, 0.095f, quality * 0.5f);
  return (float3)(
      sample_cloud(
          world_position + (float3)(epsilon, 0.0f, 0.0f),
          cloud_volume,
          volume_sampler,
          params).x - center_density,
      sample_cloud(
          world_position + (float3)(0.0f, epsilon, 0.0f),
          cloud_volume,
          volume_sampler,
          params).x - center_density,
      sample_cloud(
          world_position + (float3)(0.0f, 0.0f, epsilon),
          cloud_volume,
          volume_sampler,
          params).x - center_density);
}

inline float light_transmittance(
    const float3 world_position,
    const float3 sun_direction,
    read_only image3d_t cloud_volume,
    sampler_t volume_sampler,
    constant RenderParams* params) {
  float optical_depth = 0.0f;
  float3 sample_position = world_position;
  const float quality = clamp(params->options.y, 0.0f, 2.0f);
  const uint requested_steps = convert_uint_rte(4.0f + quality * 2.0f);
  const float step_length = mix(0.62f, 0.43f, quality * 0.5f);

  for (uint step = 0u; step < MAX_LIGHT_STEPS; ++step) {
    if (step >= requested_steps) {
      break;
    }
    sample_position += sun_direction * step_length;
    optical_depth += sample_cloud(
        sample_position,
        cloud_volume,
        volume_sampler,
        params).x * step_length;
  }

  const float extinction = params->sun_color_extinction.w;
  const float shadow_strength = params->bounds_max_shadow.w;
  return exp(-optical_depth * extinction * shadow_strength);
}

inline float artistic_light_visibility(
    const float3 world_position,
    const float3 sun_direction,
    read_only image3d_t cloud_volume,
    sampler_t volume_sampler,
    constant RenderParams* params) {
  float optical_depth = 0.0f;
  float3 sample_position = world_position;
  const float step_length = 0.68f;

  for (int step = 0; step < 3; ++step) {
    sample_position += sun_direction * step_length;
    optical_depth += sample_cloud(
        sample_position, cloud_volume, volume_sampler, params).x * step_length;
  }

  return exp(-optical_depth * params->sun_color_extinction.w
      * params->bounds_max_shadow.w * 0.82f);
}

inline float henyey_greenstein(const float cosine_theta, const float anisotropy) {
  const float g2 = anisotropy * anisotropy;
  const float denominator = pow(fmax(1.0f + g2 - 2.0f * anisotropy * cosine_theta, 0.0001f), 1.5f);
  return (1.0f - g2) / (4.0f * CLOUD_PI * denominator);
}

inline float3 realistic_sky_color(
    const float3 ray_direction,
    const float3 sun_direction,
    constant RenderParams* params) {
  const float3 horizon = params->sky_horizon_seed.xyz;
  const float3 top = params->sky_top_ambient.xyz;
  const float sky_factor = pow(clamp(ray_direction.y * 0.52f + 0.48f, 0.0f, 1.0f), 0.62f);
  float3 color = mix(horizon, top, sky_factor);

  const float sun_alignment = fmax(dot(ray_direction, sun_direction), 0.0f);
  const float sun_disc = smoothstep(0.99972f, 0.99994f, sun_alignment);
  const float sun_glow = pow(sun_alignment, 28.0f) * 0.25f
      + pow(sun_alignment, 160.0f) * 0.9f;
  const float horizon_glow = pow(fmax(0.0f, 1.0f - fabs(ray_direction.y)), 9.0f) * 0.065f;
  const float3 sun_light = params->sun_color_extinction.xyz * params->sun_direction_intensity.w;
  color += sun_light * (sun_disc * 5.5f + sun_glow + horizon_glow);
  return color;
}

inline float3 artistic_sky_color(
    const float3 ray_direction,
    const float3 sun_direction,
    constant RenderParams* params) {
  const float3 base_sky = params->art_sky_color.xyz;
  const float vertical = clamp(ray_direction.y * 0.5f + 0.5f, 0.0f, 1.0f);
  const float side_falloff = 0.86f + 0.14f * pow(fmax(1.0f - fabs(ray_direction.x), 0.0f), 2.0f);
  float3 color = base_sky * mix(0.62f, 1.18f, vertical) * side_falloff;

  const float3 reference_axis = fabs(sun_direction.y) > 0.92f
      ? (float3)(1.0f, 0.0f, 0.0f)
      : (float3)(0.0f, 1.0f, 0.0f);
  const float3 tangent = normalize(cross(reference_axis, sun_direction));
  const float3 bitangent = normalize(cross(sun_direction, tangent));
  const float moon_radius = fmax(params->art_sky_color.w, 0.001f);
  const float angular_scale = fmax(sin(moon_radius), 0.001f);
  const float2 moon_uv = (float2)(
      dot(ray_direction, tangent),
      dot(ray_direction, bitangent)) / angular_scale;
  const float moon_radius_uv = length(moon_uv);
  const int facing_moon = dot(ray_direction, sun_direction) > 0.0f;
  const float disc = facing_moon
      ? 1.0f - smoothstep(0.94f, 1.02f, moon_radius_uv)
      : 0.0f;

  const float crater_a = soft_circle(moon_uv, (float2)(-0.28f, 0.24f), 0.20f);
  const float crater_b = soft_circle(moon_uv, (float2)(0.35f, -0.08f), 0.15f);
  const float crater_c = soft_circle(moon_uv, (float2)(0.02f, -0.38f), 0.11f);
  const float crater_d = soft_circle(moon_uv, (float2)(0.42f, 0.36f), 0.08f);
  const float2 moon_cells = floor((moon_uv + (float2)(1.0f)) * 8.0f);
  const float moon_mottle = hash12(moon_cells + (float2)(params->sky_horizon_seed.w));
  const float moon_surface = clamp(
      1.03f - crater_a * 0.13f - crater_b * 0.10f - crater_c * 0.11f - crater_d * 0.08f
          + (moon_mottle - 0.5f) * 0.095f,
      0.68f,
      1.16f);

  const float alignment = fmax(dot(ray_direction, sun_direction), 0.0f);
  const float halo = facing_moon
      ? exp(-fmax(moon_radius_uv - 0.9f, 0.0f) * 3.25f)
          * smoothstep(0.16f, 0.98f, alignment)
      : 0.0f;
  const float3 moon_color = params->art_moon_color.xyz;
  color += moon_color * (
      disc * moon_surface * (1.25f + params->art_moon_color.w * 0.58f)
          + halo * params->art_moon_color.w * 0.12f);
  return color;
}

inline float3 sky_color(
    const float3 ray_direction,
    const float3 sun_direction,
    constant RenderParams* params) {
  if (params->art_style.x > 0.5f) {
    return artistic_sky_color(ray_direction, sun_direction, params);
  }
  return realistic_sky_color(ray_direction, sun_direction, params);
}

inline float3 aces_tonemap(const float3 color) {
  const float a = 2.51f;
  const float b = 0.03f;
  const float c = 2.43f;
  const float d = 0.59f;
  const float e = 0.14f;
  return clamp(
      (color * (a * color + (float3)(b)))
          / (color * (c * color + (float3)(d)) + (float3)(e)),
      (float3)(0.0f),
      (float3)(1.0f));
}

__kernel __attribute__((reqd_work_group_size(8, 8, 1)))
void cloud_render(
    constant RenderParams* params,
    read_only image3d_t cloud_volume,
    sampler_t volume_sampler,
    write_only image2d_t output_image) {
  const int x = (int)get_global_id(0);
  const int y = (int)get_global_id(1);
  const int width = get_image_width(output_image);
  const int height = get_image_height(output_image);

  if (x >= width || y >= height) {
    return;
  }

  const float2 resolution = fmax(params->resolution_time.xy, (float2)(1.0f));
  const float aspect = resolution.x / resolution.y;
  const float2 uv = ((float2)((float)x + 0.5f, (float)y + 0.5f)) / resolution;
  // Native image coordinates start at the top-left, so this is the same +Y-up
  // camera ray convention used by the corrected WebGPU fragment projection.
  const float2 screen = (float2)(uv.x * 2.0f - 1.0f, (1.0f - uv.y) * 2.0f - 1.0f);

  const float3 ray_origin = params->camera_position_tan_fov.xyz;
  const float3 ray_direction = normalize(
      params->camera_forward_exposure.xyz
      + params->camera_right_steps.xyz * (screen.x * aspect * params->camera_position_tan_fov.w)
      + params->camera_up_detail.xyz * (screen.y * params->camera_position_tan_fov.w));

  const float3 sun_direction = normalize(params->sun_direction_intensity.xyz);
  float3 background = sky_color(ray_direction, sun_direction, params);
  if (params->art_style.x > 0.5f) {
    const float2 star_cell = floor((float2)((float)x, (float)y) / 3.0f);
    const float star_hash = hash12(star_cell + (float2)(params->sky_horizon_seed.w));
    const float star = smoothstep(0.9965f, 0.9995f, star_hash)
        * smoothstep(-0.12f, 0.38f, ray_direction.y);
    background += params->art_moon_color.xyz * star * 0.19f;
  }

  const float2 hit = intersect_box(
      ray_origin,
      ray_direction,
      params->bounds_min_density.xyz,
      params->bounds_max_shadow.xyz);
  const float near_distance = fmax(hit.x, 0.0f);
  const float far_distance = hit.y;

  float3 radiance = background;

  if (far_distance > near_distance) {
    const uint requested_steps = convert_uint_rte(clamp(
        params->camera_right_steps.w,
        16.0f,
        (float)MAX_PRIMARY_STEPS));
    const float step_length = (far_distance - near_distance) / fmax((float)requested_steps, 1.0f);
    const float frame_jitter = hash12(
        (float2)((float)x, (float)y)
        + (float2)(params->resolution_time.w * 0.754877666f));
    float distance_along_ray = near_distance + frame_jitter * step_length;
    float transmittance = 1.0f;
    float3 integrated_light = (float3)(0.0f);

    const float view_sun_alignment = clamp(dot(ray_direction, sun_direction), -1.0f, 1.0f);
    const float phase_forward = henyey_greenstein(view_sun_alignment, 0.68f);
    const float phase_backward = henyey_greenstein(view_sun_alignment, -0.22f);
    const float phase = phase_forward * 0.78f + phase_backward * 0.22f;

    for (uint step = 0u; step < MAX_PRIMARY_STEPS; ++step) {
      if (step >= requested_steps || distance_along_ray > far_distance || transmittance < 0.008f) {
        break;
      }

      const float3 world_position = ray_origin + ray_direction * distance_along_ray;
      const float2 density_sample = sample_cloud(
          world_position,
          cloud_volume,
          volume_sampler,
          params);
      const float density = density_sample.x;

      if (density > 0.004f) {
        const float height_fraction = clamp(
            (world_position.y - params->bounds_min_density.y)
                / (params->bounds_max_shadow.y - params->bounds_min_density.y),
            0.0f,
            1.0f);

        if (params->art_style.x > 0.5f) {
          const float sculpt = clamp(params->art_style.w, 0.0f, 1.0f);
          const float sculpted_density = mix(
              density,
              smoothstep(0.035f, 0.62f, density),
              sculpt);
          const float sample_alpha = 1.0f - exp(
              -sculpted_density * params->sun_color_extinction.w * step_length
                  * mix(1.05f, 1.95f, sculpt));
          const float sun_visibility = artistic_light_visibility(
              world_position,
              sun_direction,
              cloud_volume,
              volume_sampler,
              params);
          const float3 gradient = cloud_gradient(
              world_position,
              density,
              cloud_volume,
              volume_sampler,
              params);
          const float gradient_strength = clamp(length(gradient) * 3.2f, 0.0f, 1.0f);
          const float3 normal = normalize(-gradient + (float3)(0.0001f, 0.0002f, 0.0001f));
          const float diffuse = fmax(dot(normal, sun_direction), 0.0f);
          const float view_facing = fabs(dot(normal, -ray_direction));
          const float rim = pow(fmax(1.0f - view_facing, 0.0f), 2.2f);
          const float bands = fmax(params->art_style.y, 2.0f);
          const float raw_shade = clamp(
              0.16f
                  + diffuse * 0.43f
                  + sun_visibility * 0.22f
                  + height_fraction * 0.10f
                  + rim * 0.08f,
              0.0f,
              1.0f);
          const float quantized_shade = floor(raw_shade * (bands - 1.0f) + 0.5f)
              / fmax(bands - 1.0f, 1.0f);
          const float3 shadow_color = params->art_shadow_color.xyz;
          const float3 cloud_color = params->art_cloud_color.xyz;
          float3 local_light = mix(shadow_color, cloud_color, quantized_shade);
          local_light = mix(
              local_light,
              shadow_color * 0.70f,
              (1.0f - sun_visibility) * 0.34f);
          local_light += params->art_moon_color.xyz
              * diffuse * sun_visibility * params->sun_direction_intensity.w * 0.095f;

          const float silhouette = pow(fmax(1.0f - view_facing, 0.0f), 1.35f);
          const float ink_mask = clamp(
              silhouette * (0.25f + gradient_strength * 0.75f)
                  * params->art_style.z * 0.92f,
              0.0f,
              0.86f);
          local_light = mix(local_light, shadow_color * 0.34f, ink_mask);
          local_light += cloud_color * rim * 0.055f;

          integrated_light += transmittance * sample_alpha * local_light;
          transmittance *= 1.0f - sample_alpha;
        } else {
          const float extinction = params->sun_color_extinction.w;
          const float sample_alpha = 1.0f - exp(-density * extinction * step_length);
          const float sun_visibility = light_transmittance(
              world_position,
              sun_direction,
              cloud_volume,
              volume_sampler,
              params);

          const float3 ambient_gradient = mix(
              params->sky_horizon_seed.xyz,
              params->sky_top_ambient.xyz,
              0.25f + height_fraction * 0.55f);
          float ambient_occluder = density * 0.28f;
          if (params->options.y > 1.5f) {
            ambient_occluder += sample_cloud(
                world_position + (float3)(0.0f, 0.46f, 0.0f),
                cloud_volume,
                volume_sampler,
                params).x * 0.55f;
          }
          const float ambient_visibility = exp(-ambient_occluder * 1.15f);
          const float3 ambient = ambient_gradient * params->sky_top_ambient.w
              * (0.75f + height_fraction * 0.32f)
              * mix(0.52f, 1.0f, ambient_visibility);

          const float multiple_scattering = (
              sun_visibility
              + sqrt(fmax(sun_visibility, 0.0f)) * 0.28f
              + pow(fmax(sun_visibility, 0.0f), 0.25f) * 0.10f) / 1.38f;
          const float powder = 1.0f - exp(-density * 2.35f);
          const float silver_lining = pow(sun_visibility, 3.0f)
              * pow(1.0f - clamp(density, 0.0f, 1.0f), 2.0f)
              * (0.04f + phase * 2.4f);
          const float direct_strength = multiple_scattering
              * (0.18f + phase * 7.8f + powder * 0.16f)
              + silver_lining * 0.34f;
          const float3 direct = params->sun_color_extinction.xyz
              * params->sun_direction_intensity.w
              * direct_strength;
          const float3 cool_core = (float3)(0.72f, 0.82f, 0.94f)
              * (1.0f - sun_visibility) * 0.047f;
          const float3 local_light = ambient + direct + cool_core;

          integrated_light += transmittance * sample_alpha * local_light;
          transmittance *= 1.0f - sample_alpha;
        }
      }

      distance_along_ray += step_length;
    }

    radiance = integrated_light + background * transmittance;
  }

  const float exposure = params->camera_forward_exposure.w;
  if (params->art_style.x > 0.5f) {
    const float3 exposed = fmax(radiance * exposure, (float3)(0.0f));
    const float3 compressed = exposed / ((float3)(1.0f) + exposed * 0.68f);
    const float3 gamma_color = pow(
        clamp(compressed, (float3)(0.0f), (float3)(1.0f)),
        (float3)(1.0f / 2.2f));
    const float grain_strength = clamp(params->art_cloud_color.w, 0.0f, 1.0f);
    const float grain = (hash12(
        (float2)((float)x, (float)y)
            + (float2)(params->sky_horizon_seed.w * 1.73f)) - 0.5f)
        * grain_strength * 0.034f;
    const float levels = mix(96.0f, 30.0f, grain_strength);
    const float3 printed = floor(
        clamp(gamma_color + (float3)(grain), (float3)(0.0f), (float3)(1.0f))
            * levels + (float3)(0.5f)) / levels;
    write_imagef(output_image, (int2)(x, y), (float4)(printed, 1.0f));
    return;
  }

  const float3 mapped = aces_tonemap(fmax(radiance * exposure, (float3)(0.0f)));
  const float dither = (hash12(
      (float2)((float)x, (float)y)
          + (float2)(params->sky_horizon_seed.w)) - 0.5f) / 255.0f;
  const float3 display_color = pow(
      clamp(mapped + (float3)(dither), (float3)(0.0f), (float3)(1.0f)),
      (float3)(1.0f / 2.2f));
  write_imagef(output_image, (int2)(x, y), (float4)(display_color, 1.0f));
}
