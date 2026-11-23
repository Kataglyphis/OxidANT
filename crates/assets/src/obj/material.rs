//! How an OBJ material draws, one rule set for the glTF the WebGPU renderer reads and the arrays the C++ engine
//! reads (owner, 2026-11-19).

use super::parse::ObjMaterial;
use crate::scene::AlphaMode;

/// A `map_d` mask cuts at the C++ engine's threshold.
pub(super) const MASK_CUTOFF: f32 = 0.5;

/// Replaces a non-finite float, which no shader and no JSON takes.
pub(super) fn finite_or(value: f32, default: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        default
    }
}

/// `Kd` and `d`/`Tr`; without `Kd` white, glTF's default.
pub(super) fn base_color(material: &ObjMaterial) -> [f32; 4] {
    material
        .base_color
        .map(|component| finite_or(component, 1.0))
}

/// `Pm`, else a plain dielectric.
pub(super) fn metallic(material: &ObjMaterial) -> f32 {
    material.metallic.map_or(0.0, |value| finite_or(value, 0.0))
}

/// `Pr`; without it `Ns` through the C++ `material_roughness()` curve, so both renderers agree; without either 1.
pub(super) fn roughness(material: &ObjMaterial) -> f32 {
    match (material.roughness, material.shininess) {
        (Some(value), _) => finite_or(value, 1.0),
        (None, Some(shininess)) if shininess.is_finite() && shininess >= 0.0 => {
            (2.0 / (shininess + 2.0)).sqrt().clamp(0.045, 1.0)
        }
        _ => 1.0,
    }
}

/// `Ke`, unclamped; a `map_Ke` without any `Ke` glows at factor 1, and an authored `Ke 0 0 0` stays dark.
pub(super) fn emissive(material: &ObjMaterial) -> [f32; 3] {
    match material.emissive {
        Some(factor) => factor.map(|component| finite_or(component, 0.0)),
        None if material.emissive_texture.is_some() => [1.0; 3],
        None => [0.0; 3],
    }
}

/// `map_d` is a cut-out at [`MASK_CUTOFF`]; otherwise a `d` below 1 (or a `Tr` above 0) blends.
pub(super) fn alpha_mode(material: &ObjMaterial) -> AlphaMode {
    if material.alpha_texture.is_some() {
        AlphaMode::Mask(MASK_CUTOFF)
    } else if base_color(material)[3] < 1.0 {
        AlphaMode::Blend
    } else {
        AlphaMode::Opaque
    }
}
