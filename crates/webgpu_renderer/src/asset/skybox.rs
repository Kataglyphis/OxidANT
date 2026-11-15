//! The C++ engine's cube-map skybox: six PNG faces in BeschleunigerBallett's `Resources/Textures/Skybox/DOOM2016`.

use anyhow::Context as _;

/// The C++ `SkyBox::loadCubeMap` files, in a cube texture's +X, -X, +Y, -Y, +Z, -Z layer order.
pub const DOOM2016_FACES: [&str; 6] = [
    "DOOM16RT.png",
    "DOOM16LF.png",
    "DOOM16UP.png",
    "DOOM16DN.png",
    "DOOM16FT.png",
    "DOOM16BK.png",
];
/// Where a BeschleunigerBallett checkout keeps them, from its root.
pub const DOOM2016_DIR: &str = "Resources/Textures/Skybox/DOOM2016";

/// Six square sRGB RGBA8 faces, one cube layer each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CubemapFaces {
    /// Texels per side of every face.
    pub size: u32,
    /// The six faces back to back, each row-major.
    pub rgba8: Vec<u8>,
}

impl CubemapFaces {
    /// One texel per face.
    pub fn solid(colors: [[u8; 4]; 6]) -> Self {
        Self {
            size: 1,
            rgba8: colors.concat(),
        }
    }

    /// Decodes six image files; every face must be the same square, which the C++ upload requires too.
    pub fn decode(files: [&[u8]; 6]) -> anyhow::Result<Self> {
        let mut size = None;
        let mut rgba8 = Vec::new();
        for (face, bytes) in files.iter().enumerate() {
            let image = image::load_from_memory(bytes)
                .with_context(|| format!("skybox face {} does not decode", DOOM2016_FACES[face]))?
                .to_rgba8();
            let (width, height) = image.dimensions();
            anyhow::ensure!(
                width == height && width > 0,
                "skybox face {} is {width}x{height}, not a square",
                DOOM2016_FACES[face]
            );
            let first = *size.get_or_insert(width);
            anyhow::ensure!(
                first == width,
                "skybox face {} has {width} texels a side, the first {first}",
                DOOM2016_FACES[face]
            );
            rgba8.extend_from_slice(image.as_raw());
        }
        Ok(Self {
            size: size.unwrap_or(1),
            rgba8,
        })
    }

    /// Reads [`DOOM2016_FACES`] from `dir`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_dir(dir: &std::path::Path) -> anyhow::Result<Self> {
        let mut files = Vec::with_capacity(DOOM2016_FACES.len());
        for name in DOOM2016_FACES {
            let path = dir.join(name);
            let bytes =
                std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
            files.push(bytes);
        }
        Self::decode(std::array::from_fn(|face| files[face].as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(size: u32, rgba: [u8; 4]) -> Vec<u8> {
        let pixels: Vec<u8> = (0..size * size).flat_map(|_| rgba).collect();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_raw(size, size, pixels)
            .expect("the pixels fill the image")
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("PNG encodes");
        bytes.into_inner()
    }

    #[test]
    fn six_equal_squares_decode_in_layer_order_and_anything_else_is_refused() {
        let faces: Vec<Vec<u8>> = (0..6u8)
            .map(|face| png(2, [face * 40, 0, 0, 255]))
            .collect();
        let files: [&[u8]; 6] = std::array::from_fn(|face| faces[face].as_slice());
        let cube = CubemapFaces::decode(files).expect("six 2x2 faces decode");
        assert_eq!(cube.size, 2);
        assert_eq!(cube.rgba8.len(), 6 * 2 * 2 * 4);
        assert_eq!(cube.rgba8[5 * 16], 200, "layer 5 is the sixth file, BK");
        let wide = png(4, [0; 4]);
        let mut mixed = files;
        mixed[3] = &wide;
        let error = CubemapFaces::decode(mixed).unwrap_err().to_string();
        assert!(error.contains("DOOM16DN.png"), "{error}");
        let mut broken = files;
        broken[0] = b"not a png";
        assert!(CubemapFaces::decode(broken).is_err());
    }

    #[test]
    fn a_folder_missing_a_face_names_it() {
        let dir = std::env::temp_dir().join(format!("skybox-faces-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        for name in &DOOM2016_FACES[..5] {
            std::fs::write(dir.join(name), png(1, [9, 9, 9, 255])).expect("face written");
        }
        let error = format!("{:#}", CubemapFaces::load_dir(&dir).unwrap_err());
        assert!(error.contains("DOOM16BK.png"), "{error}");
        std::fs::write(dir.join(DOOM2016_FACES[5]), png(1, [9, 9, 9, 255])).expect("face written");
        assert_eq!(CubemapFaces::load_dir(&dir).expect("six faces").size, 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
