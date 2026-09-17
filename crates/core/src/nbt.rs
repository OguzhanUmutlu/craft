use crate::error::{CraftError, Result};
use flate2::read::{GzDecoder, ZlibDecoder};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum NbtTag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List(Vec<NbtTag>),
    Compound(BTreeMap<String, NbtTag>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl NbtTag {
    pub fn type_id(&self) -> u8 {
        match self {
            NbtTag::End => 0,
            NbtTag::Byte(_) => 1,
            NbtTag::Short(_) => 2,
            NbtTag::Int(_) => 3,
            NbtTag::Long(_) => 4,
            NbtTag::Float(_) => 5,
            NbtTag::Double(_) => 6,
            NbtTag::ByteArray(_) => 7,
            NbtTag::String(_) => 8,
            NbtTag::List(_) => 9,
            NbtTag::Compound(_) => 10,
            NbtTag::IntArray(_) => 11,
            NbtTag::LongArray(_) => 12,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            NbtTag::End => "End",
            NbtTag::Byte(_) => "Byte",
            NbtTag::Short(_) => "Short",
            NbtTag::Int(_) => "Int",
            NbtTag::Long(_) => "Long",
            NbtTag::Float(_) => "Float",
            NbtTag::Double(_) => "Double",
            NbtTag::ByteArray(_) => "ByteArray",
            NbtTag::String(_) => "String",
            NbtTag::List(_) => "List",
            NbtTag::Compound(_) => "Compound",
            NbtTag::IntArray(_) => "IntArray",
            NbtTag::LongArray(_) => "LongArray",
        }
    }

    pub fn as_compound(&self) -> Option<&BTreeMap<String, NbtTag>> {
        if let NbtTag::Compound(map) = self {
            Some(map)
        } else {
            None
        }
    }

    pub fn as_compound_mut(&mut self) -> Option<&mut BTreeMap<String, NbtTag>> {
        if let NbtTag::Compound(map) = self {
            Some(map)
        } else {
            None
        }
    }

    pub fn get(&self, key: &str) -> Option<&NbtTag> {
        self.as_compound().and_then(|m| m.get(key))
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(NbtTag::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn get_i8(&self, key: &str) -> Option<i8> {
        match self.get(key) {
            Some(NbtTag::Byte(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn get_i16(&self, key: &str) -> Option<i16> {
        match self.get(key) {
            Some(NbtTag::Short(v)) => Some(*v),
            Some(NbtTag::Byte(v)) => Some(*v as i16),
            _ => None,
        }
    }

    pub fn get_i32(&self, key: &str) -> Option<i32> {
        match self.get(key) {
            Some(NbtTag::Int(v)) => Some(*v),
            Some(NbtTag::Short(v)) => Some(*v as i32),
            Some(NbtTag::Byte(v)) => Some(*v as i32),
            _ => None,
        }
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        match self.get(key) {
            Some(NbtTag::Long(v)) => Some(*v),
            Some(NbtTag::Int(v)) => Some(*v as i64),
            Some(NbtTag::Short(v)) => Some(*v as i64),
            Some(NbtTag::Byte(v)) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        match self.get(key) {
            Some(NbtTag::Double(v)) => Some(*v),
            Some(NbtTag::Float(v)) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn get_f32(&self, key: &str) -> Option<f32> {
        match self.get(key) {
            Some(NbtTag::Float(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn get_list(&self, key: &str) -> Option<&[NbtTag]> {
        match self.get(key) {
            Some(NbtTag::List(l)) => Some(l.as_slice()),
            _ => None,
        }
    }

    pub fn format_value_brief(&self) -> String {
        match self {
            NbtTag::End => "".to_string(),
            NbtTag::Byte(v) => format!("{}b", v),
            NbtTag::Short(v) => format!("{}s", v),
            NbtTag::Int(v) => format!("{}", v),
            NbtTag::Long(v) => format!("{}L", v),
            NbtTag::Float(v) => format!("{}f", v),
            NbtTag::Double(v) => format!("{}d", v),
            NbtTag::ByteArray(v) => format!("[{} bytes]", v.len()),
            NbtTag::String(v) => format!("\"{}\"", v),
            NbtTag::List(v) => format!("[{} entries]", v.len()),
            NbtTag::Compound(v) => format!("{{{} entries}}", v.len()),
            NbtTag::IntArray(v) => format!("[{} ints]", v.len()),
            NbtTag::LongArray(v) => format!("[{} longs]", v.len()),
        }
    }
}

pub struct NbtFile {
    pub root_name: String,
    pub root: NbtTag,
    pub is_compressed: bool,
}

impl NbtFile {
    pub fn read<P: AsRef<Path>>(path: P) -> Result<Self> {
        let raw = fs::read(path.as_ref()).map_err(CraftError::Io)?;

        if raw.is_empty() {
            return Err(CraftError::Other("Empty NBT file".to_string()));
        }

        // Detect GZIP (0x1F, 0x8B) or ZLIB (0x78)
        let (bytes, is_compressed) = if raw.len() >= 2 && raw[0] == 0x1F && raw[1] == 0x8B {
            let mut decoder = GzDecoder::new(&raw[..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| CraftError::Other(format!("GZIP decompression error: {}", e)))?;
            (decompressed, true)
        } else if raw.len() >= 2
            && raw[0] == 0x78
            && (raw[1] == 0x9C || raw[1] == 0x01 || raw[1] == 0xDA)
        {
            let mut decoder = ZlibDecoder::new(&raw[..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| CraftError::Other(format!("ZLIB decompression error: {}", e)))?;
            (decompressed, true)
        } else {
            (raw, false)
        };

        let mut cursor = Cursor::new(&bytes);
        let tag_id = read_u8(&mut cursor)?;
        if tag_id != 10 {
            return Err(CraftError::Other(format!(
                "Root NBT tag must be Compound (10), found {}",
                tag_id
            )));
        }

        let root_name = read_string(&mut cursor)?;
        let root = read_compound(&mut cursor)?;

        Ok(Self {
            root_name,
            root,
            is_compressed,
        })
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let mut uncompressed = Vec::new();
        uncompressed.push(10); // Root tag ID Compound
        write_string(&mut uncompressed, &self.root_name)?;
        write_tag_payload(&mut uncompressed, &self.root)?;

        let final_bytes = if self.is_compressed {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(&uncompressed)
                .map_err(|e| CraftError::Other(format!("GZIP compression error: {}", e)))?;
            encoder
                .finish()
                .map_err(|e| CraftError::Other(format!("GZIP finish error: {}", e)))?
        } else {
            uncompressed
        };

        let target_path = path.as_ref();
        let temp_path = target_path.with_extension("tmp");
        fs::write(&temp_path, &final_bytes)?;
        fs::rename(&temp_path, target_path)?;

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Low-level NBT Deserializer
// -----------------------------------------------------------------------------

fn read_u8<R: Read>(r: &mut R) -> Result<u8> {
    let mut buf = [0u8; 1];
    r.read_exact(&mut buf)
        .map_err(|e| CraftError::Other(format!("Failed to read byte: {}", e)))?;
    Ok(buf[0])
}

fn read_i8<R: Read>(r: &mut R) -> Result<i8> {
    read_u8(r).map(|b| b as i8)
}

fn read_i16<R: Read>(r: &mut R) -> Result<i16> {
    let mut buf = [0u8; 2];
    r.read_exact(&mut buf)
        .map_err(|e| CraftError::Other(format!("Failed to read i16: {}", e)))?;
    Ok(i16::from_be_bytes(buf))
}

fn read_i32<R: Read>(r: &mut R) -> Result<i32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| CraftError::Other(format!("Failed to read i32: {}", e)))?;
    Ok(i32::from_be_bytes(buf))
}

fn read_i64<R: Read>(r: &mut R) -> Result<i64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)
        .map_err(|e| CraftError::Other(format!("Failed to read i64: {}", e)))?;
    Ok(i64::from_be_bytes(buf))
}

fn read_f32<R: Read>(r: &mut R) -> Result<f32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| CraftError::Other(format!("Failed to read f32: {}", e)))?;
    Ok(f32::from_be_bytes(buf))
}

fn read_f64<R: Read>(r: &mut R) -> Result<f64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)
        .map_err(|e| CraftError::Other(format!("Failed to read f64: {}", e)))?;
    Ok(f64::from_be_bytes(buf))
}

fn read_string<R: Read>(r: &mut R) -> Result<String> {
    let len = read_i16(r)? as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)
        .map_err(|e| CraftError::Other(format!("Failed to read string bytes: {}", e)))?;
    String::from_utf8(buf)
        .map_err(|e| CraftError::Other(format!("Invalid UTF-8 in NBT string: {}", e)))
}

fn read_tag_payload<R: Read>(r: &mut R, tag_id: u8) -> Result<NbtTag> {
    match tag_id {
        0 => Ok(NbtTag::End),
        1 => read_i8(r).map(NbtTag::Byte),
        2 => read_i16(r).map(NbtTag::Short),
        3 => read_i32(r).map(NbtTag::Int),
        4 => read_i64(r).map(NbtTag::Long),
        5 => read_f32(r).map(NbtTag::Float),
        6 => read_f64(r).map(NbtTag::Double),
        7 => {
            let len = read_i32(r)? as usize;
            let mut buf = vec![0u8; len];
            r.read_exact(&mut buf)
                .map_err(|e| CraftError::Other(format!("Failed to read byte array: {}", e)))?;
            Ok(NbtTag::ByteArray(buf))
        }
        8 => read_string(r).map(NbtTag::String),
        9 => {
            let item_id = read_u8(r)?;
            let len = read_i32(r)?;
            let count = if len < 0 { 0 } else { len as usize };
            let mut list = Vec::with_capacity(count);
            for _ in 0..count {
                list.push(read_tag_payload(r, item_id)?);
            }
            Ok(NbtTag::List(list))
        }
        10 => read_compound(r),
        11 => {
            let len = read_i32(r)?;
            let count = if len < 0 { 0 } else { len as usize };
            let mut list = Vec::with_capacity(count);
            for _ in 0..count {
                list.push(read_i32(r)?);
            }
            Ok(NbtTag::IntArray(list))
        }
        12 => {
            let len = read_i32(r)?;
            let count = if len < 0 { 0 } else { len as usize };
            let mut list = Vec::with_capacity(count);
            for _ in 0..count {
                list.push(read_i64(r)?);
            }
            Ok(NbtTag::LongArray(list))
        }
        other => Err(CraftError::Other(format!("Unknown NBT tag ID: {}", other))),
    }
}

fn read_compound<R: Read>(r: &mut R) -> Result<NbtTag> {
    let mut map = BTreeMap::new();
    loop {
        let tag_id = read_u8(r)?;
        if tag_id == 0 {
            break; // TAG_End
        }
        let name = read_string(r)?;
        let val = read_tag_payload(r, tag_id)?;
        map.insert(name, val);
    }
    Ok(NbtTag::Compound(map))
}

// -----------------------------------------------------------------------------
// Low-level NBT Serializer
// -----------------------------------------------------------------------------

fn write_string<W: Write>(w: &mut W, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    let len = bytes.len() as u16;
    w.write_all(&len.to_be_bytes()).map_err(CraftError::Io)?;
    w.write_all(bytes).map_err(CraftError::Io)?;
    Ok(())
}

fn write_tag_payload<W: Write>(w: &mut W, tag: &NbtTag) -> Result<()> {
    match tag {
        NbtTag::End => Ok(()),
        NbtTag::Byte(v) => w.write_all(&[*v as u8]).map_err(CraftError::Io),
        NbtTag::Short(v) => w.write_all(&v.to_be_bytes()).map_err(CraftError::Io),
        NbtTag::Int(v) => w.write_all(&v.to_be_bytes()).map_err(CraftError::Io),
        NbtTag::Long(v) => w.write_all(&v.to_be_bytes()).map_err(CraftError::Io),
        NbtTag::Float(v) => w.write_all(&v.to_be_bytes()).map_err(CraftError::Io),
        NbtTag::Double(v) => w.write_all(&v.to_be_bytes()).map_err(CraftError::Io),
        NbtTag::ByteArray(v) => {
            let len = v.len() as i32;
            w.write_all(&len.to_be_bytes()).map_err(CraftError::Io)?;
            w.write_all(v).map_err(CraftError::Io)
        }
        NbtTag::String(v) => write_string(w, v),
        NbtTag::List(v) => {
            let item_id = v.first().map(|i| i.type_id()).unwrap_or(0);
            w.write_all(&[item_id]).map_err(CraftError::Io)?;
            let len = v.len() as i32;
            w.write_all(&len.to_be_bytes()).map_err(CraftError::Io)?;
            for item in v {
                write_tag_payload(w, item)?;
            }
            Ok(())
        }
        NbtTag::Compound(map) => {
            for (key, val) in map {
                w.write_all(&[val.type_id()]).map_err(CraftError::Io)?;
                write_string(w, key)?;
                write_tag_payload(w, val)?;
            }
            w.write_all(&[0]).map_err(CraftError::Io) // TAG_End
        }
        NbtTag::IntArray(v) => {
            let len = v.len() as i32;
            w.write_all(&len.to_be_bytes()).map_err(CraftError::Io)?;
            for i in v {
                w.write_all(&i.to_be_bytes()).map_err(CraftError::Io)?;
            }
            Ok(())
        }
        NbtTag::LongArray(v) => {
            let len = v.len() as i32;
            w.write_all(&len.to_be_bytes()).map_err(CraftError::Io)?;
            for i in v {
                w.write_all(&i.to_be_bytes()).map_err(CraftError::Io)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nbt_roundtrip_uncompressed() {
        let mut data_map = BTreeMap::new();
        data_map.insert(
            "LevelName".to_string(),
            NbtTag::String("Survival World".to_string()),
        );
        data_map.insert("GameType".to_string(), NbtTag::Int(0));
        data_map.insert("Difficulty".to_string(), NbtTag::Byte(2));
        data_map.insert("hardcore".to_string(), NbtTag::Byte(0));
        data_map.insert("SpawnX".to_string(), NbtTag::Int(100));
        data_map.insert("SpawnY".to_string(), NbtTag::Int(64));
        data_map.insert("SpawnZ".to_string(), NbtTag::Int(-250));
        data_map.insert("RandomSeed".to_string(), NbtTag::Long(1234567890123456789));

        let mut root_map = BTreeMap::new();
        root_map.insert("Data".to_string(), NbtTag::Compound(data_map));

        let file = NbtFile {
            root_name: "".to_string(),
            root: NbtTag::Compound(root_map),
            is_compressed: false,
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let test_path = temp_dir.path().join("level.dat");

        file.write(&test_path).unwrap();

        let loaded = NbtFile::read(&test_path).unwrap();
        assert_eq!(loaded.root_name, "");
        assert_eq!(loaded.root, file.root);

        let data = loaded.root.get("Data").unwrap();
        assert_eq!(data.get_str("LevelName"), Some("Survival World"));
        assert_eq!(data.get_i32("SpawnX"), Some(100));
        assert_eq!(data.get_i64("RandomSeed"), Some(1234567890123456789));
        assert_eq!(data.get_i8("Difficulty"), Some(2));
    }

    #[test]
    fn test_nbt_roundtrip_gzipped() {
        let mut root_map = BTreeMap::new();
        root_map.insert("Health".to_string(), NbtTag::Float(20.0));
        root_map.insert("foodLevel".to_string(), NbtTag::Int(20));
        root_map.insert(
            "Pos".to_string(),
            NbtTag::List(vec![
                NbtTag::Double(12.5),
                NbtTag::Double(65.0),
                NbtTag::Double(-89.2),
            ]),
        );

        let file = NbtFile {
            root_name: "".to_string(),
            root: NbtTag::Compound(root_map),
            is_compressed: true,
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let test_path = temp_dir.path().join("player.dat");

        file.write(&test_path).unwrap();

        let loaded = NbtFile::read(&test_path).unwrap();
        assert!(loaded.is_compressed);
        assert_eq!(loaded.root.get_f32("Health"), Some(20.0));
        assert_eq!(loaded.root.get_i32("foodLevel"), Some(20));

        let pos = loaded.root.get_list("Pos").unwrap();
        assert_eq!(pos.len(), 3);
        if let NbtTag::Double(x) = pos[0] {
            assert_eq!(x, 12.5);
        } else {
            panic!("Expected double for Pos[0]");
        }
    }
}
