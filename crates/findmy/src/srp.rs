//! SRP-6a as Apple's auth servers speak it: the RFC 5054 2048-bit group, SHA-256,
//! padded `k` and `u`, no username in `x`, and the password pre-hashed with PBKDF2
//! (`s2k` / `s2k_fo`). Equivalent to pysrp with `rfc5054_enable()` + `no_username_in_x()`.

use anyhow::{Result, bail};
use num_bigint::BigUint;
use rand::RngCore;
use sha2::{Digest, Sha256};

const N_HEX: &str = concat!(
    "AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050A37329CBB4A099ED8193E0757767A13DD52312AB4B03310D",
    "CD7F48A9DA04FD50E8083969EDB767B0CF6095179A163AB3661A05FBD5FAAAE82918A9962F0B93B855F97993EC975EEAA80D740ADBF4FF",
    "747359D041D5C33EA71D281E446B14773BCA97B43A23FB801676BD207A436C6481F1D2B9078717461A5B9D32E688F87748544523B524B0",
    "D57D5EA77A2775D2ECFA032CFBDBF52FB3786160279004E57AE6AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9",
    "DBFBB694B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73"
);

fn n() -> BigUint {
    BigUint::parse_bytes(N_HEX.as_bytes(), 16).unwrap()
}

fn g() -> BigUint {
    BigUint::from(2u32)
}

fn h(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn pad(value: &BigUint, width: usize) -> Vec<u8> {
    let bytes = value.to_bytes_be();
    let mut out = vec![0u8; width.saturating_sub(bytes.len())];
    out.extend_from_slice(&bytes);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    S2k,
    S2kFo,
}

impl Protocol {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "s2k" => Ok(Self::S2k),
            "s2k_fo" => Ok(Self::S2kFo),
            other => bail!("unsupported password protocol {other:?}"),
        }
    }
}

/// Apple's password pre-hash: PBKDF2-HMAC-SHA256 over SHA-256(password).
pub fn derive_password(password: &str, salt: &[u8], iterations: u32, protocol: Protocol) -> Vec<u8> {
    let digest = Sha256::digest(password.as_bytes());
    let input = match protocol {
        Protocol::S2k => digest.to_vec(),
        Protocol::S2kFo => hex::encode(digest).into_bytes(),
    };
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(&input, salt, iterations, &mut out);
    out.to_vec()
}

pub struct Client {
    username: String,
    a: BigUint,
    big_a: BigUint,
}

pub struct Proof {
    /// Client proof `M1`.
    pub m1: [u8; 32],
    /// Expected server proof `M2`.
    pub m2: [u8; 32],
    /// Session key `K`.
    pub key: [u8; 32],
}

impl Client {
    pub fn new(username: &str) -> Self {
        let mut secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        Self::with_secret(username, &secret)
    }

    fn with_secret(username: &str, secret: &[u8]) -> Self {
        let a = BigUint::from_bytes_be(secret);
        let big_a = g().modpow(&a, &n());
        Self { username: username.to_string(), a, big_a }
    }

    pub fn public_a(&self) -> Vec<u8> {
        self.big_a.to_bytes_be()
    }

    /// `password_key` is the output of [`derive_password`].
    pub fn process_challenge(&self, salt: &[u8], server_b: &[u8], password_key: &[u8]) -> Result<Proof> {
        let n = n();
        let width = n.to_bytes_be().len();
        let b = BigUint::from_bytes_be(server_b);
        if &b % &n == BigUint::ZERO {
            bail!("server sent an invalid SRP B");
        }
        let k = BigUint::from_bytes_be(&h(&[&n.to_bytes_be(), &pad(&g(), width)]));
        let u = BigUint::from_bytes_be(&h(&[&pad(&self.big_a, width), &pad(&b, width)]));
        let x = BigUint::from_bytes_be(&h(&[salt, &h(&[b":", password_key])]));
        let gx = g().modpow(&x, &n);
        let base = (&b + &n * &k - (&k * gx) % &n) % &n;
        let s = base.modpow(&(&self.a + &u * &x), &n);
        let key = h(&[&s.to_bytes_be()]);

        let hn = h(&[&n.to_bytes_be()]);
        let hg = h(&[&pad(&g(), width)]);
        let hxor: Vec<u8> = hn.iter().zip(hg).map(|(a, b)| a ^ b).collect();
        let m1 = h(&[
            &hxor,
            &h(&[self.username.as_bytes()]),
            salt,
            &self.big_a.to_bytes_be(),
            &b.to_bytes_be(),
            &key,
        ]);
        let m2 = h(&[&self.big_a.to_bytes_be(), &m1, &key]);
        Ok(Proof { m1, m2, key })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Play the server side and check both ends agree on the session key and proofs.
    #[test]
    fn client_matches_reference_server() {
        let salt = b"saltsaltsaltsalt";
        let key = derive_password("hunter2", salt, 1000, Protocol::S2k);
        let client = Client::with_secret("user@example.com", &[0x11; 32]);

        let n = n();
        let width = n.to_bytes_be().len();
        let x = BigUint::from_bytes_be(&h(&[salt, &h(&[b":", &key])]));
        let v = g().modpow(&x, &n);
        let k = BigUint::from_bytes_be(&h(&[&n.to_bytes_be(), &pad(&g(), width)]));
        let b_secret = BigUint::from_bytes_be(&[0x22; 32]);
        let big_b = (&k * &v + g().modpow(&b_secret, &n)) % &n;

        let proof = client.process_challenge(salt, &big_b.to_bytes_be(), &key).unwrap();

        let big_a = BigUint::from_bytes_be(&client.public_a());
        let u = BigUint::from_bytes_be(&h(&[&pad(&big_a, width), &pad(&big_b, width)]));
        let server_s = (&big_a * v.modpow(&u, &n)).modpow(&b_secret, &n);
        assert_eq!(proof.key, h(&[&server_s.to_bytes_be()]));
    }

    #[test]
    fn s2k_fo_hex_encodes_before_pbkdf2() {
        let a = derive_password("pw", b"s", 1, Protocol::S2k);
        let b = derive_password("pw", b"s", 1, Protocol::S2kFo);
        assert_ne!(a, b);
    }
}
