//! The one Wavefront OBJ reader both renderers load through: tolerant like tinyobjloader (a bad line is skipped
//! and reported), one mesh per `usemtl` run, and one set of material rules (owner, 2026-11-19).

mod flat;
mod glb;
mod gltf_json;
mod material;
mod parse;
mod paths;
mod triangulate;

pub use flat::load_obj_flat;
pub use glb::obj_to_glb;
pub use gltf_json::{convert_file, to_gltf};
pub use parse::{parse_mtl, parse_obj, parse_obj_with_materials, read_obj, ObjMaterial, ObjMesh};
pub use paths::texture_source;

/// Whether `path` names an OBJ by its extension, in any case.
pub fn is_obj_path(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"))
}
