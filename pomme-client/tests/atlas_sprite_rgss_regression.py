"""Regression check for continuous RGSS gradients across sprite-UV wrapping."""
from pathlib import Path
import re

shader = Path(__file__).parents[1] / "src/renderer/shaders/atlas_sprite.glsl"
source = shader.read_text()
rgss = source.split("vec4 sample_atlas_sprite_rgss", 1)[1]
assert "dFdx(sprite_uv) * atlas_scale" in rgss
assert "dFdy(sprite_uv) * atlas_scale" in rgss
assert "dFdx(uv)" not in rgss and "dFdy(uv)" not in rgss

# local U 0.99 -> 1.03 over one pixel: continuous gradient is 0.04.
# With a 16-texel sprite in a 256-texel atlas: 0.04 * 16 = 0.64 texel.
continuous = (1.03 - 0.99) * 16
wrapped = (0.03 - 0.99) * 16
assert abs(continuous - 0.64) < 1e-9
assert abs(wrapped - (-15.36)) < 1e-9
