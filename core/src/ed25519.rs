//! Ed25519 signatures (RFC 8032), with the SHA-512 and the small big-number arithmetic it needs. Checks the publisher's
//! signature on an update's manifest (and signs, for the publish tool and the tests). Not constant-time.

// ---------------------------------------------------------------------------------------------------- SHA-512
const K512: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc, 0x3956c25bf348b538, 0x59f111f1b605d019,
    0x923f82a4af194f9b, 0xab1c5ed5da6d8118, 0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694, 0xe49b69c19ef14ad2, 0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65, 0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4, 0xc6e00bf33da88fc2, 0xd5a79147930aa725,
    0x06ca6351e003826f, 0x142929670a0e6e70, 0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
    0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b, 0xa2bfe8a14cf10364, 0xa81a664bbc423001,
    0xc24b8b70d0f89791, 0xc76c51a30654be30, 0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8, 0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3, 0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b, 0xca273eceea26619c, 0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178, 0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c, 0x4cc5d4becb3e42b6, 0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

pub fn sha512(data: &[u8]) -> [u8; 64] {
    let mut h: [u64; 8] = [0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1, 0x510e527fade682d1,
        0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179];
    let mut msg = data.to_vec();
    let bits = (data.len() as u128) * 8;
    msg.push(0x80);
    while msg.len() % 128 != 112 { msg.push(0); }
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.chunks(128) {
        let mut w = [0u64; 80];
        for i in 0..16 { w[i] = u64::from_be_bytes(block[8 * i..8 * i + 8].try_into().unwrap()); }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K512[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1); d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) { *x = x.wrapping_add(y); }
    }
    let mut out = [0u8; 64];
    for (i, v) in h.iter().enumerate() { out[8 * i..8 * i + 8].copy_from_slice(&v.to_be_bytes()); }
    out
}

// ------------------------------------------------------------------------------------- big unsigned integers
/// Little-endian u32 limbs; just enough arithmetic for the field and group order of Ed25519.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big(Vec<u32>);

impl Big {
    pub fn zero() -> Big { Big(vec![]) }
    pub fn from_u64(v: u64) -> Big { let mut b = Big(vec![v as u32, (v >> 32) as u32]); b.trim(); b }
    pub fn from_le(bytes: &[u8]) -> Big {
        let mut v = Vec::new();
        for c in bytes.chunks(4) { let mut w = [0u8; 4]; w[..c.len()].copy_from_slice(c); v.push(u32::from_le_bytes(w)); }
        let mut b = Big(v); b.trim(); b
    }
    pub fn to_le(&self, n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for w in &self.0 { out.extend_from_slice(&w.to_le_bytes()); }
        out.resize(n, 0);
        out
    }
    fn trim(&mut self) { while self.0.last() == Some(&0) { self.0.pop(); } }
    pub fn is_zero(&self) -> bool { self.0.is_empty() }
    pub fn bit(&self, i: usize) -> bool { self.0.get(i / 32).map_or(false, |w| (w >> (i % 32)) & 1 == 1) }
    pub fn bits(&self) -> usize { match self.0.last() { None => 0, Some(w) => (self.0.len() - 1) * 32 + (32 - w.leading_zeros() as usize) } }
    pub fn cmp(&self, o: &Big) -> std::cmp::Ordering {
        if self.0.len() != o.0.len() { return self.0.len().cmp(&o.0.len()); }
        for i in (0..self.0.len()).rev() { if self.0[i] != o.0[i] { return self.0[i].cmp(&o.0[i]); } }
        std::cmp::Ordering::Equal
    }
    pub fn add(&self, o: &Big) -> Big {
        let n = self.0.len().max(o.0.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let s = *self.0.get(i).unwrap_or(&0) as u64 + *o.0.get(i).unwrap_or(&0) as u64 + carry;
            out.push(s as u32); carry = s >> 32;
        }
        if carry > 0 { out.push(carry as u32); }
        let mut b = Big(out); b.trim(); b
    }
    /// self - o (self >= o).
    pub fn sub(&self, o: &Big) -> Big {
        let mut out = Vec::with_capacity(self.0.len());
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let mut d = self.0[i] as i64 - *o.0.get(i).unwrap_or(&0) as i64 - borrow;
            if d < 0 { d += 1 << 32; borrow = 1; } else { borrow = 0; }
            out.push(d as u32);
        }
        let mut b = Big(out); b.trim(); b
    }
    pub fn mul(&self, o: &Big) -> Big {
        if self.is_zero() || o.is_zero() { return Big::zero(); }
        let mut out = vec![0u64; self.0.len() + o.0.len() + 1];
        for (i, a) in self.0.iter().enumerate() {
            let mut carry = 0u64;
            for (j, b) in o.0.iter().enumerate() {
                let t = out[i + j] + (*a as u64) * (*b as u64) + carry;
                out[i + j] = t & 0xffff_ffff; carry = t >> 32;
            }
            let mut k = i + o.0.len();
            while carry > 0 { let t = out[k] + carry; out[k] = t & 0xffff_ffff; carry = t >> 32; k += 1; }
        }
        let mut b = Big(out.into_iter().map(|x| x as u32).collect()); b.trim(); b
    }
    fn shl1(&self) -> Big { self.add(self) }
    /// self mod m (binary long division; fine for 512-bit inputs).
    pub fn rem(&self, m: &Big) -> Big {
        if self.cmp(m) == std::cmp::Ordering::Less { return self.clone(); }
        let mut r = Big::zero();
        for i in (0..self.bits()).rev() {
            r = r.shl1();
            if self.bit(i) { r = r.add(&Big::from_u64(1)); }
            if r.cmp(m) != std::cmp::Ordering::Less { r = r.sub(m); }
        }
        r
    }
}

// ----------------------------------------------------------------------------------------------- the curve
fn p() -> Big { Big::from_le(&{ let mut b = [0xffu8; 32]; b[0] = 0xed; b[31] = 0x7f; b }) }
fn order() -> Big {
    // 2^252 + 27742317777372353535851937790883648493
    Big::from_le(&[0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10])
}

struct F { p: Big }

/// x mod (2^255 - 19): fold the bits above 255 back in as 19 * high (x < 2^512).
fn reduce_p(x: Big, p: &Big) -> Big {
    let mut x = x;
    while x.bits() > 255 {
        let mut lo = x.0.clone();
        lo.truncate(8);
        if lo.len() == 8 { lo[7] &= 0x7fff_ffff; }
        let mut lo = Big(lo); lo.trim();
        let mut hi = Vec::new();
        for i in 0..x.0.len() {
            if i < 7 { continue; }
            let w = (x.0[i] >> 31) | (x.0.get(i + 1).copied().unwrap_or(0) << 1);
            hi.push(w);
        }
        let mut hi = Big(hi); hi.trim();
        x = lo.add(&hi.mul(&Big::from_u64(19)));
    }
    while x.cmp(p) != std::cmp::Ordering::Less { x = x.sub(p); }
    x
}

impl F {
    fn m(&self, a: &Big, b: &Big) -> Big { reduce_p(a.mul(b), &self.p) }
    fn add(&self, a: &Big, b: &Big) -> Big { reduce_p(a.add(b), &self.p) }
    fn sub(&self, a: &Big, b: &Big) -> Big { reduce_p(a.add(&self.p).sub(&reduce_p(b.clone(), &self.p)), &self.p) }
    fn pow(&self, base: &Big, e: &Big) -> Big {
        let mut r = Big::from_u64(1);
        let b = reduce_p(base.clone(), &self.p);
        for i in (0..e.bits()).rev() {
            r = self.m(&r, &r);
            if e.bit(i) { r = self.m(&r, &b); }
        }
        r
    }
    fn inv(&self, a: &Big) -> Big { self.pow(a, &self.p.sub(&Big::from_u64(2))) }
    fn d(&self) -> Big {
        // -121665 / 121666 mod p
        let n = self.p.sub(&Big::from_u64(121665));
        self.m(&n, &self.inv(&Big::from_u64(121666)))
    }
}

type Pt = [Big; 4];

fn pt_add(f: &F, d: &Big, a: &Pt, b: &Pt) -> Pt {
    let aa = f.m(&f.sub(&a[1], &a[0]), &f.sub(&b[1], &b[0]));
    let bb = f.m(&f.add(&a[1], &a[0]), &f.add(&b[1], &b[0]));
    let c = f.m(&f.m(&Big::from_u64(2), &f.m(&a[3], &b[3])), d);
    let dd = f.m(&Big::from_u64(2), &f.m(&a[2], &b[2]));
    let (e, ff, g, h) = (f.sub(&bb, &aa), f.sub(&dd, &c), f.add(&dd, &c), f.add(&bb, &aa));
    [f.m(&e, &ff), f.m(&g, &h), f.m(&ff, &g), f.m(&e, &h)]
}

fn pt_mul(f: &F, d: &Big, s: &Big, p0: &Pt) -> Pt {
    let mut q: Pt = [Big::zero(), Big::from_u64(1), Big::from_u64(1), Big::zero()];
    let mut p = p0.clone();
    for i in 0..s.bits() {
        if s.bit(i) { q = pt_add(f, d, &q, &p); }
        p = pt_add(f, d, &p, &p);
    }
    q
}

fn pt_equal(f: &F, a: &Pt, b: &Pt) -> bool {
    f.sub(&f.m(&a[0], &b[2]), &f.m(&b[0], &a[2])).is_zero() && f.sub(&f.m(&a[1], &b[2]), &f.m(&b[1], &a[2])).is_zero()
}

fn recover_x(f: &F, d: &Big, y: &Big, sign: bool) -> Option<Big> {
    if y.cmp(&f.p) != std::cmp::Ordering::Less { return None; }
    let yy = f.m(y, y);
    let x2 = f.m(&f.sub(&yy, &Big::from_u64(1)), &f.inv(&f.add(&f.m(d, &yy), &Big::from_u64(1))));
    if x2.is_zero() { return if sign { None } else { Some(Big::zero()) }; }
    let e = f.p.add(&Big::from_u64(3));
    let e = Big::from_le(&{ let mut v = e.to_le(33); shr3(&mut v); v });
    let mut x = f.pow(&x2, &e);
    if !f.sub(&f.m(&x, &x), &x2).is_zero() {
        let e4 = { let mut v = f.p.sub(&Big::from_u64(1)).to_le(33); shr2(&mut v); Big::from_le(&v) };
        x = f.m(&x, &f.pow(&Big::from_u64(2), &e4));
    }
    if !f.sub(&f.m(&x, &x), &x2).is_zero() { return None; }
    if x.bit(0) != sign { x = f.p.sub(&x); }
    Some(x)
}

fn shr_bits(v: &mut [u8], n: u32) {
    let mut carry = 0u8;
    for b in v.iter_mut().rev() { let nb = (*b >> n) | carry; carry = *b << (8 - n); *b = nb; }
}
fn shr3(v: &mut [u8]) { shr_bits(v, 3); }
fn shr2(v: &mut [u8]) { shr_bits(v, 2); }

fn base(f: &F, d: &Big) -> Pt {
    let gy = f.m(&Big::from_u64(4), &f.inv(&Big::from_u64(5)));
    let gx = recover_x(f, d, &gy, false).unwrap();
    let t = f.m(&gx, &gy);
    [gx, gy, Big::from_u64(1), t]
}

fn decompress(f: &F, d: &Big, s: &[u8]) -> Option<Pt> {
    if s.len() != 32 { return None; }
    let mut b = s.to_vec();
    let sign = b[31] >> 7 == 1;
    b[31] &= 0x7f;
    let y = Big::from_le(&b);
    let x = recover_x(f, d, &y, sign)?;
    let t = f.m(&x, &y);
    Some([x, y, Big::from_u64(1), t])
}

fn compress(f: &F, p: &Pt) -> Vec<u8> {
    let zi = f.inv(&p[2]);
    let (x, y) = (f.m(&p[0], &zi), f.m(&p[1], &zi));
    let mut out = y.to_le(32);
    if x.bit(0) { out[31] |= 0x80; }
    out
}

fn modq(data: &[u8]) -> Big { Big::from_le(&sha512(data)).rem(&order()) }

/// `verify(public, message, signature)`.
pub fn verify(public: &[u8], message: &[u8], signature: &[u8]) -> bool {
    if public.len() != 32 || signature.len() != 64 { return false; }
    let f = F { p: p() };
    let d = f.d();
    let (a, r) = match (decompress(&f, &d, public), decompress(&f, &d, &signature[..32])) { (Some(a), Some(r)) => (a, r), _ => return false };
    let s = Big::from_le(&signature[32..]);
    if s.cmp(&order()) != std::cmp::Ordering::Less { return false; }
    let mut hm = signature[..32].to_vec();
    hm.extend_from_slice(public);
    hm.extend_from_slice(message);
    let h = modq(&hm);
    let g = base(&f, &d);
    pt_equal(&f, &pt_mul(&f, &d, &s, &g), &pt_add(&f, &d, &r, &pt_mul(&f, &d, &h, &a)))
}

/// `sign(secret, message)`: the publish tool signs a release manifest with it.
pub fn sign(secret: &[u8; 32], message: &[u8]) -> Vec<u8> {
    let f = F { p: p() };
    let d = f.d();
    let h = sha512(secret);
    let mut a = h[..32].to_vec();
    a[0] &= 248; a[31] &= 63; a[31] |= 64;
    let a = Big::from_le(&a);
    let g = base(&f, &d);
    let pa = compress(&f, &pt_mul(&f, &d, &a, &g));
    let mut rm = h[32..].to_vec();
    rm.extend_from_slice(message);
    let r = modq(&rm);
    let pr = compress(&f, &pt_mul(&f, &d, &r, &g));
    let mut km = pr.clone();
    km.extend_from_slice(&pa);
    km.extend_from_slice(message);
    let k = modq(&km);
    let s = r.add(&k.mul(&a)).rem(&order());
    let mut out = pr;
    out.extend_from_slice(&s.to_le(32));
    out
}

pub fn public_key(secret: &[u8; 32]) -> Vec<u8> {
    let f = F { p: p() };
    let d = f.d();
    let h = sha512(secret);
    let mut a = h[..32].to_vec();
    a[0] &= 248; a[31] &= 63; a[31] |= 64;
    compress(&f, &pt_mul(&f, &d, &Big::from_le(&a), &base(&f, &d)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hex(s: &str) -> Vec<u8> { (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect() }
    #[test]
    fn sha512_vectors() {
        assert_eq!(crate::sha256::hex(&sha512(b"abc")), "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f");
        assert_eq!(crate::sha256::hex(&sha512(b"")), "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e");
    }
    #[test]
    fn rfc8032_test1() {
        // RFC 8032 7.1 TEST 1 (empty message)
        let sk: [u8; 32] = hex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60").try_into().unwrap();
        let pk = hex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
        let sig = hex("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
        assert_eq!(public_key(&sk), pk);
        assert_eq!(sign(&sk, b""), sig);
        assert!(verify(&pk, b"", &sig));
        let mut bad = sig.clone(); bad[5] ^= 1;
        assert!(!verify(&pk, b"", &bad));
        assert!(!verify(&pk, b"x", &sig));
    }
}
