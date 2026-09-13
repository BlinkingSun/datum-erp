//! Argon2id (RFC 9106) password hashing. Vendored: no hash crate is on the allow-list.
//!
//! Production parameters are pinned as OWASP 2023's interactive recommendation
//! (`m=19456` KiB, `t=2`, `p=1`, 16-byte salt, 32-byte tag).

use crate::blake2b;

/// Memory cost in KiB (OWASP 2023 Argon2id).
pub const M_KIB: u32 = 19_456;
/// Time cost (passes).
pub const T_COST: u32 = 2;
/// Parallelism (lanes).
pub const P_COST: u32 = 1;
/// Tag length in bytes.
pub const TAG_LEN: usize = 32;
/// Salt length in bytes.
pub const SALT_LEN: usize = 16;
/// RFC 9106 version 0x13.
pub const VERSION: u32 = 0x13;

const SYNC_POINTS: usize = 4;
const BLOCK_SIZE: usize = 1024;
const BLOCK_U64S: usize = BLOCK_SIZE / 8;

/// Hash `password` with a fresh salt under the pinned parameters. Returns a PHC string.
pub fn hash_password(password: &[u8]) -> Result<String, Error> {
    let salt = random_salt();
    hash_password_with_params(password, &salt, M_KIB, T_COST, P_COST)
}

/// Verify `password` against a PHC-encoded Argon2id hash.
pub fn verify_password(password: &[u8], encoded: &str) -> Result<bool, Error> {
    let parsed = parse_phc(encoded)?;
    if parsed.alg != "argon2id" {
        return Err(Error::Algorithm);
    }
    let mut out = vec![0u8; parsed.tag.len()];
    raw_hash(
        password,
        &parsed.salt,
        &[],
        &[],
        parsed.m,
        parsed.t,
        parsed.p,
        parsed.tag.len(),
        &mut out,
    )?;
    Ok(ct_eq(&out, &parsed.tag))
}

/// Hash with caller salt and parameters; PHC-encoded.
pub fn hash_password_with_params(
    password: &[u8],
    salt: &[u8],
    m: u32,
    t: u32,
    p: u32,
) -> Result<String, Error> {
    if salt.len() < 8 {
        return Err(Error::Salt);
    }
    let mut tag = [0u8; TAG_LEN];
    raw_hash(password, salt, &[], &[], m, t, p, TAG_LEN, &mut tag)?;
    Ok(encode_phc(m, t, p, salt, &tag))
}

/// Raw Argon2id into `out`. Optional `secret` and `ad` (RFC 9106 K, X).
#[allow(clippy::too_many_arguments)]
pub fn raw_hash(
    password: &[u8],
    salt: &[u8],
    secret: &[u8],
    ad: &[u8],
    m: u32,
    t: u32,
    p: u32,
    out_len: usize,
    out: &mut [u8],
) -> Result<(), Error> {
    if out.len() < out_len || !(4..=0xFFFF_FFFF).contains(&(out_len as u64)) {
        return Err(Error::Output);
    }
    if p == 0 || t == 0 {
        return Err(Error::Params);
    }
    let p_us = p as usize;
    let m_prime = 4 * p_us * (m as usize / (4 * p_us)).max(1);
    if m_prime < 8 * p_us {
        return Err(Error::Params);
    }
    let lane_length = m_prime / p_us;
    let segment_length = lane_length / SYNC_POINTS;

    let h0 = initial_hash(password, salt, secret, ad, m, t, p, out_len as u32);

    let mut memory = vec![[0u64; BLOCK_U64S]; m_prime];
    for lane in 0..p_us {
        for i in 0..2 {
            let mut block_bytes = [0u8; BLOCK_SIZE];
            h_prime(
                &[&h0, &(i as u32).to_le_bytes(), &(lane as u32).to_le_bytes()],
                &mut block_bytes,
            );
            load_block(&mut memory[lane * lane_length + i], &block_bytes);
        }
    }

    for pass in 0..t as usize {
        for slice in 0..SYNC_POINTS {
            for lane in 0..p_us {
                fill_segment(
                    &mut memory,
                    pass,
                    slice,
                    lane,
                    p_us,
                    m_prime,
                    t as usize,
                    lane_length,
                    segment_length,
                );
            }
        }
    }

    let mut c = memory[lane_length - 1];
    for lane in 1..p_us {
        xor_block(&mut c, &memory[lane * lane_length + lane_length - 1]);
    }
    let mut c_bytes = [0u8; BLOCK_SIZE];
    store_block(&c, &mut c_bytes);
    h_prime(&[&c_bytes], &mut out[..out_len]);
    Ok(())
}

/// Hasher error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Unsupported PHC algorithm.
    Algorithm,
    /// Salt too short.
    Salt,
    /// Output buffer too small.
    Output,
    /// Illegal m/t/p.
    Params,
    /// PHC string could not be parsed.
    Phc,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Algorithm => f.write_str("unsupported argon2 algorithm"),
            Error::Salt => f.write_str("salt too short"),
            Error::Output => f.write_str("output too short"),
            Error::Params => f.write_str("illegal argon2 parameters"),
            Error::Phc => f.write_str("invalid PHC string"),
        }
    }
}

impl std::error::Error for Error {}

struct Phc {
    alg: String,
    m: u32,
    t: u32,
    p: u32,
    salt: Vec<u8>,
    tag: Vec<u8>,
}

#[allow(clippy::too_many_arguments)]
fn initial_hash(
    password: &[u8],
    salt: &[u8],
    secret: &[u8],
    ad: &[u8],
    m: u32,
    t: u32,
    p: u32,
    out_len: u32,
) -> [u8; 64] {
    let mut msg = Vec::new();
    msg.extend_from_slice(&p.to_le_bytes());
    msg.extend_from_slice(&out_len.to_le_bytes());
    msg.extend_from_slice(&m.to_le_bytes());
    msg.extend_from_slice(&t.to_le_bytes());
    msg.extend_from_slice(&VERSION.to_le_bytes());
    msg.extend_from_slice(&2u32.to_le_bytes()); // y = Argon2id
    msg.extend_from_slice(&(password.len() as u32).to_le_bytes());
    msg.extend_from_slice(password);
    msg.extend_from_slice(&(salt.len() as u32).to_le_bytes());
    msg.extend_from_slice(salt);
    msg.extend_from_slice(&(secret.len() as u32).to_le_bytes());
    msg.extend_from_slice(secret);
    msg.extend_from_slice(&(ad.len() as u32).to_le_bytes());
    msg.extend_from_slice(ad);
    let v = blake2b::hash(&msg, 64);
    let mut out = [0u8; 64];
    out.copy_from_slice(&v);
    out
}

fn h_prime(parts: &[&[u8]], out: &mut [u8]) {
    let t = out.len();
    let mut msg = Vec::new();
    msg.extend_from_slice(&(t as u32).to_le_bytes());
    for p in parts {
        msg.extend_from_slice(p);
    }
    if t <= 64 {
        let v = blake2b::hash(&msg, t);
        out.copy_from_slice(&v);
        return;
    }
    let r = t.div_ceil(32) - 2;
    let v1 = blake2b::hash(&msg, 64);
    out[..32].copy_from_slice(&v1[..32]);
    let mut prev = v1;
    for i in 1..r {
        prev = blake2b::hash(&prev, 64);
        let off = i * 32;
        out[off..off + 32].copy_from_slice(&prev[..32]);
    }
    let last_len = t - 32 * r;
    let last = blake2b::hash(&prev, last_len);
    out[32 * r..].copy_from_slice(&last);
}

#[allow(clippy::too_many_arguments, clippy::explicit_counter_loop)]
fn fill_segment(
    memory: &mut [[u64; BLOCK_U64S]],
    pass: usize,
    slice: usize,
    lane: usize,
    lanes: usize,
    block_count: usize,
    iterations: usize,
    lane_length: usize,
    segment_length: usize,
) {
    let data_independent = pass == 0 && slice < SYNC_POINTS / 2;
    let mut address_block = [0u64; BLOCK_U64S];
    let mut input_block = [0u64; BLOCK_U64S];
    let zero = [0u64; BLOCK_U64S];
    if data_independent {
        input_block[0] = pass as u64;
        input_block[1] = lane as u64;
        input_block[2] = slice as u64;
        input_block[3] = block_count as u64;
        input_block[4] = iterations as u64;
        input_block[5] = 2; // Argon2id
    }

    let first_block = if pass == 0 && slice == 0 {
        if data_independent {
            update_address_block(&mut address_block, &mut input_block, &zero);
        }
        2
    } else {
        0
    };

    let mut cur_index = lane * lane_length + slice * segment_length + first_block;
    let mut prev_index = if slice == 0 && first_block == 0 {
        cur_index + lane_length - 1
    } else {
        cur_index - 1
    };

    for block in first_block..segment_length {
        let rand = if data_independent {
            let address_index = block % 128;
            if address_index == 0 {
                update_address_block(&mut address_block, &mut input_block, &zero);
            }
            address_block[address_index]
        } else {
            memory[prev_index][0]
        };

        let ref_lane = if pass == 0 && slice == 0 {
            lane
        } else {
            ((rand >> 32) as usize) % lanes
        };

        let reference_area_size = if pass == 0 {
            if slice == 0 {
                block - 1
            } else if ref_lane == lane {
                slice * segment_length + block - 1
            } else {
                slice * segment_length - usize::from(block == 0)
            }
        } else if ref_lane == lane {
            lane_length - segment_length + block - 1
        } else {
            lane_length - segment_length - usize::from(block == 0)
        };

        let mut map = rand & 0xFFFF_FFFF;
        map = (map.wrapping_mul(map)) >> 32;
        let relative_position = reference_area_size
            - 1
            - (((reference_area_size as u64).wrapping_mul(map)) >> 32) as usize;

        let start_position = if pass != 0 && slice != SYNC_POINTS - 1 {
            (slice + 1) * segment_length
        } else {
            0
        };
        let lane_index = (start_position + relative_position) % lane_length;
        let ref_index = ref_lane * lane_length + lane_index;

        let result = compress(&memory[prev_index], &memory[ref_index]);
        if pass == 0 {
            memory[cur_index] = result;
        } else {
            xor_block(&mut memory[cur_index], &result);
        }
        prev_index = cur_index;
        cur_index += 1;
    }
}

fn update_address_block(
    address_block: &mut [u64; BLOCK_U64S],
    input_block: &mut [u64; BLOCK_U64S],
    zero: &[u64; BLOCK_U64S],
) {
    input_block[6] += 1;
    *address_block = compress(zero, input_block);
    *address_block = compress(zero, address_block);
}

fn compress(rhs: &[u64; BLOCK_U64S], lhs: &[u64; BLOCK_U64S]) -> [u64; BLOCK_U64S] {
    let mut r = [0u64; BLOCK_U64S];
    for i in 0..BLOCK_U64S {
        r[i] = rhs[i] ^ lhs[i];
    }
    let mut q = r;
    for chunk in q.as_chunks_mut::<16>().0 {
        permute16(chunk);
    }
    for i in 0..8 {
        let b = i * 2;
        let mut col = [
            q[b],
            q[b + 1],
            q[b + 16],
            q[b + 17],
            q[b + 32],
            q[b + 33],
            q[b + 48],
            q[b + 49],
            q[b + 64],
            q[b + 65],
            q[b + 80],
            q[b + 81],
            q[b + 96],
            q[b + 97],
            q[b + 112],
            q[b + 113],
        ];
        permute16(&mut col);
        q[b] = col[0];
        q[b + 1] = col[1];
        q[b + 16] = col[2];
        q[b + 17] = col[3];
        q[b + 32] = col[4];
        q[b + 33] = col[5];
        q[b + 48] = col[6];
        q[b + 49] = col[7];
        q[b + 64] = col[8];
        q[b + 65] = col[9];
        q[b + 80] = col[10];
        q[b + 81] = col[11];
        q[b + 96] = col[12];
        q[b + 97] = col[13];
        q[b + 112] = col[14];
        q[b + 113] = col[15];
    }
    for i in 0..BLOCK_U64S {
        q[i] ^= r[i];
    }
    q
}

fn permute16(v: &mut [u64]) {
    gb_at(v, 0, 4, 8, 12);
    gb_at(v, 1, 5, 9, 13);
    gb_at(v, 2, 6, 10, 14);
    gb_at(v, 3, 7, 11, 15);
    gb_at(v, 0, 5, 10, 15);
    gb_at(v, 1, 6, 11, 12);
    gb_at(v, 2, 7, 8, 13);
    gb_at(v, 3, 4, 9, 14);
}

fn gb_at(v: &mut [u64], ia: usize, ib: usize, ic: usize, id: usize) {
    let mut a = v[ia];
    let mut b = v[ib];
    let mut c = v[ic];
    let mut d = v[id];
    gb(&mut a, &mut b, &mut c, &mut d);
    v[ia] = a;
    v[ib] = b;
    v[ic] = c;
    v[id] = d;
}

fn gb(a: &mut u64, b: &mut u64, c: &mut u64, d: &mut u64) {
    const TRUNC: u64 = u32::MAX as u64;
    *a = a
        .wrapping_add(*b)
        .wrapping_add(2u64.wrapping_mul((*a & TRUNC).wrapping_mul(*b & TRUNC)));
    *d = (*d ^ *a).rotate_right(32);
    *c = c
        .wrapping_add(*d)
        .wrapping_add(2u64.wrapping_mul((*c & TRUNC).wrapping_mul(*d & TRUNC)));
    *b = (*b ^ *c).rotate_right(24);
    *a = a
        .wrapping_add(*b)
        .wrapping_add(2u64.wrapping_mul((*a & TRUNC).wrapping_mul(*b & TRUNC)));
    *d = (*d ^ *a).rotate_right(16);
    *c = c
        .wrapping_add(*d)
        .wrapping_add(2u64.wrapping_mul((*c & TRUNC).wrapping_mul(*d & TRUNC)));
    *b = (*b ^ *c).rotate_right(63);
}

fn xor_block(dst: &mut [u64; BLOCK_U64S], src: &[u64; BLOCK_U64S]) {
    for i in 0..BLOCK_U64S {
        dst[i] ^= src[i];
    }
}

fn load_block(dst: &mut [u64; BLOCK_U64S], src: &[u8; BLOCK_SIZE]) {
    for (i, chunk) in src.as_chunks::<8>().0.iter().enumerate() {
        dst[i] = u64::from_le_bytes(*chunk);
    }
}

fn store_block(src: &[u64; BLOCK_U64S], dst: &mut [u8; BLOCK_SIZE]) {
    for (i, word) in src.iter().enumerate() {
        dst[i * 8..(i + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
}

fn encode_phc(m: u32, t: u32, p: u32, salt: &[u8], tag: &[u8]) -> String {
    format!(
        "$argon2id$v=19$m={m},t={t},p={p}${}${}",
        b64_encode(salt),
        b64_encode(tag)
    )
}

fn parse_phc(s: &str) -> Result<Phc, Error> {
    let parts: Vec<&str> = s.split('$').collect();
    // ["", "argon2id", "v=19", "m=..,t=..,p=..", salt, tag]
    if parts.len() != 6 || !parts[0].is_empty() {
        return Err(Error::Phc);
    }
    let alg = parts[1].to_string();
    if parts[2] != "v=19" {
        return Err(Error::Phc);
    }
    let mut m = 0u32;
    let mut t = 0u32;
    let mut p = 0u32;
    for kv in parts[3].split(',') {
        let Some((k, v)) = kv.split_once('=') else {
            return Err(Error::Phc);
        };
        let n: u32 = v.parse().map_err(|_| Error::Phc)?;
        match k {
            "m" => m = n,
            "t" => t = n,
            "p" => p = n,
            _ => return Err(Error::Phc),
        }
    }
    if m == 0 || t == 0 || p == 0 {
        return Err(Error::Phc);
    }
    Ok(Phc {
        alg,
        m,
        t,
        p,
        salt: b64_decode(parts[4]).ok_or(Error::Phc)?,
        tag: b64_decode(parts[5]).ok_or(Error::Phc)?,
    })
}

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (a << 16) | (b << 8) | c;
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64[(n & 63) as usize] as char);
        }
    }
    out
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut vals = Vec::new();
    for b in s.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => continue,
            _ => return None,
        };
        vals.push(v);
    }
    let mut out = Vec::new();
    for chunk in vals.chunks(4) {
        let a = *chunk.first()? as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let d = chunk.get(3).copied().unwrap_or(0) as u32;
        let n = (a << 18) | (b << 12) | (c << 6) | d;
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

fn random_salt() -> [u8; SALT_LEN] {
    let a = uuid::Uuid::now_v7();
    let mut s = [0u8; SALT_LEN];
    s.copy_from_slice(a.as_bytes());
    s
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for (x, y) in a.iter().zip(b) {
        d |= x ^ y;
    }
    d == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
            .collect()
    }

    #[test]
    fn rfc9106_argon2id_known_answer() {
        let password = [0x01u8; 32];
        let salt = [0x02u8; 16];
        let secret = [0x03u8; 8];
        let ad = [0x04u8; 12];
        let mut out = [0u8; 32];
        raw_hash(&password, &salt, &secret, &ad, 32, 3, 4, 32, &mut out).expect("hash");
        let expect = hex("0d 64 0d f5 8d 78 76 6c 08 c0 37 a3 4a 8b 53 c9 d0
             1e f0 45 2d 75 b6 5e b5 25 20 e9 6b 01 e6 59");
        assert_eq!(out.as_slice(), expect.as_slice());
    }

    #[test]
    fn rfc9106_h0_matches() {
        let password = [0x01u8; 32];
        let salt = [0x02u8; 16];
        let secret = [0x03u8; 8];
        let ad = [0x04u8; 12];
        let h0 = initial_hash(&password, &salt, &secret, &ad, 32, 3, 4, 32);
        let expect = hex("28 89 de 48 7e b4 2a e5 00 c0 00 7e d9 25 2f
             10 69 ea de c4 0d 57 65 b4 85 de 6d c2 43 7a 67 b8 54 6a 2f 0a
             cc 1a 08 82 db 8f cf 74 71 4b 47 2e 94 df 42 1a 5d a1 11 2f fa
             11 43 43 70 a1 e9 97");
        assert_eq!(h0.as_slice(), expect.as_slice());
    }

    #[test]
    fn argon2id_parameters_are_pinned_and_tested() {
        assert_eq!(M_KIB, 19_456);
        assert_eq!(T_COST, 2);
        assert_eq!(P_COST, 1);
        assert_eq!(TAG_LEN, 32);
        assert_eq!(SALT_LEN, 16);
        assert_eq!(VERSION, 0x13);

        let salt = b"wicket-identity16";
        let phc =
            hash_password_with_params(b"password", salt, M_KIB, T_COST, P_COST).expect("hash");
        assert!(phc.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_password(b"password", &phc).expect("verify"));
        assert!(!verify_password(b"wrong", &phc).expect("verify wrong"));
    }

    #[test]
    fn phc_roundtrip_small() {
        let salt = b"somesalt";
        let phc = hash_password_with_params(b"password", salt, 16, 2, 1).expect("hash");
        assert!(verify_password(b"password", &phc).expect("ok"));
        assert!(!verify_password(b"passwore", &phc).expect("no"));
    }
}
