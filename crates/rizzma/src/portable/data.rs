//! Typed binary accessors: bulk arrays in one binary chunk.
//!
//! JSON floats run 2–3× their binary size and make exact round-trips fiddly,
//! so bulk numeric data (line vertices, scatter offsets, image samples) lives
//! in the container's `BIN ` chunk as little-endian typed arrays. The JSON spec
//! references them by accessor index; small hand-authored arrays may inline as
//! plain JSON lists instead ([`ArrF64::Inline`]).
//!
//! Every accessor is bounds-checked against the chunk before any figure state
//! is constructed: a malformed artifact is a [`PortableError::Malformed`], not
//! a panic.

use serde::{Deserialize, Serialize};

use super::PortableError;

/// Element type of a binary accessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DType {
    /// 64-bit IEEE-754 floats, little-endian.
    F64,
    /// Raw bytes (path codes).
    U8,
}

impl DType {
    /// Size of one element in bytes.
    fn size(self) -> usize {
        match self {
            DType::F64 => 8,
            DType::U8 => 1,
        }
    }
}

/// One typed view into the binary chunk: `count` elements of `dtype` starting
/// at byte `offset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accessor {
    /// Element type.
    pub dtype: DType,
    /// Number of elements.
    pub count: usize,
    /// Byte offset of the first element within the binary chunk.
    pub offset: usize,
}

/// A reference to an `f64` array: an accessor index, or a small inline list.
///
/// The exporter always writes accessors; the importer accepts either form.
/// Note JSON cannot express NaN or infinity, so inline arrays are limited to
/// finite values — another reason the exporter goes through the binary chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArrF64 {
    /// Index into the spec's accessor table (`{"acc": n}`).
    Acc {
        /// Accessor index.
        acc: usize,
    },
    /// Values inline in the JSON.
    Inline(Vec<f64>),
}

/// A reference to a `u8` array: an accessor index, or a small inline list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArrU8 {
    /// Index into the spec's accessor table (`{"acc": n}`).
    Acc {
        /// Accessor index.
        acc: usize,
    },
    /// Values inline in the JSON.
    Inline(Vec<u8>),
}

/// Accumulates binary data and its accessor table during export.
#[cfg(feature = "portable")]
#[derive(Debug, Default)]
pub(crate) struct BankWriter {
    /// The binary chunk under construction.
    pub(crate) bytes: Vec<u8>,
    /// One entry per accessor handed out.
    pub(crate) accessors: Vec<Accessor>,
}

#[cfg(feature = "portable")]
impl BankWriter {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Append `values` as a little-endian `f64` accessor.
    pub(crate) fn push_f64(&mut self, values: &[f64]) -> ArrF64 {
        self.align_to(8);
        let offset = self.bytes.len();
        for v in values {
            self.bytes.extend_from_slice(&v.to_le_bytes());
        }
        self.accessors.push(Accessor {
            dtype: DType::F64,
            count: values.len(),
            offset,
        });
        ArrF64::Acc {
            acc: self.accessors.len() - 1,
        }
    }

    /// Append `points` flattened `[x0, y0, x1, y1, …]` as an `f64` accessor.
    pub(crate) fn push_pairs(&mut self, points: &[[f64; 2]]) -> ArrF64 {
        self.align_to(8);
        let offset = self.bytes.len();
        for [x, y] in points {
            self.bytes.extend_from_slice(&x.to_le_bytes());
            self.bytes.extend_from_slice(&y.to_le_bytes());
        }
        self.accessors.push(Accessor {
            dtype: DType::F64,
            count: points.len() * 2,
            offset,
        });
        ArrF64::Acc {
            acc: self.accessors.len() - 1,
        }
    }

    /// Append `values` as a `u8` accessor.
    pub(crate) fn push_u8(&mut self, values: &[u8]) -> ArrU8 {
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(values);
        self.accessors.push(Accessor {
            dtype: DType::U8,
            count: values.len(),
            offset,
        });
        ArrU8::Acc {
            acc: self.accessors.len() - 1,
        }
    }

    /// Pad with zero bytes until the write position is `align`-aligned.
    fn align_to(&mut self, align: usize) {
        while !self.bytes.len().is_multiple_of(align) {
            self.bytes.push(0);
        }
    }
}

/// Resolves array references against the binary chunk during import.
#[cfg(feature = "portable")]
#[derive(Debug, Clone, Copy)]
pub(crate) struct BankReader<'a> {
    bin: &'a [u8],
    accessors: &'a [Accessor],
}

#[cfg(feature = "portable")]
impl<'a> BankReader<'a> {
    pub(crate) fn new(bin: &'a [u8], accessors: &'a [Accessor]) -> Self {
        Self { bin, accessors }
    }

    /// The validated byte range of accessor `index`, required to be `dtype`.
    fn slice(&self, index: usize, dtype: DType) -> Result<&'a [u8], PortableError> {
        let acc = self.accessors.get(index).ok_or_else(|| {
            PortableError::Malformed(format!(
                "accessor index {index} out of range (table has {})",
                self.accessors.len()
            ))
        })?;
        if acc.dtype != dtype {
            return Err(PortableError::Malformed(format!(
                "accessor {index} has dtype {:?}, expected {dtype:?}",
                acc.dtype
            )));
        }
        let len = acc
            .count
            .checked_mul(acc.dtype.size())
            .ok_or_else(|| PortableError::Malformed(format!("accessor {index} size overflows")))?;
        let end = acc.offset.checked_add(len).ok_or_else(|| {
            PortableError::Malformed(format!("accessor {index} extent overflows"))
        })?;
        self.bin.get(acc.offset..end).ok_or_else(|| {
            PortableError::Malformed(format!(
                "accessor {index} spans bytes {}..{end} but the binary chunk holds {}",
                acc.offset,
                self.bin.len()
            ))
        })
    }

    /// Resolve an [`ArrF64`] to owned values.
    pub(crate) fn f64s(&self, arr: &ArrF64) -> Result<Vec<f64>, PortableError> {
        match arr {
            ArrF64::Inline(values) => Ok(values.clone()),
            ArrF64::Acc { acc } => {
                let bytes = self.slice(*acc, DType::F64)?;
                Ok(bytes
                    .chunks_exact(8)
                    .map(|c| f64::from_le_bytes(c.try_into().expect("chunks_exact(8)")))
                    .collect())
            }
        }
    }

    /// Resolve an [`ArrF64`] holding flattened pairs to `[x, y]` points.
    pub(crate) fn pairs(&self, arr: &ArrF64) -> Result<Vec<[f64; 2]>, PortableError> {
        let flat = self.f64s(arr)?;
        if flat.len() % 2 != 0 {
            return Err(PortableError::Malformed(format!(
                "point array has odd element count {}",
                flat.len()
            )));
        }
        Ok(flat.chunks_exact(2).map(|c| [c[0], c[1]]).collect())
    }

    /// Resolve an [`ArrU8`] to owned bytes.
    pub(crate) fn u8s(&self, arr: &ArrU8) -> Result<Vec<u8>, PortableError> {
        match arr {
            ArrU8::Inline(values) => Ok(values.clone()),
            ArrU8::Acc { acc } => Ok(self.slice(*acc, DType::U8)?.to_vec()),
        }
    }
}
