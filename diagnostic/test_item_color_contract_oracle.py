"""Framework-free checks for the item color contract oracle."""
from item_color_contract_oracle import linear_to_srgb, palettes, srgb_to_linear

for value in (0.0, 0.0031308, 0.01, 0.18, 0.5, 1.0):
    assert abs(linear_to_srgb(srgb_to_linear(value)) - value) < 2e-6

source = [(107, 110, 107, 255), (88, 89, 88, 255), (0, 0, 0, 0)]
result = palettes(source, [124, 189, 107])
assert result["encoded_sample_encoded_tint"]
assert result["linear_sample_encoded_tint"] != result["encoded_sample_encoded_tint"]
print("item color contract oracle synthetic: PASS")
