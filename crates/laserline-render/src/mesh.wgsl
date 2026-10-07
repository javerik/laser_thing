// 3D preview of the object in the scan frame (mm, z up), rendered offscreen with
// MSAA and depth, then blitted into the target pass.

struct View {
    view_proj: mat4x4f,
    eye: vec4f,      // camera position
    z_range: vec4f,  // x = max height (for the height tint)
};

@group(0) @binding(0) var<uniform> view: View;

struct MeshOut {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
};

@vertex
fn vs_mesh(@location(0) pos: vec3f) -> MeshOut {
    var o: MeshOut;
    o.clip = view.view_proj * vec4f(pos, 1.0);
    o.world = pos;
    return o;
}

@fragment
fn fs_mesh(in: MeshOut) -> @location(0) vec4f {
    // Flat face normal from screen-space derivatives; lit two-sided by a headlight.
    let n = normalize(cross(dpdx(in.world), dpdy(in.world)));
    let to_eye = normalize(view.eye.xyz - in.world);
    let lambert = abs(dot(n, to_eye));
    let h = clamp(in.world.z / max(view.z_range.x, 1e-3), 0.0, 1.0);
    let base = mix(vec3f(0.42, 0.46, 0.52), vec3f(0.80, 0.82, 0.86), h);
    return vec4f(base * (0.25 + 0.75 * lambert), 1.0);
}

struct LineOut {
    @builtin(position) clip: vec4f,
    @location(0) color: vec4f,
};

@vertex
fn vs_line(@location(0) pos: vec3f, @location(1) color: vec4f) -> LineOut {
    var o: LineOut;
    o.clip = view.view_proj * vec4f(pos, 1.0);
    o.color = color;
    return o;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4f {
    return in.color;
}

// ---- Blit of the resolved offscreen image into the egui render pass ----

// Bindings 1–3 so they never collide with `view` within this module.
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var image_sampler: sampler;
@group(0) @binding(3) var<uniform> blit_srgb: vec4u; // x = 1 if the target encodes sRGB itself

struct BlitOut {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_blit(@builtin(vertex_index) i: u32) -> BlitOut {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    var o: BlitOut;
    o.clip = vec4f(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2f(p.x, 1.0 - p.y);
    return o;
}

fn srgb_to_linear(c: vec3f) -> vec3f {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3f(2.4));
    return select(hi, lo, c <= vec3f(0.04045));
}

@fragment
fn fs_blit(in: BlitOut) -> @location(0) vec4f {
    var c = textureSample(image, image_sampler, in.uv).rgb;
    if blit_srgb.x == 1u {
        c = srgb_to_linear(c);
    }
    return vec4f(c, 1.0);
}
