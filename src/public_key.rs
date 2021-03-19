use core::convert::TryFrom;

use num_bigint::{BigInt, Sign};
use sha3::{
    digest::{ExtendableOutput, Update},
    Shake256,
};

use crate::{
    array_to_key,
    point::Point,
    private_key::PrivateKey,
    {shake256, Ed448Error, PreHash, KEY_LENGTH, SIG_LENGTH},
};

/// This is a public key. _Should be distributed._
pub struct PublicKey(PublicKeyRaw);

pub type PublicKeyRaw = [u8; KEY_LENGTH];

opaque_debug::implement!(PublicKey);

impl PublicKey {
    pub fn as_byte(&self) -> &PublicKeyRaw {
        &self.0
    }

    /// Verify signature with public key.
    pub fn verify(&self, msg: &[u8], sign: &[u8], ctx: Option<&[u8]>) -> crate::Result<()> {
        self.verify_real(msg, sign, ctx, PreHash::False)
    }

    /// Verify signature with public key. Message is pre-hashed before checked.
    pub fn verify_ph(&self, msg: &[u8], sign: &[u8], ctx: Option<&[u8]>) -> crate::Result<()> {
        self.verify_real(msg, sign, ctx, PreHash::True)
    }

    fn verify_real(
        &self,
        msg: &[u8],
        sign: &[u8],
        ctx: Option<&[u8]>,
        pre_hash: PreHash,
    ) -> crate::Result<()> {
        // Sanity-check sizes.
        if sign.len() < SIG_LENGTH {
            return Err(Ed448Error::WrongSignatureLength);
        }

        let ctx = ctx.unwrap_or(b"");
        let msg = match pre_hash {
            PreHash::False => msg.to_vec(),
            PreHash::True => Shake256::default().chain(msg).finalize_boxed(64).to_vec(),
        };

        // Split signature into R and S, and parse.
        let (Rraw, Sraw) = sign.split_at(KEY_LENGTH);
        let (R, S) = (
            Point::default()
                .decode(Rraw)
                .map_err(|_| Ed448Error::InvalidSignature)?,
            BigInt::from_bytes_le(Sign::Plus, Sraw),
        );
        // Parse public key.
        let A = Point::default()
            .decode(self.as_byte())
            .map_err(|_| Ed448Error::InvalidSignature)?;
        if &S >= Point::l() {
            return Err(Ed448Error::InvalidSignature);
        }
        // Calculate h.
        let h = shake256(vec![Rraw, self.as_byte(), &msg], ctx, pre_hash);
        let h = BigInt::from_bytes_le(Sign::Plus, &h) % Point::l();
        // Calculate left and right sides of check eq.
        let mut rhs = R + (A * h);
        let mut lhs = Point::default() * S;
        for _ in 0..2 {
            lhs = lhs.double();
            rhs = rhs.double();
        }
        // Check eq. holds?
        if lhs == rhs {
            Ok(())
        } else {
            Err(Ed448Error::InvalidSignature)
        }
    }
}

impl From<&PrivateKey> for PublicKey {
    fn from(private_key: &PrivateKey) -> Self {
        let (s, _) = &private_key.expand();
        // 3.  Interpret the buffer as the little-endian integer, forming a
        //     secret scalar s.
        PublicKey::from(BigInt::from_bytes_le(Sign::Plus, s))
    }
}

impl From<BigInt> for PublicKey {
    fn from(s: BigInt) -> Self {
        //     Perform a known-base-point scalar multiplication [s]B.
        let A = Point::default() * s;

        // 4.  The public key A is the encoding of the point [s]B.
        PublicKey(A.encode())
    }
}

impl From<[u8; KEY_LENGTH]> for PublicKey {
    fn from(array: [u8; KEY_LENGTH]) -> Self {
        Self(array)
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = Ed448Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != KEY_LENGTH {
            return Err(Ed448Error::WrongPublicKeyLength);
        }
        Ok(PublicKey::from(array_to_key(value)))
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use super::*;

    #[test]
    fn test_vectors_rfc8032_public() {
        let secret_vec = hex::decode(
            "6c82a562cb808d10d632be89c8513ebf\
                6c929f34ddfa8c9f63c9960ef6e348a3\
                528c8a3fcc2f044e39a3fc5b94492f8f\
                032e7549a20098f95b",
        )
        .unwrap();
        let ref_public = hex::decode(
            "5fd7449b59b461fd2ce787ec616ad46a\
                1da1342485a70e1f8a0ea75d80e96778\
                edf124769b46c7061bd6783df1e50f6c\
                d1fa1abeafe8256180",
        )
        .unwrap();

        let secret = PrivateKey::try_from(&secret_vec[..]).unwrap();
        let public = PublicKey::from(&secret);

        assert_eq!(&public.as_byte()[..], &ref_public[..]);
    }
}
