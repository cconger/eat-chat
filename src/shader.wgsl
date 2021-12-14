
[[block]]
struct ProjectionUniform {
  cell_dim: vec2<f32>;
  size: vec2<f32>;
  offet: vec2<f32>;
};
[[group(0), binding(0)]]
var<uniform> projection: ProjectionUniform;

struct VertexInput {
  [[location(0)]] position: vec2<f32>;
};

struct InstanceInput {
  [[location(5)]] cell_coords: vec2<f32>;
  [[location(6)]] tex_offset: vec2<f32>;
  [[location(7)]] color: vec3<f32>;
};

struct VertexOutput {
  [[builtin(position)]] clip_position: vec4<f32>;
  [[location(0)]] tex_coords: vec2<f32>;
  [[location(1)]] color: vec3<f32>;
};

[[stage(vertex)]]
fn vs_main(
  model: VertexInput,
  instance: InstanceInput,
) -> VertexOutput {
  var out: VertexOutput;

  // Top left position 
  let pos: vec2<f32> = instance.cell_coords * projection.cell_dim;
  // Pixel offsets
  let size: vec2<f32> = model.position * projection.cell_dim;

  var translated: vec2<f32> = ((pos + size) * vec2<f32>(2.0/projection.size.x, -2.0/projection.size.y)) + vec2<f32>(-1.0, 1.0);

  out.tex_coords = instance.tex_offset;
  out.clip_position = vec4<f32>(translated, 0.0, 1.0);
  out.color = instance.color;
  return out;
}

[[stage(fragment)]]
fn fs_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
  return vec4<f32>(in.color, 1.0);
}
