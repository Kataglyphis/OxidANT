//! `brdf.slang`'s energy fit on the CPU, evaluated as `ggx_energy` evaluates it, so analytic references take the shaders' gain.

use std::sync::OnceLock;

/// Embedded, so a test staged on another device reads it without the source tree.
const FIT_SOURCE: &str = include_str!("../../../../shaders/common/brdf_energy_fit.slang");
/// `brdf.slang`'s `GGX_ENERGY_MIN_N_DOT_V` and `MIN_ROUGHNESS`: where the fit's coordinates start.
const MIN_N_DOT_V: f64 = 1e-4;
const MIN_ROUGHNESS: f64 = 0.045;

/// The checked-in spline: its size per axis and its (scale, bias) coefficients, row by row.
pub fn fit() -> &'static (usize, Vec<[f64; 2]>) {
    static FIT: OnceLock<(usize, Vec<[f64; 2]>)> = OnceLock::new();
    FIT.get_or_init(|| {
        // The coefficients are the float2 literals of ggx_energy_fit's body, in order.
        let body = FIT_SOURCE
            .split_once("float2 ggx_energy_fit(")
            .expect("the fit declares ggx_energy_fit")
            .1;
        let coefficients: Vec<[f64; 2]> = body
            .split("float2(")
            .skip(1)
            .map(|entry| {
                let pair: Vec<f64> = entry
                    .split(')')
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .map(|n| n.trim().parse().expect("a coefficient is two numbers"))
                    .collect();
                [pair[0], pair[1]]
            })
            .collect();
        let size = (coefficients.len() as f64).sqrt().round() as usize;
        assert_eq!(size * size, coefficients.len(), "the fit is not square");
        (size, coefficients)
    })
}

/// `ggx_energy_basis`: the uniform cubic B-spline, nonzero on (-2, 2).
pub fn basis(x: f64) -> f64 {
    let t = x.abs();
    if t < 1.0 {
        2.0 / 3.0 - t * t + 0.5 * t * t * t
    } else if t < 2.0 {
        (2.0 - t).powi(3) / 6.0
    } else {
        0.0
    }
}

/// `ggx_energy_coordinates`: an n.v and a roughness as fit coordinates in [0, 1].
pub fn coordinates(n_dot_v: f64, roughness: f64) -> [f64; 2] {
    let low = [MIN_N_DOT_V.cbrt(), MIN_ROUGHNESS.sqrt()];
    let c = [
        n_dot_v.clamp(MIN_N_DOT_V, 1.0).cbrt(),
        roughness.clamp(MIN_ROUGHNESS, 1.0).sqrt(),
    ];
    [0, 1].map(|a| (c[a] - low[a]) / (1.0 - low[a]))
}

/// `ggx_energy_fit`: the `size` x `size` spline of `coefficients` at coordinates `t`.
pub fn spline(size: usize, coefficients: &[[f64; 2]], t: [f64; 2]) -> [f64; 2] {
    let x = t.map(|c| c * (size - 3) as f64 + 1.0);
    let mut out = [0.0; 2];
    for (i, row) in coefficients.chunks(size).enumerate() {
        let weight = basis(x[0] - i as f64);
        for (j, c) in row.iter().enumerate() {
            let w = weight * basis(x[1] - j as f64);
            out[0] += c[0] * w;
            out[1] += c[1] * w;
        }
    }
    out
}

/// `ggx_energy(n_dot_v, roughness)`: the single-scatter lobe's (Fresnel scale, bias).
pub fn ggx_energy(n_dot_v: f64, roughness: f64) -> [f64; 2] {
    let (size, coefficients) = fit();
    spline(*size, coefficients, coordinates(n_dot_v, roughness))
}

/// `multiscatter_gain` for a grey f0.
pub fn multiscatter_gain(energy: [f64; 2], f0: f64) -> f64 {
    1.0 + f0 * (1.0 / (energy[0] + energy[1]).max(1e-3) - 1.0)
}

/// `specular_albedo` for a grey f0: the compensated lobe's E_spec(mu).
pub fn specular_albedo(energy: [f64; 2], f0: f64) -> f64 {
    multiscatter_gain(energy, f0) * (f0 * energy[0] + energy[1])
}
