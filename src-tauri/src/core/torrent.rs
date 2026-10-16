//! Minimal .torrent (BitTorrent v1) reader: file list, piece hashes, info-hash,
//! and a release-to-release diff that works without touching the disk.

use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};

// ---------- bencode ----------

enum Value<'a> {
    Int(i64),
    Bytes(&'a [u8]),
    List(Vec<Value<'a>>),
    Dict(BTreeMap<&'a [u8], Value<'a>>),
}

impl<'a> Value<'a> {
    fn get(&self, key: &str) -> Option<&Value<'a>> {
        match self {
            Value::Dict(d) => d.get(key.as_bytes()),
            _ => None,
        }
    }
    fn int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }
    fn bytes(&self) -> Option<&'a [u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }
    fn str(&self) -> Option<String> {
        self.bytes().map(|b| String::from_utf8_lossy(b).into_owned())
    }
    fn list(&self) -> Option<&[Value<'a>]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }
}

struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
    /// Byte span of the top-level `info` dict, for the info-hash.
    info_span: Option<Range<usize>>,
}

impl<'a> Decoder<'a> {
    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("Invalid torrent: {what} at byte {}", self.pos))
    }

    fn peek(&self) -> Result<u8, String> {
        self.buf.get(self.pos).copied().map_or_else(|| self.err("unexpected end"), Ok)
    }

    fn until(&mut self, end: u8) -> Result<&'a [u8], String> {
        let start = self.pos;
        let len = self.buf[start..].iter().position(|&b| b == end).map_or_else(|| self.err("unterminated value"), Ok)?;
        self.pos = start + len + 1;
        Ok(&self.buf[start..start + len])
    }

    fn value(&mut self, depth: usize) -> Result<Value<'a>, String> {
        if depth > 64 {
            return self.err("nesting too deep");
        }
        match self.peek()? {
            b'i' => {
                self.pos += 1;
                let raw = self.until(b'e')?;
                std::str::from_utf8(raw).ok().and_then(|s| s.parse().ok()).map(Value::Int).map_or_else(|| self.err("bad integer"), Ok)
            }
            b'l' => {
                self.pos += 1;
                let mut items = Vec::new();
                while self.peek()? != b'e' {
                    items.push(self.value(depth + 1)?);
                }
                self.pos += 1;
                Ok(Value::List(items))
            }
            b'd' => {
                self.pos += 1;
                let mut map = BTreeMap::new();
                while self.peek()? != b'e' {
                    let Value::Bytes(key) = self.value(depth + 1)? else { return self.err("non-string key") };
                    let start = self.pos;
                    let v = self.value(depth + 1)?;
                    if depth == 0 && key == b"info" {
                        self.info_span = Some(start..self.pos);
                    }
                    map.insert(key, v);
                }
                self.pos += 1;
                Ok(Value::Dict(map))
            }
            b'0'..=b'9' => {
                let raw = self.until(b':')?;
                let len: usize = std::str::from_utf8(raw).ok().and_then(|s| s.parse().ok()).map_or_else(|| self.err("bad length"), Ok)?;
                let end = self.pos.checked_add(len).filter(|&e| e <= self.buf.len()).map_or_else(|| self.err("string past end"), Ok)?;
                let s = &self.buf[self.pos..end];
                self.pos = end;
                Ok(Value::Bytes(s))
            }
            _ => self.err("unexpected byte"),
        }
    }
}

// ---------- torrent ----------

#[derive(Clone, Debug)]
pub struct TorrentFile {
    /// Path relative to the client folder, `/`-separated.
    pub path: String,
    pub length: u64,
    /// Byte offset of this file within the torrent's concatenated data.
    pub offset: u64,
}

#[derive(Clone, Debug)]
pub struct Torrent {
    pub info_hash: String,
    pub name: String,
    pub piece_length: u64,
    pub pieces: Vec<[u8; 20]>,
    pub files: Vec<TorrentFile>,
    pub total_size: u64,
    pub creation_date: Option<i64>,
    pub web_seeds: Vec<String>,
}

/// Rejects anything that could escape the client folder.
fn safe_component(c: &str) -> bool {
    !c.is_empty() && c != "." && c != ".." && !c.contains(['/', '\\', ':']) && !c.contains('\0')
}

impl Torrent {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let mut d = Decoder { buf: bytes, pos: 0, info_span: None };
        let root = d.value(0)?;
        let info = root.get("info").ok_or("Invalid torrent: no info dict")?;
        let span = d.info_span.clone().ok_or("Invalid torrent: no info dict")?;
        let info_hash = hex(&Sha1::digest(&bytes[span]));

        let name = info.get("name").and_then(Value::str).unwrap_or_default();
        let piece_length = info.get("piece length").and_then(Value::int).filter(|&n| n > 0).ok_or("Invalid torrent: bad piece length")? as u64;
        let raw_pieces = info.get("pieces").and_then(Value::bytes).ok_or("Invalid torrent: no pieces")?;
        if raw_pieces.len() % 20 != 0 {
            return Err("Invalid torrent: truncated piece hashes".into());
        }
        let pieces = raw_pieces.chunks_exact(20).map(|c| c.try_into().unwrap()).collect();

        let mut files = Vec::new();
        let mut offset = 0u64;
        if let Some(list) = info.get("files").and_then(Value::list) {
            for f in list {
                let length = f.get("length").and_then(Value::int).filter(|&n| n >= 0).ok_or("Invalid torrent: bad file length")? as u64;
                let parts: Vec<String> = f.get("path").and_then(Value::list).unwrap_or_default().iter().filter_map(Value::str).collect();
                if parts.is_empty() || !parts.iter().all(|p| safe_component(p)) {
                    return Err(format!("Invalid torrent: unsafe file path {parts:?}"));
                }
                files.push(TorrentFile { path: parts.join("/"), length, offset });
                offset += length;
            }
        } else {
            let length = info.get("length").and_then(Value::int).filter(|&n| n >= 0).ok_or("Invalid torrent: bad length")? as u64;
            if !safe_component(&name) {
                return Err("Invalid torrent: unsafe name".into());
            }
            files.push(TorrentFile { path: name.clone(), length, offset: 0 });
            offset = length;
        }

        let expected_pieces = offset.div_ceil(piece_length) as usize;
        let t = Self {
            info_hash,
            name,
            piece_length,
            pieces,
            files,
            total_size: offset,
            creation_date: root.get("creation date").and_then(Value::int),
            web_seeds: match root.get("url-list") {
                Some(Value::Bytes(b)) => vec![String::from_utf8_lossy(b).into_owned()],
                Some(Value::List(l)) => l.iter().filter_map(Value::str).collect(),
                _ => Vec::new(),
            },
        };
        if t.pieces.len() != expected_pieces {
            return Err(format!("Invalid torrent: {} piece hashes for {expected_pieces} pieces", t.pieces.len()));
        }
        Ok(t)
    }

    /// Pieces that overlap a file (empty for zero-length files).
    pub fn pieces_for(&self, file: usize) -> Range<usize> {
        let f = &self.files[file];
        if f.length == 0 {
            return 0..0;
        }
        let first = (f.offset / self.piece_length) as usize;
        let last = ((f.offset + f.length - 1) / self.piece_length) as usize;
        first..last + 1
    }

    /// Byte range a piece covers in the concatenated data.
    pub fn piece_span(&self, piece: usize) -> Range<u64> {
        let start = piece as u64 * self.piece_length;
        start..(start + self.piece_length).min(self.total_size)
    }

    /// Files overlapping a byte range, in order.
    pub fn files_in(&self, span: Range<u64>) -> impl Iterator<Item = usize> + '_ {
        let first = self.files.partition_point(|f| f.offset + f.length <= span.start);
        (first..self.files.len()).take_while(move |&i| self.files[i].offset < span.end).filter(move |&i| self.files[i].length > 0)
    }

    pub fn local_path(&self, root: &Path, file: usize) -> PathBuf {
        self.files[file].path.split('/').fold(root.to_path_buf(), |p, c| p.join(c))
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------- release diff ----------

#[derive(Clone, Debug, PartialEq)]
pub enum Delta {
    /// Not in the previous release.
    New,
    /// Size changed since the previous release.
    Resized { old: u64 },
    /// Same size, but piece hashes inside the file differ.
    Changed,
    Unchanged,
    /// Can't tell from piece hashes alone (piece alignment shifted, or only
    /// boundary pieces shared with neighbours differ). These pieces need a
    /// local check against the new release.
    Unknown { pieces: Vec<usize> },
}

/// Compares each file of `new` with the same path in `old` using only the
/// torrents' piece hashes.
pub fn diff(old: &Torrent, new: &Torrent) -> Vec<Delta> {
    let by_path: HashMap<String, usize> = old.files.iter().enumerate().map(|(i, f)| (f.path.to_ascii_lowercase(), i)).collect();
    let plen = new.piece_length;

    new.files
        .iter()
        .enumerate()
        .map(|(ni, nf)| {
            let Some(&oi) = by_path.get(&nf.path.to_ascii_lowercase()) else { return Delta::New };
            let of = &old.files[oi];
            if of.length != nf.length {
                return Delta::Resized { old: of.length };
            }
            if nf.length == 0 {
                return Delta::Unchanged;
            }
            let all = new.pieces_for(ni).collect::<Vec<_>>();
            if old.piece_length != plen || of.offset % plen != nf.offset % plen {
                return Delta::Unknown { pieces: all };
            }
            // Same alignment: the k-th piece of the file lines up in both releases.
            let (np, op) = (new.pieces_for(ni), old.pieces_for(oi));
            let mut boundary_diff = Vec::new();
            for (n, o) in np.zip(op) {
                if new.pieces[n] == old.pieces[o] {
                    continue;
                }
                let span = new.piece_span(n);
                if span.start >= nf.offset && span.end <= nf.offset + nf.length {
                    return Delta::Changed;
                }
                // Boundary pieces also hold neighbour bytes, so a mismatch there
                // may come from a neighbouring file instead.
                boundary_diff.push(n);
            }
            if boundary_diff.is_empty() { Delta::Unchanged } else { Delta::Unknown { pieces: boundary_diff } }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a multi-file torrent with fake piece hashes derived from `content`.
    fn make(files: &[(&str, &[u8])], plen: usize) -> Vec<u8> {
        let data: Vec<u8> = files.iter().flat_map(|(_, c)| c.iter().copied()).collect();
        let pieces: Vec<u8> = data.chunks(plen).flat_map(|c| Sha1::digest(c).to_vec()).collect();
        let mut info = b"d5:filesl".to_vec();
        for (p, c) in files {
            info.extend(format!("d6:lengthi{}e4:pathl{}:{}ee", c.len(), p.len(), p).as_bytes());
        }
        info.extend(format!("e4:name6:client12:piece lengthi{plen}e6:pieces{}:", pieces.len()).as_bytes());
        info.extend(&pieces);
        info.push(b'e');
        let mut t = b"d4:info".to_vec();
        t.extend(info);
        t.push(b'e');
        t
    }

    #[test]
    fn parses_files_offsets_and_info_hash() {
        let bytes = make(&[("a.mpq", b"aaaaaaa"), ("b.dll", b"bbb")], 4);
        let t = Torrent::parse(&bytes).unwrap();
        assert_eq!(t.files.len(), 2);
        assert_eq!(t.files[1].offset, 7);
        assert_eq!(t.pieces.len(), 3);
        assert_eq!(t.pieces_for(0), 0..2);
        assert_eq!(t.pieces_for(1), 1..3);
        assert_eq!(t.files_in(t.piece_span(1)).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(t.info_hash.len(), 40);
    }

    #[test]
    fn rejects_path_traversal() {
        let bytes = make(&[("..", b"x")], 4);
        assert!(Torrent::parse(&bytes).is_err());
    }

    #[test]
    fn diff_detects_changes() {
        let plen = 4;
        let old = Torrent::parse(&make(&[("a", b"AAAAAAAAAAAA"), ("b", b"BBBBBBBB"), ("c", b"CC")], plen)).unwrap();
        let new = Torrent::parse(&make(&[("a", b"AAAAAXAAAAAA"), ("b", b"BBBBBBBB"), ("c", b"CCC"), ("d", b"D")], plen)).unwrap();
        let d = diff(&old, &new);
        assert_eq!(d[0], Delta::Changed);
        assert_eq!(d[1], Delta::Unchanged);
        assert_eq!(d[2], Delta::Resized { old: 2 });
        assert_eq!(d[3], Delta::New);
    }

    #[test]
    fn diff_marks_shifted_files_unknown() {
        let plen = 4;
        let old = Torrent::parse(&make(&[("a", b"AA"), ("b", b"BBBBBBBB")], plen)).unwrap();
        let new = Torrent::parse(&make(&[("a", b"AAA"), ("b", b"BBBBBBBB")], plen)).unwrap();
        assert!(matches!(diff(&old, &new)[1], Delta::Unknown { .. }));
    }
}
