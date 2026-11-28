//! EXT_meshopt_compression: every compressed bufferView decoded into its parent's range before an accessor reads it.

use anyhow::{bail, ensure, Context as _};
use gltf::json::Value;

use crate::meshopt_codec as codec;

pub(crate) const EXTENSION: &str = "EXT_meshopt_compression";
/// All of one file's compressed views, decoded: an all-zero attribute stream inflates 64-fold.
const MAX_DECODED_BYTES: u64 = 1 << 30;
/// The padding a placeholder buffer may hold after each view it backs; gltfpack aligns them to 4 bytes.
const VIEW_ALIGNMENT: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Attributes,
    Triangles,
    Indices,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    None,
    Octahedral,
    Quaternion,
    Exponential,
}

/// A byte range of one buffer.
#[derive(Clone, Copy, Debug)]
struct Range {
    buffer: usize,
    offset: usize,
    length: usize,
}

impl Range {
    fn end(&self) -> Option<usize> {
        self.offset.checked_add(self.length)
    }
}

/// One compressed view: its stream, and the `count` elements of `stride` bytes it decodes into the parent's range.
#[derive(Clone, Debug)]
struct CompressedView {
    view: usize,
    stream: Range,
    target: Range,
    stride: usize,
    count: usize,
    mode: Mode,
    filter: Filter,
}

/// The file's compressed views and the buffers they decode into.
pub(crate) struct Plan {
    views: Vec<CompressedView>,
    /// Tagged `fallback: true`: never read, since every view in it decodes from elsewhere.
    fallback: Vec<bool>,
    /// Written by a compressed view and read by no other view nor stream: a URI-less one is a placeholder.
    backs: Vec<bool>,
}

impl Plan {
    /// Every compressed view, checked against the extension's rules before anything is read or allocated.
    pub(crate) fn new(document: &gltf::Document) -> anyhow::Result<Plan> {
        let buffers = document.buffers().len();
        let mut views = Vec::new();
        let mut decoded: u64 = 0;
        for view in document.views() {
            let Some(json) = view.extension_value(EXTENSION) else {
                continue;
            };
            let compressed = parse(&view, json)
                .with_context(|| format!("bufferViews[{}].extensions.{EXTENSION}", view.index()))?;
            ensure!(
                compressed.stream.buffer < buffers,
                "bufferViews[{}] decodes from buffer {}, which does not exist",
                view.index(),
                compressed.stream.buffer
            );
            decoded = decoded.saturating_add(compressed.target.length as u64);
            ensure!(
                decoded <= MAX_DECODED_BYTES,
                "the compressed views decode to over {MAX_DECODED_BYTES} bytes"
            );
            views.push(compressed);
        }
        let fallback: Vec<bool> = document
            .buffers()
            .map(|buffer| is_fallback(&buffer))
            .collect();
        let mut backs = vec![false; buffers];
        for compressed in &views {
            backs[compressed.target.buffer] = true;
        }
        for compressed in &views {
            let source = compressed.stream.buffer;
            ensure!(
                !fallback[source],
                "bufferViews[{}] decodes from fallback buffer {source}, which holds no data",
                compressed.view
            );
            backs[source] = false;
        }
        for view in document
            .views()
            .filter(|view| view.extension_value(EXTENSION).is_none())
        {
            let buffer = view.buffer().index();
            ensure!(
                !fallback[buffer],
                "bufferViews[{}] reads fallback buffer {buffer} uncompressed",
                view.index()
            );
            backs[buffer] = false;
        }
        Ok(Plan {
            views,
            fallback,
            backs,
        })
    }

    /// A buffer the loader leaves unread: a tagged fallback, whose contents the decoded views replace.
    pub(crate) fn skips(&self, buffer: usize) -> bool {
        self.fallback[buffer]
    }

    /// Whether a URI-less buffer with no GLB chunk left is a placeholder for decoded views rather than an error.
    pub(crate) fn backs(&self, buffer: usize) -> bool {
        self.backs[buffer]
    }

    /// Allocates each placeholder in `placeholders`, then decodes every view into its range of `buffers`.
    pub(crate) fn decode(
        &self,
        document: &gltf::Document,
        buffers: &mut [gltf::buffer::Data],
        placeholders: &[bool],
    ) -> anyhow::Result<()> {
        let lengths: Vec<usize> = document.buffers().map(|buffer| buffer.length()).collect();
        for compressed in &self.views {
            check_ranges(compressed, &lengths, buffers, placeholders)
                .with_context(|| format!("bufferViews[{}]", compressed.view))?;
        }
        for buffer in (0..placeholders.len()).filter(|&buffer| placeholders[buffer]) {
            buffers[buffer].0 = self.placeholder(buffer, lengths[buffer])?;
        }
        for compressed in &self.views {
            decode_in_place(compressed, buffers)
                .with_context(|| format!("decoding bufferViews[{}]", compressed.view))?;
        }
        Ok(())
    }

    /// Zeroes for a placeholder no longer than the views it backs, so a byteLength cannot ask for more than they decode.
    fn placeholder(&self, buffer: usize, length: usize) -> anyhow::Result<Vec<u8>> {
        let mut backing = self
            .views
            .iter()
            .filter(|compressed| compressed.target.buffer == buffer)
            .peekable();
        if backing.peek().is_none() {
            // A fallback no view decodes into: no view reads it either, so it needs no bytes.
            return Ok(Vec::new());
        }
        let backed: usize = backing
            .map(|compressed| compressed.target.length.next_multiple_of(VIEW_ALIGNMENT))
            .sum();
        ensure!(
            length <= backed,
            "placeholder buffer {buffer} declares {length} bytes, more than the {backed} its views decode"
        );
        Ok(vec![0; length.next_multiple_of(4)])
    }
}

fn is_fallback(buffer: &gltf::Buffer) -> bool {
    buffer
        .extension_value(EXTENSION)
        .and_then(|json| json.get("fallback"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The stream lies in its buffer's data and the decoded range in its target's declared length.
fn check_ranges(
    compressed: &CompressedView,
    lengths: &[usize],
    buffers: &[gltf::buffer::Data],
    placeholders: &[bool],
) -> anyhow::Result<()> {
    let stream = compressed.stream;
    ensure!(
        !placeholders[stream.buffer],
        "its stream is in placeholder buffer {}, which holds no data",
        stream.buffer
    );
    ensure!(
        stream.end().is_some_and(
            |end| end <= lengths[stream.buffer] && end <= buffers[stream.buffer].0.len()
        ),
        "its {}-byte stream at offset {} overruns buffer {}",
        stream.length,
        stream.offset,
        stream.buffer
    );
    let target = compressed.target;
    ensure!(
        target
            .end()
            .is_some_and(|end| end <= lengths[target.buffer]),
        "its {} decoded bytes at offset {} overrun buffer {}",
        target.length,
        target.offset,
        target.buffer
    );
    ensure!(
        placeholders[target.buffer]
            || target
                .end()
                .is_some_and(|end| end <= buffers[target.buffer].0.len()),
        "its decoded bytes overrun buffer {}'s data",
        target.buffer
    );
    Ok(())
}

/// Decodes straight into the target range, so the peak stays one copy of the decoded bytes.
fn decode_in_place(
    compressed: &CompressedView,
    buffers: &mut [gltf::buffer::Data],
) -> anyhow::Result<()> {
    let (stream, target) = (compressed.stream, compressed.target);
    let streamed = stream.offset..stream.offset + stream.length;
    let decoded = target.offset..target.offset + target.length;
    if stream.buffer == target.buffer {
        let bytes = buffers[stream.buffer].0[streamed].to_vec();
        return decode_view(compressed, &bytes, &mut buffers[target.buffer].0[decoded]);
    }
    let [source, out] = buffers
        .get_disjoint_mut([stream.buffer, target.buffer])
        .context("no such buffer")?;
    decode_view(compressed, &source.0[streamed], &mut out.0[decoded])
}

fn decode_view(compressed: &CompressedView, stream: &[u8], out: &mut [u8]) -> anyhow::Result<()> {
    let (count, stride) = (compressed.count, compressed.stride);
    match compressed.mode {
        Mode::Attributes => codec::decode_vertex_buffer(out, stride, stream)?,
        Mode::Triangles => codec::decode_index_buffer(out, count, stride, stream)?,
        Mode::Indices => codec::decode_index_sequence(out, count, stride, stream)?,
    }
    match compressed.filter {
        Filter::None => {}
        Filter::Octahedral => codec::filter_octahedral(out, stride)?,
        Filter::Quaternion => codec::filter_quaternion(out),
        Filter::Exponential => codec::filter_exponential(out),
    }
    Ok(())
}

/// A non-negative integer member; `None` when absent, an error when it is anything else.
fn optional_size(json: &Value, name: &str) -> anyhow::Result<Option<usize>> {
    json.get(name)
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .with_context(|| format!("{name} is {value}, not a byte count"))
        })
        .transpose()
}

fn required_size(json: &Value, name: &str) -> anyhow::Result<usize> {
    optional_size(json, name)?.with_context(|| format!("no {name}"))
}

/// The extension object, held to the rules its specification lists for a valid one.
fn parse(view: &gltf::buffer::View, json: &Value) -> anyhow::Result<CompressedView> {
    let mode = match json.get("mode").and_then(Value::as_str) {
        Some("ATTRIBUTES") => Mode::Attributes,
        Some("TRIANGLES") => Mode::Triangles,
        Some("INDICES") => Mode::Indices,
        _ => bail!(
            "mode is {:?}, not ATTRIBUTES, TRIANGLES or INDICES",
            json.get("mode")
        ),
    };
    let filter = match json.get("filter").map(Value::as_str) {
        None | Some(Some("NONE")) => Filter::None,
        Some(Some("OCTAHEDRAL")) => Filter::Octahedral,
        Some(Some("QUATERNION")) => Filter::Quaternion,
        Some(Some("EXPONENTIAL")) => Filter::Exponential,
        Some(_) => bail!(
            "filter is {:?}, not a filter this extension defines",
            json.get("filter")
        ),
    };
    let compressed = CompressedView {
        view: view.index(),
        stream: Range {
            buffer: required_size(json, "buffer")?,
            offset: optional_size(json, "byteOffset")?.unwrap_or(0),
            length: required_size(json, "byteLength")?,
        },
        target: Range {
            buffer: view.buffer().index(),
            offset: view.offset(),
            length: view.length(),
        },
        stride: required_size(json, "byteStride")?,
        count: required_size(json, "count")?,
        mode,
        filter,
    };
    check_layout(&compressed, view.stride())?;
    Ok(compressed)
}

/// The specification's validity rules, then the shortest stream that could hold the view.
fn check_layout(compressed: &CompressedView, parent_stride: Option<usize>) -> anyhow::Result<()> {
    let CompressedView {
        stride,
        count,
        mode,
        filter,
        ..
    } = *compressed;
    ensure!(
        parent_stride.is_none_or(|parent| parent == stride),
        "byteStride {stride} differs from the bufferView's {parent_stride:?}"
    );
    ensure!(
        stride.checked_mul(count) == Some(compressed.target.length),
        "{count} elements of {stride} bytes do not fill the bufferView's {} bytes",
        compressed.target.length
    );
    let fits = match mode {
        Mode::Attributes => stride > 0 && stride <= 256 && stride.is_multiple_of(4),
        Mode::Triangles => count.is_multiple_of(3) && (stride == 2 || stride == 4),
        Mode::Indices => stride == 2 || stride == 4,
    };
    ensure!(
        fits,
        "{mode:?} cannot hold {count} elements of {stride} bytes"
    );
    let filtered = match filter {
        Filter::None => true,
        Filter::Octahedral => mode == Mode::Attributes && (stride == 4 || stride == 8),
        Filter::Quaternion => mode == Mode::Attributes && stride == 8,
        Filter::Exponential => mode == Mode::Attributes && stride.is_multiple_of(4),
    };
    ensure!(
        filtered,
        "the {filter:?} filter cannot apply to {mode:?} of {stride} bytes"
    );
    let shortest = match mode {
        Mode::Attributes => codec::min_vertex_stream_bytes(count, stride),
        Mode::Triangles => (count / 3).checked_add(17),
        Mode::Indices => count.checked_add(5),
    };
    ensure!(
        shortest.is_some_and(|shortest| compressed.stream.length >= shortest),
        "a {}-byte stream cannot hold {count} elements of {stride} bytes",
        compressed.stream.length
    );
    Ok(())
}
