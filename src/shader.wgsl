
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
  [[location(7)]] tex_size: vec2<f32>;
  [[location(8)]] bg_color: vec3<f32>;
  [[location(9)]] fg_color: vec4<f32>;
  [[location(10)]] glyph_pos: vec4<f32>;
};

struct BGOutput {
  [[builtin(position)]] clip_position: vec4<f32>;
  [[location(1)]] color: vec3<f32>;
};

[[stage(vertex)]]
fn vs_bg(
  model: VertexInput,
  instance: InstanceInput,
) -> BGOutput {
  var out: BGOutput;

  // Top left position 
  let pos: vec2<f32> = instance.cell_coords * projection.cell_dim;
  // Pixel offsets
  let size: vec2<f32> = model.position * projection.cell_dim;

  var translated: vec2<f32> = ((pos + size) * vec2<f32>(2.0/projection.size.x, -2.0/projection.size.y)) + vec2<f32>(-1.0, 1.0);

  out.clip_position = vec4<f32>(translated, 0.0, 1.0);
  out.color = instance.bg_color;
  return out;
}

[[stage(fragment)]]
fn fs_bg(in: BGOutput) -> [[location(0)]] vec4<f32> {
  return vec4<f32>(in.color, 1.0);
}

struct VertexOutput {
  [[builtin(position)]] clip_position: vec4<f32>;
  [[location(0)]] tex_coords: vec2<f32>;
  [[location(1)]] color: vec4<f32>;
};

[[stage(vertex)]]
fn vs_main(
  model: VertexInput,
  instance: InstanceInput,
) -> VertexOutput {
  var out: VertexOutput;

  // Top Left of Cell
  let pos: vec2<f32> = (instance.cell_coords * projection.cell_dim);

  // Position Scaled to size of glyph
  let size: vec2<f32> = model.position * vec2<f32>(instance.glyph_pos.zw);

  let top_offset = (projection.cell_dim.y - instance.glyph_pos.y);
  let left_offset = instance.glyph_pos.x;
  let cell_offset = vec2<f32>(left_offset, top_offset);

  // This vertex's position translated to cell and with glyph offsets and projected to screen space;
  var translated: vec2<f32> = ((pos + size + cell_offset) * vec2<f32>(2.0/projection.size.x, -2.0/projection.size.y)) + vec2<f32>(-1.0, 1.0);

  out.color = instance.fg_color;
  out.tex_coords = (model.position * instance.tex_size) + instance.tex_offset;
  out.clip_position = vec4<f32>(translated, 0.0, 1.0);
  return out;
}

[[group(1), binding(0)]]
var t_diffuse: texture_2d<f32>;
[[group(1), binding(1)]]
var s_diffuse: sampler;

[[stage(fragment)]]
fn fs_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {

  let tex_color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
  return tex_color * in.color;
}
