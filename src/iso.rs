use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn bytes(data: &[u8], off: usize, size: usize) -> Result<&[u8]> {
    let end = off.checked_add(size).ok_or("offset overflow")?;
    data.get(off..end)
        .ok_or_else(|| format!("data out of bounds at {off:#x} + {size:#x}").into())
}
pub fn u16le(data: &[u8], off: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes(data, off, 2)?.try_into()?))
}
pub fn u32le(data: &[u8], off: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(data, off, 4)?.try_into()?))
}
pub fn f32le(data: &[u8], off: usize) -> Result<f32> {
    Ok(f32::from_bits(u32le(data, off)?))
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub path: String,
    pub lba: u32,
    pub size: u32,
    pub directory: bool,
}

pub struct Disc {
    file: File,
    pub size: u64,
    pub entries: BTreeMap<String, Entry>,
}
impl Disc {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        let mut disc = Self {
            file,
            size,
            entries: BTreeMap::new(),
        };
        let pvd = disc.read_at(16 * 2048, 2048)?;
        if &pvd[0..7] != b"\x01CD001\x01" {
            return Err("ISO9660 volume not found".into());
        }
        let root = bytes(&pvd, 156, 34)?;
        disc.walk(u32le(root, 2)?, u32le(root, 10)?, "", 0)?;
        Ok(disc)
    }
    pub fn read_at(&mut self, off: u64, size: usize) -> Result<Vec<u8>> {
        if size > 512 * 1024 * 1024
            || off.checked_add(size as u64).ok_or("ISO overflow")? > self.size
        {
            return Err("ISO extent out of bounds or exceeds 512 MiB read limit".into());
        }
        self.file.seek(SeekFrom::Start(off))?;
        let mut data = vec![0; size];
        self.file.read_exact(&mut data)?;
        Ok(data)
    }
    fn walk(&mut self, lba: u32, size: u32, parent: &str, depth: usize) -> Result<()> {
        if depth > 16 || size > 16 * 1024 * 1024 {
            return Err("Invalid ISO directory".into());
        }
        let data = self.read_at(lba as u64 * 2048, size as usize)?;
        let mut off = 0;
        while off < data.len() {
            let length = data[off] as usize;
            if length == 0 {
                off = (off / 2048 + 1) * 2048;
                continue;
            }
            let row = bytes(&data, off, length)?;
            off += length;
            if row.len() < 34 {
                return Err("Short ISO directory record".into());
            }
            let name = bytes(row, 33, row[32] as usize)?;
            if name == [0] || name == [1] {
                continue;
            }
            let name = std::str::from_utf8(name)?
                .split(';')
                .next()
                .ok_or("Missing filename")?;
            if name.contains('/') || name.contains('\\') || name == ".." {
                return Err("Unsafe ISO filename".into());
            }
            let path = format!("{parent}{name}");
            let entry = Entry {
                path: path.clone(),
                lba: u32le(row, 2)?,
                size: u32le(row, 10)?,
                directory: row[25] & 2 != 0,
            };
            if entry.directory {
                self.walk(entry.lba, entry.size, &format!("{path}/"), depth + 1)?;
            }
            self.entries.insert(path, entry);
        }
        Ok(())
    }
    pub fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(path)
            .ok_or_else(|| format!("ISO file missing: {path}"))?
            .clone();
        if entry.directory {
            return Err("Cannot read a directory as a file".into());
        }
        self.read_at(entry.lba as u64 * 2048, entry.size as usize)
    }
    pub fn range(&mut self, path: &str, off: usize, size: usize) -> Result<Vec<u8>> {
        let entry = self.entries.get(path).ok_or("IMG missing")?.clone();
        if off.checked_add(size).ok_or("IMG extent overflow")? > entry.size as usize {
            return Err("IMG extent out of bounds".into());
        }
        self.read_at(entry.lba as u64 * 2048 + off as u64, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_truncated_and_overflowing_fields() {
        assert!(u32le(&[1, 2, 3], 0).is_err());
        assert!(bytes(&[0; 8], usize::MAX, 2).is_err());
        assert_eq!(u32le(&[0x22, 0x46, 0x05, 0], 0).unwrap(), 0x54622);
    }
}
