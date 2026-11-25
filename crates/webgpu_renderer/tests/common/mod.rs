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
        // The coefficients are the float2 literals of GGX_ENERGY_COEFFICIENTS, in order.
        let table = FIT_SOURCE
            .split_once("GGX_ENERGY_COEFFICIENTS[")
            .and_then(|(_, rest)| rest.split_once("};"))
            .expect("the fit declares GGX_ENERGY_COEFFICIENTS")
            .0;
        let coefficients: Vec<[f64; 2]> = table
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

/// `ggx_energy_coordinates`: an n.v and a roughness as fit coordinates in [0, 1].
pub fn coordinates(n_dot_v: f64, roughness: f64) -> [f64; 2] {
    let low = [MIN_N_DOT_V.cbrt(), MIN_ROUGHNESS.sqrt()];
    let c = [
        n_dot_v.clamp(MIN_N_DOT_V, 1.0).cbrt(),
        roughness.clamp(MIN_ROUGHNESS, 1.0).sqrt(),
    ];
    [0, 1].map(|a| (c[a] - low[a]) / (1.0 - low[a]))
}

/// `ggx_energy_weights`: the uniform cubic B-spline's four weights in a cell, at fraction `f` of it.
fn weights(f: f64) -> [f64; 4] {
    let g = 1.0 - f;
    [
        g * g * g,
        4.0 - 6.0 * f * f + 3.0 * f * f * f,
        4.0 - 6.0 * g * g + 3.0 * g * g * g,
        f * f * f,
    ]
    .map(|w| w / 6.0)
}

/// `ggx_energy_fit`: the `size` x `size` spline of `coefficients` at coordinates `t`, over the 4 x 4 basis functions
/// nonzero there.
pub fn spline(size: usize, coefficients: &[[f64; 2]], t: [f64; 2]) -> [f64; 2] {
    let cells = (size - 3) as f64;
    let x = t.map(|c| c * cells + 1.0);
    let cell = x.map(|c| c.floor().clamp(1.0, cells));
    let (wi, wj) = (weights(x[0] - cell[0]), weights(x[1] - cell[1]));
    let first = (cell[0] as usize - 1) * size + cell[1] as usize - 1;
    let mut out = [0.0; 2];
    for (i, a) in wi.iter().enumerate() {
        let mut row = [0.0; 2];
        for (j, b) in wj.iter().enumerate() {
            let c = coefficients[first + i * size + j];
            row[0] += c[0] * b;
            row[1] += c[1] * b;
        }
        out[0] += row[0] * a;
        out[1] += row[1] * a;
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
