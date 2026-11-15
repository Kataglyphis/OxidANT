//! Image references the glTF importer matches by exact lowercase extension or MIME type.

use kataglyphis_webgpu_renderer::asset::obj_to_gltf::convert_file;
use kataglyphis_webgpu_renderer::load_gltf;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kataglyphis_image_formats_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// A 2x2 PNG whose first texel is red.
fn write_png(path: &std::path::Path) {
    let mut image = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]));
    image.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    image
        .save_with_format(path, image::ImageFormat::Png)
        .expect("PNG");
}

const QUAD_OBJ: &str = "mtllib quad.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvn 0 0 1\nusemtl painted\nf 1/1/1 2/2/1 3/3/1\n";

/// An MTL naming `texture`, beside the quad OBJ in `dir`; returns the OBJ's path.
fn quad_obj_with(dir: &std::path::Path, texture: &str) -> std::path::PathBuf {
    std::fs::write(
        dir.join("quad.mtl"),
        format!("newmtl painted\nKd 1 1 1\nmap_Kd {texture}\n"),
    )
    .expect("mtl");
    let obj = dir.join("quad.obj");
    std::fs::write(&obj, QUAD_OBJ).expect("obj");
    obj
}

/// Windows exporters write `Wall.JPG`; the importer only knows lowercase `png`, `jpg` and `jpeg`.
#[test]
fn a_gltf_uri_with_an_uppercase_extension_loads() {
    let dir = scratch("uppercase_uri");
    write_png(&dir.join("tex.PNG"));
    let positions: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    std::fs::write(dir.join("tri.bin"), &positions).expect("bin");
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"uri":"tri.bin","byteLength":36}],
        "bufferViews":[{"buffer":0,"byteLength":36}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
        "images":[{"uri":"tex.PNG"}],"textures":[{"source":0}],
        "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
        "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;
    std::fs::write(dir.join("tri.gltf"), json).expect("gltf");
    let scene = load_gltf(dir.join("tri.gltf")).expect("tex.PNG must load");
    let texture = scene.primitives[0]
        .material
        .base_color_texture
        .as_ref()
        .expect("texture");
    assert_eq!(&texture.texture.rgba8[0..4], &[255, 0, 0, 255]);
}

#[test]
fn an_obj_with_an_uppercase_texture_name_converts_to_a_loadable_gltf() {
    let dir = scratch("uppercase_obj");
    write_png(&dir.join("PAINT.PNG"));
    let obj = quad_obj_with(&dir, "PAINT.PNG");
    let gltf = dir.join("out").join("quad.gltf");
    convert_file(&obj, &gltf).expect("conversion");
    let scene = load_gltf(&gltf).expect("the converted glTF must load");
    assert!(scene.primitives[0].material.base_color_texture.is_some());
}

/// The module promises to fail loudly; a .tga map converted fine and then failed the whole load.
#[test]
fn an_obj_texture_gltf_cannot_carry_fails_the_conversion() {
    let dir = scratch("tga_obj");
    std::fs::write(dir.join("stone.tga"), b"not decoded anyway").expect("tga");
    let obj = quad_obj_with(&dir, "stone.tga");
    let error = convert_file(&obj, &dir.join("out").join("quad.gltf"))
        .expect_err("a TGA map must fail the conversion");
    assert!(
        format!("{error:#}").contains("stone.tga"),
        "the error must name the texture: {error:#}"
    );
}
