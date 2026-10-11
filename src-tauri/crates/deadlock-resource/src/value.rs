use crate::{invalid, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Annotation {
    Resource = 1,
    ResourceName = 2,
    Panorama = 3,
    SoundEvent = 4,
    Subclass = 5,
    Unspecified = 8,
}
impl TryFrom<u8> for Annotation {
    type Error = crate::Error;
    fn try_from(value: u8) -> Result<Self> {
        Ok(match value {
            1 => Self::Resource,
            2 => Self::ResourceName,
            3 => Self::Panorama,
            4 => Self::SoundEvent,
            5 => Self::Subclass,
            8 => Self::Unspecified,
            _ => {
                return Err(invalid(format!(
                    "unsupported annotation {value}; refusing metadata loss"
                )))
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Value {
    pub annotation: Annotation,
    pub kind: Kind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    String(String),
    Blob(Vec<u8>),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}
impl Value {
    pub fn new(kind: Kind) -> Self {
        Self {
            annotation: Annotation::Unspecified,
            kind,
        }
    }
    pub fn get(&self, name: &str) -> Option<&Self> {
        match &self.kind {
            Kind::Object(rows) => rows.iter().find(|(k, _)| k == name).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Self> {
        match &mut self.kind {
            Kind::Object(rows) => rows.iter_mut().find(|(k, _)| k == name).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn object(&self) -> Result<&[(String, Self)]> {
        match &self.kind {
            Kind::Object(v) => Ok(v),
            _ => Err(invalid("expected object")),
        }
    }
    pub fn array(&self) -> Result<&[Self]> {
        match &self.kind {
            Kind::Array(v) => Ok(v),
            _ => Err(invalid("expected array")),
        }
    }
    pub fn string(&self) -> Result<&str> {
        match &self.kind {
            Kind::String(v) => Ok(v),
            _ => Err(invalid("expected string")),
        }
    }
    pub fn integer(&self) -> Result<i128> {
        match self.kind {
            Kind::I64(v) => Ok(v.into()),
            Kind::U64(v) => Ok(v.into()),
            _ => Err(invalid("expected integer")),
        }
    }
    pub fn number(&self) -> Result<f64> {
        match self.kind {
            Kind::F32(v) => Ok(v as f64),
            Kind::F64(v) => Ok(v),
            _ => Err(invalid("expected floating-point number")),
        }
    }
    /// Python canonical() compares exact values and annotations, preserving order,
    /// while allowing f32 -> exactly representable f64 and integer storage widths.
    pub fn canonical_eq(&self, other: &Self) -> bool {
        if self.annotation != other.annotation {
            return false;
        }
        match (&self.kind, &other.kind) {
            (Kind::I64(a), Kind::U64(b)) | (Kind::U64(b), Kind::I64(a)) => {
                *a >= 0 && *a as u64 == *b
            }
            (Kind::F32(a), Kind::F64(b)) | (Kind::F64(b), Kind::F32(a)) => {
                (*a as f64).to_bits() == b.to_bits()
            }
            (Kind::F32(a), Kind::F32(b)) => a.to_bits() == b.to_bits(),
            (Kind::F64(a), Kind::F64(b)) => a.to_bits() == b.to_bits(),
            (Kind::Array(a), Kind::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.canonical_eq(y))
            }
            (Kind::Object(a), Kind::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|((k, x), (l, y))| k == l && x.canonical_eq(y))
            }
            (a, b) => a == b,
        }
    }
    pub fn validate(&self) -> Result<()> {
        fn visit(v: &Value, depth: usize, count: &mut usize) -> Result<()> {
            if depth > crate::MAX_DEPTH {
                return Err(invalid("nesting exceeds supported bound"));
            }
            *count += 1;
            if *count > 8 * crate::MAX_COUNT {
                return Err(invalid("total values exceed supported bound"));
            }
            match &v.kind {
                Kind::F32(n) if !n.is_finite() => return Err(invalid("non-finite number")),
                Kind::F64(n) if !n.is_finite() => return Err(invalid("non-finite number")),
                Kind::String(s) if s.contains('\0') => {
                    return Err(invalid("embedded NUL in string"))
                }
                Kind::Array(a) => {
                    if a.len() > crate::MAX_COUNT {
                        return Err(invalid("array count exceeds supported bound"));
                    }
                    for x in a {
                        visit(x, depth + 1, count)?;
                    }
                }
                Kind::Object(a) => {
                    if a.len() > crate::MAX_COUNT {
                        return Err(invalid("object count exceeds supported bound"));
                    }
                    let mut keys = std::collections::HashSet::new();
                    for (k, x) in a {
                        if k.contains('\0') || !keys.insert(k) {
                            return Err(invalid("invalid or duplicate key"));
                        }
                        visit(x, depth + 1, count)?;
                    }
                }
                _ => (),
            }
            Ok(())
        }
        visit(self, 0, &mut 0)
    }
}

/// SHA-256 over the exact canonical value domain used by `canonical_eq`.
/// Length prefixes and node tags prevent strings, blobs, and authored objects
/// from impersonating each other. Integer storage width is not semantic;
/// float32 expands exactly to float64. Order, annotations and signed zero stay.
pub fn canonical_sha256(value: &Value) -> Result<String> {
    use sha2::{Digest, Sha256};
    value.validate()?;
    fn bytes(hasher: &mut Sha256, bytes: &[u8]) {
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    fn visit(hasher: &mut Sha256, value: &Value) {
        hasher.update([value.annotation as u8]);
        match &value.kind {
            Kind::Null => hasher.update([0]),
            Kind::Bool(v) => hasher.update([1, *v as u8]),
            Kind::I64(v) => {
                hasher.update([2]);
                hasher.update((*v as i128).to_le_bytes());
            }
            Kind::U64(v) => {
                hasher.update([2]);
                hasher.update((*v as i128).to_le_bytes());
            }
            Kind::F32(v) => {
                hasher.update([3]);
                hasher.update((*v as f64).to_bits().to_le_bytes());
            }
            Kind::F64(v) => {
                hasher.update([3]);
                hasher.update(v.to_bits().to_le_bytes());
            }
            Kind::String(v) => {
                hasher.update([4]);
                bytes(hasher, v.as_bytes());
            }
            Kind::Blob(v) => {
                hasher.update([5]);
                bytes(hasher, v);
            }
            Kind::Array(v) => {
                hasher.update([6]);
                hasher.update((v.len() as u64).to_le_bytes());
                for value in v {
                    visit(hasher, value);
                }
            }
            Kind::Object(v) => {
                hasher.update([7]);
                hasher.update((v.len() as u64).to_le_bytes());
                for (key, value) in v {
                    bytes(hasher, key.as_bytes());
                    visit(hasher, value);
                }
            }
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(b"deadlock-resource:canonical-kv3-v1\0");
    visit(&mut hasher, value);
    Ok(format!("{:x}", hasher.finalize()))
}
