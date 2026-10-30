//! In-memory pack validation: no Git helper, object import, temporary file or index write.
//! This decoder is not an acquisition quota until the transport and every persistence write use it.
use engram_knowledge::{CoreError, CoreResult};
use flate2::bufread::ZlibDecoder;
use sha1collisiondetection::Sha1CD;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub(crate) struct PackLimits {
    pub wire_bytes: usize,
    pub inflated_bytes: usize,
    pub objects: usize,
    pub delta_depth: usize,
    pub timeout: Duration,
}
#[derive(Clone, Debug)]
pub(crate) struct GitObject {
    pub kind: u8,
    pub bytes: Vec<u8>,
}
#[derive(Debug)]
enum Base {
    Offset(usize),
    Reference(Vec<u8>),
}
struct Entry {
    offset: usize,
    bytes: Vec<u8>,
    base: Option<Base>,
    resolved: Option<(Vec<u8>, GitObject, usize)>,
}
fn invalid() -> CoreError {
    CoreError::InvalidRequest {
        reason: "invalid or resource-exceeding remote Git pack".into(),
    }
}
fn hash_parts(parts: &[&[u8]], hash_len: usize) -> CoreResult<Vec<u8>> {
    if hash_len == 20 {
        let mut h = Sha1CD::default();
        for part in parts {
            h.update(part);
        }
        h.finalize_cd()
            .map(|d| d.to_vec())
            .map_err(|_| CoreError::InvalidRequest {
                reason: "Git SHA-1 collision detected".into(),
            })
    } else if hash_len == 32 {
        let mut h = Sha256::new();
        for part in parts {
            h.update(part);
        }
        Ok(h.finalize().to_vec())
    } else {
        Err(invalid())
    }
}
fn hash(bytes: &[u8], hash_len: usize) -> CoreResult<Vec<u8>> {
    hash_parts(&[bytes], hash_len)
}
fn object_id(kind: u8, bytes: &[u8], hash_len: usize) -> CoreResult<Vec<u8>> {
    let kind = match kind {
        1 => "commit",
        2 => "tree",
        3 => "blob",
        4 => "tag",
        _ => return Err(invalid()),
    };
    let header = format!("{kind} {}\0", bytes.len());
    // Both pack and object paths use the same fallible collision-detecting SHA-1 primitive.
    hash_parts(&[header.as_bytes(), bytes], hash_len)
}

fn byte(input: &[u8], at: &mut usize) -> CoreResult<u8> {
    let b = *input.get(*at).ok_or_else(invalid)?;
    *at = at.checked_add(1).ok_or_else(invalid)?;
    Ok(b)
}
fn charge(remaining: &mut usize, size: usize) -> CoreResult<()> {
    *remaining = remaining.checked_sub(size).ok_or_else(invalid)?;
    Ok(())
}
fn varint(input: &[u8], at: &mut usize) -> CoreResult<usize> {
    let mut value = 0usize;
    let mut shift = 0;
    loop {
        let b = byte(input, at)?;
        if shift >= usize::BITS || ((b & 127) as usize) > (usize::MAX >> shift) {
            return Err(invalid());
        }
        value |= ((b & 127) as usize) << shift;
        if b & 128 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
}
fn delta(
    base: &[u8],
    instructions: &[u8],
    remaining: &mut usize,
    mut check_deadline: impl FnMut() -> CoreResult<()>,
) -> CoreResult<Vec<u8>> {
    let mut at = 0;
    if varint(instructions, &mut at)? != base.len() {
        return Err(invalid());
    }
    let size = varint(instructions, &mut at)?;
    // All reconstructed bytes, including intermediate delta bases, consume the same monotonic budget.
    charge(remaining, size)?;
    let mut result = Vec::new();
    while at < instructions.len() {
        check_deadline()?;
        let op = byte(instructions, &mut at)?;
        let (offset, len) = if op & 128 != 0 {
            let mut offset = 0usize;
            let mut len = 0usize;
            for n in 0..4 {
                if op & (1 << n) != 0 {
                    offset |= (byte(instructions, &mut at)? as usize) << (n * 8);
                }
            }
            for n in 0..3 {
                if op & (1 << (n + 4)) != 0 {
                    len |= (byte(instructions, &mut at)? as usize) << (n * 8);
                }
            }
            if len == 0 {
                len = 65536;
            }
            (Some(offset), len)
        } else if op != 0 {
            (None, op as usize)
        } else {
            return Err(invalid());
        };
        if len > size.saturating_sub(result.len()) {
            return Err(invalid());
        }
        if let Some(offset) = offset {
            let end = offset.checked_add(len).ok_or_else(invalid)?;
            result.extend_from_slice(base.get(offset..end).ok_or_else(invalid)?);
        } else {
            let end = at.checked_add(len).ok_or_else(invalid)?;
            result.extend_from_slice(instructions.get(at..end).ok_or_else(invalid)?);
            at = end;
        }
    }
    if result.len() != size {
        return Err(invalid());
    }
    Ok(result)
}

struct DeadlineSlice<'a> {
    bytes: &'a [u8],
    started: Instant,
    timeout: Duration,
}
impl Read for DeadlineSlice<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.started.elapsed() >= self.timeout {
            return Err(std::io::Error::from(std::io::ErrorKind::TimedOut));
        }
        let len = out.len().min(self.bytes.len()).min(8192);
        out[..len].copy_from_slice(&self.bytes[..len]);
        self.bytes = &self.bytes[len..];
        Ok(len)
    }
}

pub(crate) fn decode_pack(
    input: &[u8],
    hash_len: usize,
    limits: &PackLimits,
) -> CoreResult<BTreeMap<Vec<u8>, GitObject>> {
    let started = Instant::now();
    if !matches!(hash_len, 20 | 32)
        || input.len() > limits.wire_bytes
        || input.len() < 12 + hash_len
    {
        return Err(invalid());
    }
    let end = input.len() - hash_len;
    if &input[..4] != b"PACK"
        || !matches!(
            u32::from_be_bytes(input[4..8].try_into().map_err(|_| invalid())?),
            2 | 3
        )
        || hash(&input[..end], hash_len)? != input[end..]
    {
        return Err(invalid());
    }
    let count = u32::from_be_bytes(input[8..12].try_into().map_err(|_| invalid())?) as usize;
    if count > limits.objects {
        return Err(invalid());
    }
    let input = &input[..end];
    let mut remaining = limits.inflated_bytes;
    let mut at = 12;
    let mut entries = Vec::new();
    for _ in 0..count {
        if started.elapsed() >= limits.timeout {
            return Err(invalid());
        }
        let offset = at;
        let first = byte(input, &mut at)?;
        let kind = (first >> 4) & 7;
        let mut size = (first & 15) as usize;
        let mut more = first & 128 != 0;
        let mut shift = 4;
        while more {
            let b = byte(input, &mut at)?;
            if shift >= usize::BITS || ((b & 127) as usize) > (usize::MAX >> shift) {
                return Err(invalid());
            }
            size |= ((b & 127) as usize) << shift;
            shift += 7;
            more = b & 128 != 0;
        }
        let base = match kind {
            1..=4 => None,
            6 => {
                let mut b = byte(input, &mut at)?;
                let mut distance = (b & 127) as usize;
                while b & 128 != 0 {
                    b = byte(input, &mut at)?;
                    distance = distance
                        .checked_add(1)
                        .and_then(|x| x.checked_mul(128))
                        .and_then(|x| x.checked_add((b & 127) as usize))
                        .ok_or_else(invalid)?;
                }
                if distance == 0 {
                    return Err(invalid());
                }
                Some(Base::Offset(
                    offset.checked_sub(distance).ok_or_else(invalid)?,
                ))
            }
            7 => {
                let end = at.checked_add(hash_len).ok_or_else(invalid)?;
                let oid = input.get(at..end).ok_or_else(invalid)?.to_vec();
                at = end;
                Some(Base::Reference(oid))
            }
            _ => return Err(invalid()),
        };
        charge(&mut remaining, size)?;
        let reader = DeadlineSlice {
            bytes: input.get(at..).ok_or_else(invalid)?,
            started,
            timeout: limits.timeout,
        };
        let mut decoder = ZlibDecoder::new(std::io::BufReader::with_capacity(8192, reader));
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            if started.elapsed() >= limits.timeout {
                return Err(invalid());
            }
            let remaining = size.checked_sub(bytes.len()).ok_or_else(invalid)?;
            let capacity = remaining.saturating_add(1).min(buffer.len());
            let len = decoder
                .read(&mut buffer[..capacity])
                .map_err(|_| invalid())?;
            if len == 0 {
                break;
            }
            if len > remaining {
                return Err(invalid());
            }
            bytes.extend_from_slice(&buffer[..len]);
        }
        if bytes.len() != size {
            return Err(invalid());
        }
        let consumed = usize::try_from(decoder.total_in()).map_err(|_| invalid())?;
        if consumed == 0 {
            return Err(invalid());
        }
        at = at.checked_add(consumed).ok_or_else(invalid)?;
        let resolved = if base.is_none() {
            let oid = object_id(kind, &bytes, hash_len)?;
            Some((
                oid,
                GitObject {
                    kind,
                    bytes: std::mem::take(&mut bytes),
                },
                0,
            ))
        } else {
            None
        };
        entries.push(Entry {
            offset,
            bytes,
            base,
            resolved,
        });
    }
    if at != input.len() {
        return Err(invalid());
    }
    let offsets: BTreeMap<_, _> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.offset, i))
        .collect();
    let mut oids: BTreeMap<Vec<u8>, usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.resolved.as_ref().map(|r| (r.0.clone(), i)))
        .collect();
    for _ in 0..=limits.delta_depth {
        let mut progress = false;
        for i in 0..entries.len() {
            if started.elapsed() >= limits.timeout {
                return Err(invalid());
            }
            if entries[i].resolved.is_some() {
                continue;
            }
            let base = match entries[i].base.as_ref().ok_or_else(invalid)? {
                Base::Offset(offset) => offsets.get(offset),
                Base::Reference(oid) => oids.get(oid),
            }
            .copied();
            let Some(base) = base else {
                continue;
            };
            let Some((_, object, depth)) = entries[base].resolved.as_ref() else {
                continue;
            };
            if *depth >= limits.delta_depth {
                return Err(invalid());
            }
            let depth = depth + 1;
            let kind = object.kind;
            let bytes = delta(&object.bytes, &entries[i].bytes, &mut remaining, || {
                if started.elapsed() >= limits.timeout {
                    Err(invalid())
                } else {
                    Ok(())
                }
            })?;
            let oid = object_id(kind, &bytes, hash_len)?;
            entries[i].bytes.clear();
            entries[i].resolved = Some((oid.clone(), GitObject { kind, bytes }, depth));
            oids.insert(oid, i);
            progress = true;
        }
        if entries.iter().all(|e| e.resolved.is_some()) {
            break;
        }
        if !progress {
            return Err(invalid());
        }
    }
    let mut result = BTreeMap::new();
    for entry in entries {
        let (oid, object, _) = entry.resolved.ok_or_else(invalid)?;
        if let Some(old) = result.get(&oid) {
            let old: &GitObject = old;
            if old.kind != object.kind || old.bytes != object.bytes {
                return Err(invalid());
            }
        } else {
            result.insert(oid, object);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;
    #[test]
    fn task_008_sha1dc_known_collision_rejected_and_sha256_preserved() {
        for input in [
            include_bytes!("../../tests/fixtures/sha1dc/sha-mbles-1.bin").as_slice(),
            include_bytes!("../../tests/fixtures/sha1dc/sha-mbles-2.bin").as_slice(),
        ] {
            let mut ordinary = Sha1CD::configure().detect_collisions(false).build();
            ordinary.update(input);
            let digest = ordinary.finalize_cd().unwrap();
            assert_eq!(
                digest
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                "8ac60ba76f1999a1ab70223f225aefdc78d4ddc0"
            );
            assert!(
                matches!(hash(input,20),Err(CoreError::InvalidRequest {reason}) if reason=="Git SHA-1 collision detected")
            );
            assert!(hash_parts(&[&input[..37], &input[37..]], 20).is_err());
            assert_eq!(hash(input, 32).unwrap(), Sha256::digest(input).as_slice());
        }
        assert!(object_id(3, b"ordinary Git object", 20).is_ok());
        assert!(object_id(3, b"ordinary Git object", 32).is_ok());
    }
    fn limits() -> PackLimits {
        PackLimits {
            wire_bytes: 65536,
            inflated_bytes: 65536,
            objects: 100,
            delta_depth: 8,
            timeout: Duration::from_secs(2),
        }
    }
    fn pack(kind: u8, bytes: &[u8], hash_len: usize) -> Vec<u8> {
        let mut p = b"PACK\0\0\0\x02\0\0\0\x01".to_vec();
        let mut len = bytes.len();
        let mut b = (kind << 4) | (len as u8 & 15);
        len >>= 4;
        if len != 0 {
            b |= 128;
        }
        p.push(b);
        while len != 0 {
            let mut b = (len & 127) as u8;
            len >>= 7;
            if len != 0 {
                b |= 128;
            }
            p.push(b);
        }
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(bytes).unwrap();
        p.extend(z.finish().unwrap());
        let checksum = hash(&p, hash_len).unwrap();
        p.extend(checksum);
        p
    }
    #[test]
    fn task_008_portable_pack_sha1_sha256() {
        for hash_len in [20, 32] {
            let p = pack(3, b"exact\r\nbytes", hash_len);
            let objects = decode_pack(&p, hash_len, &limits()).unwrap();
            assert_eq!(
                objects[&object_id(3, b"exact\r\nbytes", hash_len).unwrap()].bytes,
                b"exact\r\nbytes"
            );
        }
    }
    #[test]
    fn task_008_portable_pack_quota_before_inflation() {
        let p = pack(3, &vec![0; 32768], 20);
        let mut l = limits();
        l.inflated_bytes = 1024;
        assert!(decode_pack(&p, 20, &l).is_err());
        l = limits();
        l.wire_bytes = p.len() - 1;
        assert!(decode_pack(&p, 20, &l).is_err());
        l = limits();
        l.objects = 0;
        assert!(decode_pack(&p, 20, &l).is_err());
    }
    #[test]
    fn task_008_portable_pack_corruption_truncation_and_trailing_data() {
        let p = pack(3, b"safe", 20);
        for n in 0..p.len() {
            assert!(decode_pack(&p[..n], 20, &limits()).is_err());
        }
        let mut corrupt = p.clone();
        corrupt[14] ^= 1;
        assert!(decode_pack(&corrupt, 20, &limits()).is_err());
        let mut trailing = p[..p.len() - 20].to_vec();
        trailing.push(0);
        let h = hash(&trailing, 20).unwrap();
        trailing.extend(h);
        assert!(decode_pack(&trailing, 20, &limits()).is_err());
    }
    fn finish(mut payload: Vec<u8>, hash_len: usize, count: u32) -> Vec<u8> {
        let mut out = b"PACK\0\0\0\x02".to_vec();
        out.extend(count.to_be_bytes());
        out.append(&mut payload);
        let checksum = hash(&out, hash_len).unwrap();
        out.extend(checksum);
        out
    }
    fn entry(kind: u8, bytes: &[u8], hash_len: usize) -> Vec<u8> {
        let p = pack(kind, bytes, hash_len);
        p[12..p.len() - hash_len].to_vec()
    }
    #[test]
    fn task_008_portable_forward_reference_and_offset_delta_depth() {
        for hash_len in [20, 32] {
            let base = entry(3, b"base", hash_len);
            let compressed = entry(3, &[4, 4, 0x90, 4], hash_len);
            let mut reference = vec![0x74];
            reference.extend(object_id(3, b"base", hash_len).unwrap());
            reference.extend(&compressed[1..]);
            reference.extend(&base);
            assert!(decode_pack(&finish(reference, hash_len, 2), hash_len, &limits()).is_ok());
            let mut payload = base.clone();
            payload.extend([0x64, base.len() as u8]);
            payload.extend(&compressed[1..]);
            let previous = payload.len() - base.len();
            payload.extend([0x64, previous as u8]);
            payload.extend(&compressed[1..]);
            let p = finish(payload, hash_len, 3);
            let mut l = limits();
            l.inflated_bytes = 20;
            assert!(decode_pack(&p, hash_len, &l).is_ok());
            l.inflated_bytes = 19;
            assert!(decode_pack(&p, hash_len, &l).is_err());
            l = limits();
            l.delta_depth = 1;
            assert!(decode_pack(&p, hash_len, &l).is_err());
        }
    }
    #[test]
    fn task_008_portable_missing_bases_and_authenticated_malformed_zlib() {
        let compressed = entry(3, &[4, 4, 0x90, 4], 20);
        let mut thin = vec![0x74];
        thin.extend([0; 20]);
        thin.extend(&compressed[1..]);
        assert!(decode_pack(&finish(thin, 20, 1), 20, &limits()).is_err());
        let mut wrong_size = entry(3, b"base", 20);
        wrong_size[0] = 0x35;
        assert!(decode_pack(&finish(wrong_size, 20, 1), 20, &limits()).is_err());
        let mut bad_zlib = entry(3, b"base", 20);
        *bad_zlib.last_mut().unwrap() ^= 1;
        assert!(decode_pack(&finish(bad_zlib, 20, 1), 20, &limits()).is_err());
        let mut overflow = vec![0xb0];
        overflow.extend([0xff; 20]);
        overflow.push(0);
        assert!(decode_pack(&finish(overflow, 20, 1), 20, &limits()).is_err());
        let original = entry(3, &vec![0; 16384], 20);
        let mut zero_declared = vec![0x30];
        zero_declared.extend(&original[3..]);
        assert!(decode_pack(&finish(zero_declared, 20, 1), 20, &limits()).is_err());
    }
    #[test]
    fn task_008_portable_delta_accounting() {
        let mut budget = 4;
        assert_eq!(
            delta(b"base", &[4, 4, 0x90, 4], &mut budget, || Ok(())).unwrap(),
            b"base"
        );
        assert_eq!(budget, 0);
        assert!(delta(b"base", &[4, 4, 0x90, 4], &mut budget, || Ok(())).is_err());
        assert!(delta(b"base", &[4, 4, 0], &mut 100, || Ok(())).is_err());
        assert!(delta(b"base", &[4, 4, 0x91, 3, 4], &mut 100, || Ok(())).is_err());
        assert!(delta(b"base", &[4, 3, 0x90, 4], &mut 100, || Ok(())).is_err());
        assert!(delta(b"base", &[5, 0], &mut 100, || Ok(())).is_err());
    }
}
