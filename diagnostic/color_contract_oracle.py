#!/usr/bin/env python3
"""Independent float oracle for Java 26.2 terrain color versus Rust's paths.

It intentionally stops before byte rounding, alpha blending, and pixel metrics.
The Java shader operates on normalized texture/vertex values; Rust samples an
SRGB atlas and writes an SRGB swapchain, so the contract bridge is explicit.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Iterable


def srgb_to_linear(c: float) -> float:
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def linear_to_srgb(c: float) -> float:
    c = max(0.0, min(1.0, c))
    return 12.92 * c if c <= 0.0031308 else 1.055 * c ** (1.0 / 2.4) - 0.055


def vec(fn, values: Iterable[float]) -> list[float]:
    return [fn(value) for value in values]


def java_surface(tex_srgb: list[float], tint_srgb: list[float], light_srgb: float) -> list[float]:
    return [tex * tint * light_srgb for tex, tint in zip(tex_srgb, tint_srgb)]


def rust_old_surface(tex_srgb: list[float], tint_srgb: list[float], light_srgb: float) -> list[float]:
    tex_linear = vec(srgb_to_linear, tex_srgb)
    tint_linear = vec(lambda c: c ** 2.2, tint_srgb)
    light_linear = light_srgb ** 2.2
    return vec(linear_to_srgb, [t * c * light_linear for t, c in zip(tex_linear, tint_linear)])


def rust_contract_surface(tex_srgb: list[float], tint_srgb: list[float], light_srgb: float) -> list[float]:
    # SRGB atlas sample is decoded by hardware; restore Java's normalized value,
    # do the official encoded-space multiplication, then feed linear values to
    # the SRGB swapchain encoder.
    tex_encoded = vec(linear_to_srgb, vec(srgb_to_linear, tex_srgb))
    encoded = java_surface(tex_encoded, tint_srgb, light_srgb)
    return vec(srgb_to_linear, encoded)


def assert_close(actual: float, expected: float, epsilon: float = 2e-6) -> None:
    if abs(actual - expected) > epsilon:
        raise AssertionError(f"{actual!r} != {expected!r}")


def check() -> None:
    for value in (0.0, 0.0031308, 0.01, 0.04045, 0.18, 0.5, 1.0):
        assert_close(linear_to_srgb(srgb_to_linear(value)), value)
    for got in vec(srgb_to_linear, [0.0, 0.0, 0.0]):
        assert_close(got, 0.0)
    for got in vec(linear_to_srgb, [1.0, 1.0, 1.0]):
        assert_close(got, 1.0)
    for tex, tint, light in (
        ([1.0, 0.0, 0.0], [0.5, 1.0, 1.0], 0.8),
        ([0.5, 0.5, 0.5], [0.25, 0.75, 1.0], 0.8),
        ([0.2, 0.4, 0.8], [0.0, 0.5, 1.0], 0.0),
        ([0.2, 0.4, 0.8], [0.0, 0.5, 1.0], 1.0),
    ):
        expected = java_surface(tex, tint, light)
        actual = rust_contract_surface(tex, tint, light)
        for got, want in zip(actual, expected):
            assert_close(got, srgb_to_linear(want))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    check()
    cases = [
        {"name": "primary_red", "textureSrgb": [1.0, 0.0, 0.0], "tintSrgb": [0.5, 1.0, 1.0], "lightSrgb": 0.8},
        {"name": "neutral_mid", "textureSrgb": [0.5, 0.5, 0.5], "tintSrgb": [0.25, 0.75, 1.0], "lightSrgb": 0.8},
        {"name": "black_and_brightness_zero", "textureSrgb": [0.2, 0.4, 0.8], "tintSrgb": [0.0, 0.5, 1.0], "lightSrgb": 0.0},
        {"name": "brightness_one", "textureSrgb": [0.2, 0.4, 0.8], "tintSrgb": [0.0, 0.5, 1.0], "lightSrgb": 1.0},
        {"name": "stem_age0_light204", "textureSrgb": [0.5, 0.5, 0.5], "tintSrgb": [0.0, 1.0, 0.0], "lightSrgb": 204.0 / 255.0},
        {"name": "stem_age3_light204", "textureSrgb": [0.5, 0.5, 0.5], "tintSrgb": [96.0 / 255.0, 231.0 / 255.0, 12.0 / 255.0], "lightSrgb": 204.0 / 255.0},
        {"name": "stem_age7_light204", "textureSrgb": [0.5, 0.5, 0.5], "tintSrgb": [224.0 / 255.0, 199.0 / 255.0, 28.0 / 255.0], "lightSrgb": 204.0 / 255.0},
    ]
    for case in cases:
        tex = case["textureSrgb"]
        tint = case["tintSrgb"]
        light = case["lightSrgb"]
        case["javaEncoded"] = java_surface(tex, tint, light)
        case["rustOldFramebufferLinear"] = rust_old_surface(tex, tint, light)
        case["rustContractFramebufferLinear"] = rust_contract_surface(tex, tint, light)
    result = {
        "schema": 1,
        "contract": {
            "java": "UNORM normalized texture * vertexColor/lightmap/fog, UNORM target",
            "rustBefore": "SRGB atlas hardware decode -> pow(x,2.2) tint/light -> SRGB framebuffer",
            "rustAfter": "SRGB atlas decode -> exact sRGB re-encode -> Java encoded-space math -> exact sRGB decode -> SRGB framebuffer",
            "rounding": "none",
            "alphaBlend": "not evaluated",
        },
        "checks": {"piecewiseRoundtrip": True, "whiteIdentity": True, "black": True, "brightnessZeroAndOne": True},
        "cases": cases,
    }
    text = json.dumps(result, indent=2) + "\n"
    if args.write:
        args.write.parent.mkdir(parents=True, exist_ok=True)
        args.write.write_text(text, encoding="utf-8")
    else:
        print(text, end="")
    print("color contract oracle: PASS")


if __name__ == "__main__":
    main()
