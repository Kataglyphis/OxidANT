//! EXT_meshopt_compression's bitstreams and filters, read as meshoptimizer's scalar decoder reads them.

use anyhow::{bail, ensure, Context as _};

const VERTEX_HEADER: u8 = 0xa0;
const TRIANGLES_HEADER: u8 = 0xe0;
const SEQUENCE_HEADER: u8 = 0xd0;
/// The highest index codec version this decoder reads; meshoptimizer's own limit.
const INDEX_VERSION: u8 = 1;
const BLOCK_BYTES: usize = 8192;
const BLOCK_MAX_ELEMENTS: usize = 256;
const GROUP: usize = 16;
/// Version 0's tail: the baseline element, padded to this many bytes.
const TAIL_MIN: usize = 32;
/// The 16-byte `codeaux` table that ends a triangle stream.
const CODEAUX_BYTES: usize = 16;
/// The reserved bytes that end an index sequence.
const SEQUENCE_TAIL: usize = 4;
/// Bits per delta for each 2-bit group header.
const GROUP_BITS: [usize; 4] = [0, 2, 4, 8];

/// The bytes of one stream, read forward; `None` once a read would pass its end.
struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let bytes = self.data.get(self.at..self.at.checked_add(count)?)?;
        self.at += count;
        Some(bytes)
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    /// Unsigned LEB128 in at most five bytes; the fifth ends the value whatever its top bit says.
    fn varint(&mut self) -> Option<u32> {
        let lead = self.byte()?;
        let mut value = u32::from(lead & 127);
        if lead < 128 {
            return Some(value);
        }
        for shift in [7, 14, 21, 28] {
            let group = self.byte()?;
            value |= u32::from(group & 127) << shift;
            if group < 128 {
                break;
            }
        }
        Some(value)
    }
}

/// Elements per attribute block: what fits 8 KiB, in whole groups, at most 256.
pub fn block_elements(stride: usize) -> usize {
    ((BLOCK_BYTES / stride) & !(GROUP - 1)).min(BLOCK_MAX_ELEMENTS)
}

/// The shortest ATTRIBUTES stream that holds `count` elements of `stride` bytes: every group all zero.
pub fn min_vertex_stream_bytes(count: usize, stride: usize) -> Option<usize> {
    let block = block_elements(stride);
    let header = |elements: usize| elements.div_ceil(GROUP).div_ceil(4);
    let rest = count % block;
    let headers = (count / block)
        .checked_mul(header(block))?
        .checked_add(if rest == 0 { 0 } else { header(rest) })?;
    headers
        .checked_mul(stride)?
        .checked_add(1 + stride.max(TAIL_MIN))
}

fn unzigzag8(value: u8) -> u8 {
    (value >> 1) ^ 0u8.wrapping_sub(value & 1)
}

fn unzigzag32(value: u32) -> u32 {
    (value >> 1) ^ 0u32.wrapping_sub(value & 1)
}

/// Mode 0: `out` gets `out.len() / stride` elements of `stride` bytes, a multiple of 4 up to 256, from `data`.
pub fn decode_vertex_buffer(out: &mut [u8], stride: usize, data: &[u8]) -> anyhow::Result<()> {
    ensure!(
        stride > 0 && stride <= 256 && stride.is_multiple_of(4),
        "an attribute stride of {stride} bytes, not a multiple of 4 up to 256"
    );
    ensure!(
        out.len().is_multiple_of(stride),
        "{} bytes are no whole number of {stride}-byte elements",
        out.len()
    );
    match data.first() {
        Some(&VERTEX_HEADER) => {}
        Some(&header) => bail!("attribute header {header:#04x}, not version 0's 0xa0"),
        None => bail!("an empty attribute stream"),
    }
    let tail = stride.max(TAIL_MIN);
    ensure!(
        data.len() > tail,
        "{} bytes cannot hold a {tail}-byte tail",
        data.len()
    );
    let body_end = data.len() - tail;
    let mut last = [0u8; 256];
    last[..stride].copy_from_slice(&data[data.len() - stride..]);
    let mut reader = Bytes {
        data: &data[..body_end],
        at: 1,
    };
    let mut deltas = [0u8; BLOCK_MAX_ELEMENTS];
    for (index, block) in out.chunks_mut(block_elements(stride) * stride).enumerate() {
        let elements = block.len() / stride;
        let groups = &mut deltas[..elements.div_ceil(GROUP) * GROUP];
        for (k, previous) in last[..stride].iter_mut().enumerate() {
            decode_byte_column(&mut reader, groups)
                .with_context(|| format!("attribute block {index} ends early at byte {k}"))?;
            for (element, &delta) in groups[..elements].iter().enumerate() {
                *previous = previous.wrapping_add(unzigzag8(delta));
                block[element * stride + k] = *previous;
            }
        }
    }
    ensure!(
        reader.at == body_end,
        "{} bytes sit between the last attribute block and the tail",
        body_end - reader.at
    );
    Ok(())
}

/// One byte of every element in a block: the group headers, then each group's deltas.
fn decode_byte_column(reader: &mut Bytes<'_>, deltas: &mut [u8]) -> anyhow::Result<()> {
    let groups = deltas.len() / GROUP;
    let header = reader
        .take(groups.div_ceil(4))
        .context("no room for the group headers")?;
    for (group, out) in deltas.as_chunks_mut::<GROUP>().0.iter_mut().enumerate() {
        let mode = (header[group / 4] >> ((group % 4) * 2)) & 3;
        decode_group(reader, out, GROUP_BITS[usize::from(mode)])
            .with_context(|| format!("group {group} ends early"))?;
    }
    Ok(())
}

/// Sixteen deltas of `bits` each, most significant first; an all-ones value is a sentinel for a following full byte.
fn decode_group(reader: &mut Bytes<'_>, out: &mut [u8], bits: usize) -> Option<()> {
    match bits {
        0 => out.fill(0),
        8 => out.copy_from_slice(reader.take(GROUP)?),
        _ => {
            let packed = reader.take(GROUP * bits / 8)?;
            let sentinel = (1u8 << bits) - 1;
            for (i, delta) in out.iter_mut().enumerate() {
                let shift = 8 - bits - (i * bits) % 8;
                let value = (packed[i * bits / 8] >> shift) & sentinel;
                *delta = if value == sentinel {
                    reader.byte()?
                } else {
                    value
                };
            }
        }
    }
    Some(())
}

/// The index codecs' version, `None` for a header of another mode or a newer version.
fn index_version(data: &[u8], header: u8) -> Option<u8> {
    let first = *data.first()?;
    (first & 0xf0 == header && first & 0x0f <= INDEX_VERSION).then_some(first & 0x0f)
}

/// Little-endian u16 or u32 indices, one after another.
struct IndexWriter<'a> {
    out: &'a mut [u8],
    size: usize,
    at: usize,
}

impl IndexWriter<'_> {
    fn new(out: &mut [u8], count: usize, size: usize) -> anyhow::Result<IndexWriter<'_>> {
        ensure!(
            size == 2 || size == 4,
            "an index stride of {size} bytes, not 2 or 4"
        );
        ensure!(
            count.checked_mul(size) == Some(out.len()),
            "{} bytes for {count} indices of {size} bytes",
            out.len()
        );
        Ok(IndexWriter { out, size, at: 0 })
    }

    fn push(&mut self, index: u32) {
        let bytes = index.to_le_bytes();
        self.out[self.at..self.at + self.size].copy_from_slice(&bytes[..self.size]);
        self.at += self.size;
    }
}

/// The decoder's recent vertices and edges; a FIFO slot is written even when its push does not advance it.
struct Fifos {
    edges: [[u32; 2]; 16],
    edge_at: usize,
    vertices: [u32; 16],
    vertex_at: usize,
}

impl Fifos {
    fn edge(&self, back: usize) -> [u32; 2] {
        self.edges[self.edge_at.wrapping_sub(1 + back) & 15]
    }

    /// The vertex `back` pushes ago; `vertex(0)` is the newest.
    fn vertex(&self, back: usize) -> u32 {
        self.vertices[self.vertex_at.wrapping_sub(1 + back) & 15]
    }

    fn push_edge(&mut self, a: u32, b: u32) {
        self.edges[self.edge_at] = [a, b];
        self.edge_at = (self.edge_at + 1) & 15;
    }

    fn push_vertex(&mut self, v: u32, advance: bool) {
        self.vertices[self.vertex_at] = v;
        self.vertex_at = (self.vertex_at + usize::from(advance)) & 15;
    }
}

/// The triangle decoder's counters: `next` the high-watermark, `last` the delta base of explicit indices.
struct Triangles<'a, 'b> {
    fifos: Fifos,
    next: u32,
    last: u32,
    extra: Bytes<'a>,
    codeaux: &'a [u8],
    out: IndexWriter<'b>,
}

impl Triangles<'_, '_> {
    fn next_index(&mut self) -> u32 {
        let index = self.next;
        self.next = self.next.wrapping_add(1);
        index
    }

    fn explicit_index(&mut self) -> Option<u32> {
        self.last = self.last.wrapping_add(unzigzag32(self.extra.varint()?));
        Some(self.last)
    }

    /// `0xXY`, X < 15: edge X of the FIFO, then the next vertex, a recent one, `last` -/+ 1 or an explicit one.
    fn edge_code(&mut self, code: u8, max_recent: usize) -> Option<()> {
        let [a, b] = self.fifos.edge(usize::from(code >> 4));
        let fec = usize::from(code & 15);
        let c = if fec < max_recent {
            let c = if fec == 0 {
                self.next_index()
            } else {
                self.fifos.vertex(fec)
            };
            self.fifos.push_vertex(c, fec == 0);
            c
        } else {
            self.last = match fec {
                13 => self.last.wrapping_sub(1),
                14 => self.last.wrapping_add(1),
                _ => self.explicit_index()?,
            };
            self.fifos.push_vertex(self.last, true);
            self.last
        };
        self.fifos.push_edge(c, b);
        self.fifos.push_edge(a, c);
        self.emit(a, b, c);
        Some(())
    }

    /// `0xfY`: three vertices, each the next one, a recent one or (from `data`, `0xfe` and `0xff`) an explicit one.
    fn fan_code(&mut self, code: u8) -> Option<()> {
        let (explicit, aux) = if code < 0xfe {
            (false, self.codeaux[usize::from(code & 15)])
        } else {
            (true, self.extra.byte()?)
        };
        if explicit && aux == 0 {
            self.next = 0;
        }
        let (feb, fec) = (usize::from(aux >> 4), usize::from(aux & 15));
        let explicit_a = code == 0xff;
        let mut a = if explicit_a { 0 } else { self.next_index() };
        let recent =
            |fifos: &Fifos, fe: usize| fifos.vertices[fifos.vertex_at.wrapping_sub(fe) & 15];
        let mut b = if feb == 0 {
            self.next_index()
        } else {
            recent(&self.fifos, feb)
        };
        let mut c = if fec == 0 {
            self.next_index()
        } else {
            recent(&self.fifos, fec)
        };
        if explicit_a {
            a = self.explicit_index()?;
        }
        if explicit && feb == 15 {
            b = self.explicit_index()?;
        }
        if explicit && fec == 15 {
            c = self.explicit_index()?;
        }
        self.emit(a, b, c);
        self.fifos.push_vertex(a, true);
        self.fifos
            .push_vertex(b, feb == 0 || (explicit && feb == 15));
        self.fifos
            .push_vertex(c, fec == 0 || (explicit && fec == 15));
        self.fifos.push_edge(b, a);
        self.fifos.push_edge(c, b);
        self.fifos.push_edge(a, c);
        Some(())
    }

    fn emit(&mut self, a: u32, b: u32, c: u32) {
        self.out.push(a);
        self.out.push(b);
        self.out.push(c);
    }
}

/// Mode 1: `count` triangle-list indices of `size` bytes (2 or 4) into `out`, from `data`.
pub fn decode_index_buffer(
    out: &mut [u8],
    count: usize,
    size: usize,
    data: &[u8],
) -> anyhow::Result<()> {
    ensure!(
        count.is_multiple_of(3),
        "{count} indices are no whole triangles"
    );
    let out = IndexWriter::new(out, count, size)?;
    let triangles = count / 3;
    ensure!(
        data.len() >= 1 + triangles + CODEAUX_BYTES,
        "{} bytes cannot hold {triangles} triangle codes and the codeaux table",
        data.len()
    );
    let version = index_version(data, TRIANGLES_HEADER)
        .with_context(|| format!("triangle header {:#04x}, not 0xe0 or 0xe1", data[0]))?;
    let table_at = data.len() - CODEAUX_BYTES;
    let mut decoder = Triangles {
        fifos: Fifos {
            edges: [[u32::MAX; 2]; 16],
            edge_at: 0,
            vertices: [u32::MAX; 16],
            vertex_at: 0,
        },
        next: 0,
        last: 0,
        extra: Bytes {
            data: &data[..table_at],
            at: 1 + triangles,
        },
        codeaux: &data[table_at..],
        out,
    };
    // Version 0 reads codes 0xXd and 0xXe as recent vertices too.
    let max_recent = if version >= 1 { 13 } else { 15 };
    for (triangle, &code) in data[1..1 + triangles].iter().enumerate() {
        let decoded = if code < 0xf0 {
            decoder.edge_code(code, max_recent)
        } else {
            decoder.fan_code(code)
        };
        decoded.with_context(|| format!("triangle {triangle} reads past the stream's data"))?;
    }
    ensure!(
        decoder.extra.at == table_at,
        "{} bytes sit between the last triangle's data and the codeaux table",
        table_at - decoder.extra.at
    );
    Ok(())
}

/// Mode 2: `count` indices of `size` bytes (2 or 4) into `out`, each a delta from one of two baselines.
pub fn decode_index_sequence(
    out: &mut [u8],
    count: usize,
    size: usize,
    data: &[u8],
) -> anyhow::Result<()> {
    let mut out = IndexWriter::new(out, count, size)?;
    ensure!(
        data.len() >= 1 + count + SEQUENCE_TAIL,
        "{} bytes cannot hold {count} indices and the tail",
        data.len()
    );
    index_version(data, SEQUENCE_HEADER)
        .with_context(|| format!("index sequence header {:#04x}, not 0xd0 or 0xd1", data[0]))?;
    let end = data.len() - SEQUENCE_TAIL;
    let mut reader = Bytes {
        data: &data[..end],
        at: 1,
    };
    let mut last = [0u32; 2];
    for index in 0..count {
        let value = reader
            .varint()
            .with_context(|| format!("index {index} reads past the stream's data"))?;
        let baseline = &mut last[(value & 1) as usize];
        *baseline = baseline.wrapping_add(unzigzag32(value >> 1));
        out.push(*baseline);
    }
    ensure!(
        reader.at == end,
        "{} bytes sit between the last index and the tail",
        end - reader.at
    );
    Ok(())
}

/// Rounds half away from zero, then truncates as C's `int(float)` does; NaN is 0.
fn round_to_int(value: f32) -> i32 {
    (value + if value >= 0.0 { 0.5 } else { -0.5 }) as i32
}

/// The unit vector an octahedral (x, y) encodes, with `z` the same K-bit 1.0, scaled to `max`.
fn octahedral([x, y, one]: [f32; 3], max: f32) -> [i32; 3] {
    let z = one - x.abs() - y.abs();
    let t = if z >= 0.0 { 0.0 } else { z };
    let x = x + if x >= 0.0 { t } else { -t };
    let y = y + if y >= 0.0 { t } else { -t };
    let scale = max / (x * x + y * y + z * z).sqrt();
    [
        round_to_int(x * scale),
        round_to_int(y * scale),
        round_to_int(z * scale),
    ]
}

/// Filter 1, in place: four i8 (stride 4) or i16 (stride 8) components per element, the fourth passed through.
pub fn filter_octahedral(data: &mut [u8], stride: usize) -> anyhow::Result<()> {
    match stride {
        4 => {
            for element in data.as_chunks_mut::<4>().0 {
                let component = |i: usize| f32::from(element[i] as i8);
                let decoded = octahedral([component(0), component(1), component(2)], 127.0);
                for (byte, value) in element.iter_mut().zip(decoded) {
                    *byte = value as i8 as u8;
                }
            }
        }
        8 => {
            for element in data.as_chunks_mut::<8>().0 {
                let pairs = element.as_chunks_mut::<2>().0;
                let component = |i: usize| f32::from(i16::from_le_bytes(pairs[i]));
                let decoded = octahedral([component(0), component(1), component(2)], 32767.0);
                for (pair, value) in pairs.iter_mut().zip(decoded) {
                    *pair = (value as i16).to_le_bytes();
                }
            }
        }
        _ => bail!("an octahedral stride of {stride} bytes, not 4 or 8"),
    }
    Ok(())
}

/// Filter 2, in place: three i16 components scaled by sqrt(2) and the K-bit 1.0 whose low 2 bits name the fourth.
pub fn filter_quaternion(data: &mut [u8]) {
    let scale = 32767.0 / std::f32::consts::SQRT_2;
    for element in data.as_chunks_mut::<8>().0 {
        let pairs = element.as_chunks_mut::<2>().0;
        let q: [i16; 4] = std::array::from_fn(|i| i16::from_le_bytes(pairs[i]));
        let one = (i32::from(q[3]) | 3) as f32;
        let [x, y, z] = [q[0], q[1], q[2]].map(f32::from);
        let squared = one * one * 2.0 - x * x - y * y - z * z;
        let w = (if squared >= 0.0 { squared } else { 0.0 }).sqrt();
        let unit = scale / one;
        let largest = (q[3] & 3) as usize;
        let mut out = [0i16; 4];
        out[(largest + 1) & 3] = round_to_int(x * unit) as i16;
        out[(largest + 2) & 3] = round_to_int(y * unit) as i16;
        out[(largest + 3) & 3] = round_to_int(z * unit) as i16;
        out[largest] = (w * unit + 0.5) as i32 as i16;
        for (pair, value) in pairs.iter_mut().zip(out) {
            *pair = value.to_le_bytes();
        }
    }
}

/// Filter 3, in place: each little-endian i32 is an 8-bit exponent over a 24-bit mantissa, `2^e * m` as f32.
pub fn filter_exponential(data: &mut [u8]) {
    for value in data.as_chunks_mut::<4>().0 {
        let bits = u32::from_le_bytes(*value);
        let mantissa = ((bits << 8) as i32) >> 8;
        let exponent = (bits as i32) >> 24;
        // 2^e as an f32 built from its exponent field, as meshoptimizer does; e outside [-100, 100] is unspecified.
        let power = f32::from_bits(((exponent + 127) as u32) << 23);
        *value = (power * mantissa as f32).to_bits().to_le_bytes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_specifications_four_bit_group_decodes_to_its_deltas() {
        let stream = [0x17, 0x5f, 0xf0, 0xbc, 0x77, 0xa9, 0x21, 0x00, 0x34, 0xb5];
        let mut reader = Bytes {
            data: &stream,
            at: 0,
        };
        let mut deltas = [0u8; GROUP];
        decode_group(&mut reader, &mut deltas, 4).unwrap();
        assert_eq!(reader.at, stream.len(), "both sentinels' bytes are read");
        let signed = deltas.map(|delta| unzigzag8(delta) as i8);
        assert_eq!(
            signed,
            [-1, -4, -3, 26, -91, 0, -6, 6, -4, -4, 5, -5, 1, -1, 0, 0]
        );
    }

    #[test]
    fn varints_take_at_most_five_bytes() {
        let read = |bytes: &[u8]| {
            let mut reader = Bytes { data: bytes, at: 0 };
            (reader.varint(), reader.at)
        };
        assert_eq!(read(&[0x7f]), (Some(0x7f), 1));
        assert_eq!(read(&[0x81, 0x04]), (Some(0x201), 2));
        // Low groups first, as meshoptimizer's decodeVByte reads them; the specification's third example is not LEB128.
        assert_eq!(read(&[0xff, 0xa0, 0x05]), (Some(0x1_507f), 3));
        assert_eq!(
            read(&[0xff, 0xff, 0xff, 0xff, 0xff, 0x01]).1,
            5,
            "the fifth byte ends it"
        );
        assert_eq!(read(&[0x80]).0, None, "a continuation past the end");
    }

    #[test]
    fn an_all_zero_vertex_stream_is_the_baseline_repeated() {
        // Header, one block of 20 elements: two groups, one header byte of mode 0 per byte, then the tail.
        let stride = 4;
        let mut stream = vec![VERTEX_HEADER, 0, 0, 0, 0];
        stream.extend([0u8; TAIL_MIN - 4]);
        stream.extend([1, 2, 3, 4]);
        assert_eq!(min_vertex_stream_bytes(20, stride), Some(stream.len()));
        let mut out = vec![0u8; 20 * stride];
        decode_vertex_buffer(&mut out, stride, &stream).unwrap();
        assert!(out
            .as_chunks::<4>()
            .0
            .iter()
            .all(|element| element == &[1, 2, 3, 4]));
        assert!(decode_vertex_buffer(&mut out, stride, &stream[..stream.len() - 1]).is_err());
        let mut longer = stream.clone();
        longer.insert(1, 0);
        assert!(
            decode_vertex_buffer(&mut out, stride, &longer).is_err(),
            "a stray byte before the tail"
        );
    }

    #[test]
    fn headers_of_another_mode_or_version_are_refused() {
        // Streams that decode under the right header: three zero elements, two triangles, six zero indices.
        let mut vertex = vec![VERTEX_HEADER, 0, 0, 0, 0];
        vertex.extend([0u8; TAIL_MIN]);
        let mut triangles = vec![0xe1, 0xf0, 0x00];
        triangles.extend([0u8; CODEAUX_BYTES]);
        let mut sequence = vec![0xd1];
        sequence.extend([0u8; 6 + SEQUENCE_TAIL]);
        let mut out = [0u8; 12];
        assert!(decode_vertex_buffer(&mut out, 4, &vertex).is_ok());
        assert!(decode_index_buffer(&mut out, 6, 2, &triangles).is_ok());
        assert!(decode_index_sequence(&mut out, 6, 2, &sequence).is_ok());
        for (stream, header) in [
            (&mut vertex, 0xa1),
            (&mut triangles, 0xe2),
            (&mut sequence, 0xd2),
        ] {
            stream[0] = header;
        }
        assert!(
            decode_vertex_buffer(&mut out, 4, &vertex).is_err(),
            "vertex codec version 1"
        );
        assert!(
            decode_index_buffer(&mut out, 6, 2, &triangles).is_err(),
            "index codec version 2"
        );
        assert!(
            decode_index_sequence(&mut out, 6, 2, &sequence).is_err(),
            "index codec version 2"
        );
        triangles[0] = 0xd1;
        assert!(
            decode_index_buffer(&mut out, 6, 2, &triangles).is_err(),
            "a sequence header"
        );
    }

    #[test]
    fn triangles_from_the_next_vertex_and_the_codeaux_table_decode() {
        // 0xf0 with codeaux[0] = 0x00: three new vertices; 0x00 then takes the newest edge (a, c) and the next vertex.
        let mut stream = vec![0xe1, 0xf0, 0x00];
        stream.extend([0u8; CODEAUX_BYTES]);
        let mut out = [0u8; 12];
        decode_index_buffer(&mut out, 6, 2, &stream).unwrap();
        let indices: Vec<u16> = out
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        assert_eq!(indices, [0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn exponents_scale_their_mantissas_exactly() {
        let encode = |exponent: i32, mantissa: i32| {
            ((exponent as u32) << 24) | (mantissa as u32 & 0xff_ffff)
        };
        let cases = [
            (0, 1, 1.0f32),
            (-3, 12, 1.5),
            (4, -3, -48.0),
            (-100, 1 << 22, 2f32.powi(-78)),
            (100, -1, -(2f32.powi(100))),
        ];
        let mut data: Vec<u8> = cases
            .iter()
            .flat_map(|&(exponent, mantissa, _)| encode(exponent, mantissa).to_le_bytes())
            .collect();
        filter_exponential(&mut data);
        for (bytes, (_, _, expected)) in data.as_chunks::<4>().0.iter().zip(cases) {
            assert_eq!(f32::from_le_bytes(*bytes), expected);
        }
    }
}
