use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use josekit::{jwk::Jwk, Value};
use openssl::{
    bn::{BigNum, BigNumContext},
    pkey::{PKey, Public},
};

const SIGNATURE_LEN: usize = 64; // 32 bytes for X, 32 bytes for Y
const CURVE_STR: &str = "P-256";

/// Extension trait for `josekit::jwk::Jwk` to construct EC P-256 public keys.
pub trait JwkExt {
    /// Sets EC P-256 X/Y coordinates on this JWK and marks algorithm/curve.
    fn set_ec_p256_xy(&mut self, x: &[u8], y: &[u8]) -> anyhow::Result<()>;

    /// Builds a new EC P-256 public JWK from an OpenSSL public key and sets `kid`.
    fn new_ec_p256_from_openssl(
        public_key: &PKey<Public>,
        kid: impl Into<String>,
    ) -> anyhow::Result<Jwk>;
}

impl JwkExt for Jwk {
    fn set_ec_p256_xy(&mut self, x: &[u8], y: &[u8]) -> anyhow::Result<()> {
        self.set_algorithm("ES256");
        self.set_key_use("sig");
        self.set_parameter("crv", Some(Value::from(CURVE_STR)))?;
        self.set_parameter("x", Some(Value::String(URL_SAFE_NO_PAD.encode(x))))?;
        self.set_parameter("y", Some(Value::String(URL_SAFE_NO_PAD.encode(y))))?;
        Ok(())
    }

    fn new_ec_p256_from_openssl(
        public_key: &PKey<Public>,
        kid: impl Into<String>,
    ) -> anyhow::Result<Jwk> {
        let mut jwk = Self::new("EC");

        let ec_key = public_key.ec_key()?;
        let mut x = BigNum::new()?;
        let mut y = BigNum::new()?;
        let mut ctx = BigNumContext::new()?;
        ec_key
            .public_key()
            .affine_coordinates(ec_key.group(), &mut x, &mut y, &mut ctx)?;

        let (mut x, mut y) = (x.to_vec(), y.to_vec());
        pad_left(&mut x, SIGNATURE_LEN / 2);
        pad_left(&mut y, SIGNATURE_LEN / 2);

        jwk.set_ec_p256_xy(&x, &y)?;
        jwk.set_key_id(kid.into());
        Ok(jwk)
    }
}

/// Adds padding to the left of a vector to make it the specified length.
/// Forked from <https://github.com/blckngm/jwtk/blob/9cd5cc1e345ecccc3c9f5d2618d03afbde34e54f/src/ecdsa.rs#L223>
fn pad_left(v: &mut Vec<u8>, len: usize) {
    debug_assert!(v.len() <= len);
    if v.len() == len {
        return;
    }
    let old_len = v.len();
    v.resize(len, 0);
    v.copy_within(0..old_len, len - old_len);
    v[..(len - old_len)].fill(0);
}
