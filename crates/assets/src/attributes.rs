//! Vertex attributes as f32, from every component type core glTF and KHR_mesh_quantization allow.

use gltf::accessor::{DataType, Item, Iter};

/// An accessor's elements as f32: a `normalized` integer by glTF's rule, any other integer as its value.
/// `None` when the accessor has nothing to read, as gltf's own readers answer.
pub(crate) fn read_f32<const N: usize>(
    accessor: gltf::Accessor,
    buffers: &[gltf::buffer::Data],
) -> Option<Vec<[f32; N]>>
where
    [f32; N]: Item,
    [i8; N]: Item,
    [u8; N]: Item,
    [i16; N]: Item,
    [u16; N]: Item,
{
    let normalized = accessor.normalized();
    match accessor.data_type() {
        DataType::F32 => elements(accessor, buffers, |c: f32| c),
        DataType::I8 => elements(accessor, buffers, |c: i8| {
            decoded(f32::from(c), 127.0, normalized)
        }),
        DataType::U8 => elements(accessor, buffers, |c: u8| {
            decoded(f32::from(c), 255.0, normalized)
        }),
        DataType::I16 => elements(accessor, buffers, |c: i16| {
            decoded(f32::from(c), 32767.0, normalized)
        }),
        DataType::U16 => elements(accessor, buffers, |c: u16| {
            decoded(f32::from(c), 65535.0, normalized)
        }),
        DataType::U32 => None,
    }
}

/// glTF's normalized-integer rule: the value over the type's maximum, the signed minimum clamped to -1.
fn decoded(value: f32, max: f32, normalized: bool) -> f32 {
    if normalized {
        (value / max).max(-1.0)
    } else {
        value
    }
}

fn elements<T: Copy, const N: usize>(
    accessor: gltf::Accessor,
    buffers: &[gltf::buffer::Data],
    component: impl Fn(T) -> f32,
) -> Option<Vec<[f32; N]>>
where
    [T; N]: Item,
{
    let iter = Iter::<[T; N]>::new(accessor, |buffer: gltf::Buffer| {
        buffers.get(buffer.index()).map(|data| &data.0[..])
    })?;
    Some(iter.map(|element| element.map(&component)).collect())
}
