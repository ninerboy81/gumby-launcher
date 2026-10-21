//! Base64 for the locked password blobs in accounts.json.
const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 { out.push(if i <= c.len() { ABC[(n >> (18 - 6 * i)) as usize & 63] as char } else { '=' }); }
    }
    out
}

pub fn decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| ABC.iter().position(|&x| x == c).map(|v| v as u32);
    let b: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if b.len() % 4 != 0 { return None; }
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    for q in b.chunks(4) {
        let pad = q.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 { return None; }
        let mut n = 0u32;
        for (i, &c) in q.iter().enumerate() { n = (n << 6) | if i >= 4 - pad { 0 } else { val(c)? }; }
        out.push((n >> 16) as u8);
        if pad < 2 { out.push((n >> 8) as u8); }
        if pad < 1 { out.push(n as u8); }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        for n in 0..40usize {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(super::decode(&super::encode(&data)).unwrap(), data);
        }
        assert_eq!(super::encode(b"Man"), "TWFu");
        assert_eq!(super::encode(b"Ma"), "TWE=");
        assert!(super::decode("abc").is_none());
        assert!(super::decode("a===").is_none());
    }
}
