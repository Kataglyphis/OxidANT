//! The glTF 2.0 accessor rules gltf 1.4.1's reader assumes but never checks: it panics on a file that breaks them.

use anyhow::{ensure, Context};
use gltf::accessor::{DataType, Dimensions};
use gltf::mesh::Semantic;

/// Every accessor a primitive reads has a format its semantic allows, at least one element, and fits its view.
pub(crate) fn check_primitive_accessors(document: &gltf::Document) -> anyhow::Result<()> {
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let at = || format!("meshes[{}].primitives[{}]", mesh.index(), primitive.index());
            for (semantic, accessor) in primitive.attributes() {
                if let Some(allowed) = attribute_formats(&semantic) {
                    check(&accessor, allowed).with_context(|| format!("{}.{semantic:?}", at()))?;
                }
            }
            if let Some(indices) = primitive.indices() {
                check(&indices, INDICES).with_context(|| format!("{}.indices", at()))?;
            }
            for target in primitive.morph_targets() {
                for accessor in [target.positions(), target.normals(), target.tangents()]
                    .into_iter()
                    .flatten()
                {
                    check(&accessor, VEC3_F32).with_context(|| format!("{} morph target", at()))?;
                }
            }
        }
    }
    Ok(())
}

type Formats = &'static [(DataType, Dimensions)];

const VEC3_F32: Formats = &[(DataType::F32, Dimensions::Vec3)];
const VEC4_F32: Formats = &[(DataType::F32, Dimensions::Vec4)];
const INDICES: Formats = &[
    (DataType::U8, Dimensions::Scalar),
    (DataType::U16, Dimensions::Scalar),
    (DataType::U32, Dimensions::Scalar),
];
const TEX_COORDS: Formats = &[
    (DataType::F32, Dimensions::Vec2),
    (DataType::U8, Dimensions::Vec2),
    (DataType::U16, Dimensions::Vec2),
];
const COLORS: Formats = &[
    (DataType::F32, Dimensions::Vec3),
    (DataType::U8, Dimensions::Vec3),
    (DataType::U16, Dimensions::Vec3),
    (DataType::F32, Dimensions::Vec4),
    (DataType::U8, Dimensions::Vec4),
    (DataType::U16, Dimensions::Vec4),
];
const JOINTS: Formats = &[
    (DataType::U8, Dimensions::Vec4),
    (DataType::U16, Dimensions::Vec4),
];
const WEIGHTS: Formats = &[
    (DataType::F32, Dimensions::Vec4),
    (DataType::U8, Dimensions::Vec4),
    (DataType::U16, Dimensions::Vec4),
];

/// The formats the reader's match arms cover; an application-specific attribute is never read.
fn attribute_formats(semantic: &Semantic) -> Option<Formats> {
    match semantic {
        Semantic::Positions | Semantic::Normals => Some(VEC3_F32),
        Semantic::Tangents => Some(VEC4_F32),
        Semantic::TexCoords(_) => Some(TEX_COORDS),
        Semantic::Colors(_) => Some(COLORS),
        Semantic::Joints(_) => Some(JOINTS),
        Semantic::Weights(_) => Some(WEIGHTS),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn check(accessor: &gltf::Accessor, allowed: Formats) -> anyhow::Result<()> {
    let format = (accessor.data_type(), accessor.dimensions());
    ensure!(
        allowed.contains(&format),
        "accessor {} is {format:?}, which this attribute cannot be",
        accessor.index()
    );
    ensure!(
        accessor.count() > 0,
        "accessor {} has no elements",
        accessor.index()
    );
    let size = accessor.size();
    if let Some(view) = accessor.view() {
        fits(&view, accessor.offset(), accessor.count(), size)
            .with_context(|| format!("accessor {}", accessor.index()))?;
    }
    if let Some(sparse) = accessor.sparse() {
        let count = sparse.count();
        ensure!(
            count > 0,
            "accessor {}'s sparse block has no elements",
            accessor.index()
        );
        let indices = sparse.indices();
        fits(
            &indices.view(),
            indices.offset(),
            count,
            indices.index_type().size(),
        )
        .with_context(|| format!("accessor {}'s sparse indices", accessor.index()))?;
        let values = sparse.values();
        fits(&values.view(), values.offset(), count, size)
            .with_context(|| format!("accessor {}'s sparse values", accessor.index()))?;
    }
    Ok(())
}

/// `count` elements of `size` bytes from `offset`, at the view's stride, end inside the view.
fn fits(view: &gltf::buffer::View, offset: usize, count: usize, size: usize) -> anyhow::Result<()> {
    let stride = view.stride().unwrap_or(size);
    ensure!(
        stride >= size,
        "stride {stride} is below the element size {size}"
    );
    let end = stride
        .checked_mul(count - 1)
        .and_then(|span| span.checked_add(offset))
        .and_then(|start| start.checked_add(size));
    ensure!(
        end.is_some_and(|end| end <= view.length()),
        "{count} elements of {size} bytes at offset {offset} overrun buffer view {} ({} bytes)",
        view.index(),
        view.length()
    );
    Ok(())
}
