use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Context;
use plotters::coord::Shift;
use plotters::prelude::*;
use plotters::style::{register_font, FontStyle};

/// Convert any plotters `DrawingAreaErrorKind` into an `anyhow::Error`.
fn plotters_err<E: std::fmt::Debug>(e: E) -> anyhow::Error {
    anyhow::anyhow!("plotters error: {e:?}")
}

pub fn plot_loss_curve(path: impl AsRef<Path>, losses: &[f32], title: &str) -> anyhow::Result<()> {
    if losses.is_empty() {
        anyhow::bail!("cannot plot empty loss series");
    }

    let path = path.as_ref();
    ensure_parent_dir(path)?;
    ensure_font()?;

    render_loss_curve(path, losses, title)
}

/// Registers this system's sans-serif TrueType font as plotters' `sans-serif`, once per process.
fn ensure_font() -> anyhow::Result<()> {
    static FONT: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    FONT.get_or_init(|| {
        let candidates = font_candidates();
        let path = candidates.iter().find(|p| p.is_file()).ok_or_else(|| {
            format!("no TrueType font for the plot's text; looked for {candidates:?}")
        })?;
        let bytes =
            std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        // plotters keeps fonts for the process' lifetime, so the bytes are leaked on purpose.
        register_font(
            "sans-serif",
            FontStyle::Normal,
            Box::leak(bytes.into_boxed_slice()),
        )
        .map_err(|_| format!("{} is not a font plotters can read", path.display()))?;
        Ok(path.clone())
    })
    .as_ref()
    .map(|_| ())
    .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Where a sans-serif TrueType font sits: fontconfig's own answer first, then the usual files.
fn font_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(out) = std::process::Command::new("fc-match")
        .args(["--format=%{file}", "sans-serif:fontformat=TrueType"])
        .output()
    {
        if out.status.success() && !out.stdout.is_empty() {
            candidates.push(PathBuf::from(
                String::from_utf8_lossy(&out.stdout).into_owned(),
            ));
        }
    }
    if cfg!(windows) {
        let fonts = Path::new(&std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
            .join("Fonts");
        // Lucida Console last: the only font a Server Core container ships.
        candidates.extend(
            ["arial.ttf", "segoeui.ttf", "calibri.ttf", "lucon.ttf"].map(|f| fonts.join(f)),
        );
    } else {
        candidates.extend(
            [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",
                "/System/Library/Fonts/Supplemental/Arial.ttf",
            ]
            .map(PathBuf::from),
        );
    }
    candidates
}

fn render_loss_curve(path: &Path, losses: &[f32], title: &str) -> anyhow::Result<()> {
    let (width, height) = (1280, 720);
    let root = BitMapBackend::new(path, (width, height)).into_drawing_area();

    fill_white(&root)?;
    let (x_max, y_min, y_max) = chart_bounds(losses);

    let mut chart = ChartBuilder::on(&root)
        .caption(title, ("sans-serif", 36))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0i32..x_max, y_min..y_max)
        .map_err(plotters_err)?;

    chart
        .configure_mesh()
        .x_desc("epoch")
        .y_desc("loss")
        .draw()
        .map_err(plotters_err)?;

    chart
        .draw_series(LineSeries::new(
            losses.iter().enumerate().map(|(i, y)| (i as i32, *y)),
            &RED,
        ))
        .map_err(plotters_err)?
        .label("loss")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED));

    chart
        .configure_series_labels()
        .border_style(BLACK)
        .background_style(WHITE.mix(0.9))
        .draw()
        .map_err(plotters_err)?;

    present(&root)?;

    Ok(())
}

fn fill_white(area: &DrawingArea<BitMapBackend<'_>, Shift>) -> anyhow::Result<()> {
    area.fill(&WHITE).map_err(plotters_err)
}

fn chart_bounds(losses: &[f32]) -> (i32, f32, f32) {
    let x_max = (losses.len() - 1) as i32;
    let (min_y, max_y) = min_max(losses);
    let y_pad = ((max_y - min_y).abs() * 0.1).max(1e-6);
    (x_max, min_y - y_pad, max_y + y_pad)
}

fn present(area: &DrawingArea<BitMapBackend<'_>, Shift>) -> anyhow::Result<()> {
    area.present().map_err(plotters_err)
}

fn ensure_parent_dir(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create output directory {}", parent.display()))?;
        }
    }
    Ok(())
}

fn min_max(values: &[f32]) -> (f32, f32) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for &v in values {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    (min, max)
}
