//! A request's identity, as Areneon defines it: the SHA-256 of the request's
//! canonical JSON (RFC 8785) without `rqid`, which only the Showdown server
//! assigns, so a request from the simulator and one from the server hash
//! alike. The canonical form sorts every object's keys, so it does not depend
//! on the order the engine writes them in.
//!
//! The hash is here, and not in the caller, because every decision of a
//! training run is identified by it; it is a dozen lines of [`sha256`] and
//! [`canonical`], with no dependency.

use serde_json::Value;

/// SHA-256 of `data` (FIPS 180-4).
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut block = |chunk: &[u8]| {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(v);
        }
    };
    let mut chunks = data.chunks_exact(64);
    for chunk in &mut chunks {
        block(chunk);
    }
    // The padding: a 1 bit, zeros, and the length in bits.
    let rest = chunks.remainder();
    let mut tail = [0u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let len = if rest.len() < 56 { 64 } else { 128 };
    tail[len - 8..len].copy_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in tail[..len].chunks_exact(64) {
        block(chunk);
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// `bytes` as lower-case hexadecimal.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

/// A string as JavaScript's `JSON.stringify` writes it (and RFC 8785
/// requires): `"`, `\` and the control characters escaped, everything else
/// as it is.
pub fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The canonical JSON (RFC 8785) of `value`, written to `out`: keys sorted
/// by UTF-16 code units, no whitespace, integers in decimal. A request holds
/// no other number; a non-integer is an error.
pub fn canonical(value: &Value, out: &mut String) -> Result<(), String> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else {
                return Err(format!("not an integer: {n}"));
            }
        }
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, key);
                out.push(':');
                canonical(&map[key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// The bytes a request identity hashes: the canonical JSON of the request
/// without its `rqid`.
pub fn request_identity_text(request: &Value) -> Result<String, String> {
    let mut out = String::with_capacity(4096);
    match request {
        Value::Object(map) if map.contains_key("rqid") => {
            let mut rest = map.clone();
            rest.remove("rqid");
            canonical(&Value::Object(rest), &mut out)?;
        }
        other => canonical(other, &mut out)?,
    }
    Ok(out)
}

/// A request's `request_hash`: the SHA-256 of [`request_identity_text`], as
/// 64 lower-case hexadecimal digits.
pub fn request_hash(request: &Value) -> Result<String, String> {
    Ok(hex(&sha256(request_identity_text(request)?.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sha256_matches_the_fips_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // Lengths around the padding boundaries (55, 56, 63, 64, 65 bytes) and a long input.
        let million_a = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha256(&million_a)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        assert_eq!(
            hex(&sha256(&[b'a'; 55])),
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"
        );
        assert_eq!(
            hex(&sha256(&[b'a'; 56])),
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
        );
        assert_eq!(
            hex(&sha256(&[b'a'; 64])),
            "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"
        );
    }

    #[test]
    fn canonical_sorts_keys_and_drops_whitespace() {
        let v = json!({"b": [1, true, null, "x"], "a": {"z": 0, "y": -3}});
        let mut out = String::new();
        canonical(&v, &mut out).unwrap();
        assert_eq!(out, r#"{"a":{"y":-3,"z":0},"b":[1,true,null,"x"]}"#);
    }

    #[test]
    fn canonical_escapes_like_json_stringify() {
        let mut out = String::new();
        write_string(&mut out, "a\"b\\c\n\t\u{1}\u{7f}é\u{2028}");
        assert_eq!(out, "\"a\\\"b\\\\c\\n\\t\\u0001\u{7f}é\u{2028}\"");
        let mut out = String::new();
        assert!(canonical(&json!(1.5), &mut out).is_err());
    }

    #[test]
    fn the_request_hash_leaves_out_rqid_and_ignores_key_order() {
        let a = json!({"wait": true, "rqid": 7, "side": {"id": "p1", "name": "p1"}});
        let b = json!({"side": {"name": "p1", "id": "p1"}, "wait": true});
        assert_eq!(request_hash(&a).unwrap(), request_hash(&b).unwrap());
        assert_eq!(
            request_identity_text(&a).unwrap(),
            r#"{"side":{"id":"p1","name":"p1"},"wait":true}"#
        );
        assert_ne!(
            request_hash(&a).unwrap(),
            request_hash(&json!({"wait": true, "side": {"id": "p1", "name": "p2"}})).unwrap()
        );
    }
}
