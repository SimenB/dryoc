//! # Public/secret keypair tools
//!
//! Provides an implementation for handling public/private keypairs based on
//! libsodium's crypto_box, which uses X25519.
//!
//! Refer to the [protected] mod for details on usage with protected memory.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::classic::crypto_box::crypto_box_seed_keypair_inplace;
use crate::constants::{
    CRYPTO_BOX_BEFORENMBYTES, CRYPTO_BOX_PUBLICKEYBYTES, CRYPTO_BOX_SECRETKEYBYTES,
    CRYPTO_KX_SESSIONKEYBYTES,
};
use crate::error::Error;
use crate::kx;
use crate::precalc::PrecalcSecretKey;
use crate::types::*;

/// Stack-allocated public key type alias.
pub type PublicKey = StackByteArray<CRYPTO_BOX_PUBLICKEYBYTES>;
/// Stack-allocated secret key type alias.
pub type SecretKey = StackByteArray<CRYPTO_BOX_SECRETKEYBYTES>;
/// Stack-allocated key pair type alias.
pub type StackKeyPair = KeyPair<PublicKey, SecretKey>;

#[cfg_attr(
    feature = "serde",
    derive(Zeroize, ZeroizeOnDrop, Serialize, Deserialize, Debug, Clone)
)]
#[cfg_attr(not(feature = "serde"), derive(Zeroize, ZeroizeOnDrop, Debug, Clone))]
/// Public/private keypair for use with [`crate::dryocbox::DryocBox`], aka
/// libsodium box
pub struct KeyPair<
    PublicKey: ByteArray<CRYPTO_BOX_PUBLICKEYBYTES> + Zeroize,
    SecretKey: ByteArray<CRYPTO_BOX_SECRETKEYBYTES> + Zeroize,
> {
    /// Public key
    pub public_key: PublicKey,
    /// Secret key
    pub secret_key: SecretKey,
}

impl<
    PublicKey: NewByteArray<CRYPTO_BOX_PUBLICKEYBYTES> + Zeroize,
    SecretKey: NewByteArray<CRYPTO_BOX_SECRETKEYBYTES> + Zeroize,
> KeyPair<PublicKey, SecretKey>
{
    /// Creates a new, empty keypair.
    pub fn new() -> Self {
        Self {
            public_key: PublicKey::new_byte_array(),
            secret_key: SecretKey::new_byte_array(),
        }
    }

    /// Generates a random keypair.
    pub fn gen() -> Self {
        use crate::classic::crypto_box::crypto_box_keypair_inplace;

        let mut public_key = PublicKey::new_byte_array();
        let mut secret_key = SecretKey::new_byte_array();
        crypto_box_keypair_inplace(public_key.as_mut_array(), secret_key.as_mut_array());

        Self {
            public_key,
            secret_key,
        }
    }

    /// Derives a keypair from `secret_key`, and consumes it, and returns a new
    /// keypair.
    pub fn from_secret_key(secret_key: SecretKey) -> Self {
        use crate::classic::crypto_core::crypto_scalarmult_base;

        let mut public_key = PublicKey::new_byte_array();
        crypto_scalarmult_base(public_key.as_mut_array(), secret_key.as_array());

        Self {
            public_key,
            secret_key,
        }
    }

    /// Derives a keypair from `seed`, returning
    /// a new keypair.
    pub fn from_seed<Seed: Bytes>(seed: &Seed) -> Self {
        let mut public_key = PublicKey::new_byte_array();
        let mut secret_key = SecretKey::new_byte_array();

        crypto_box_seed_keypair_inplace(
            public_key.as_mut_array(),
            secret_key.as_mut_array(),
            seed.as_slice(),
        );

        Self {
            public_key,
            secret_key,
        }
    }
}

impl KeyPair<StackByteArray<CRYPTO_BOX_PUBLICKEYBYTES>, StackByteArray<CRYPTO_BOX_SECRETKEYBYTES>> {
    /// Randomly generates a new keypair, using default types
    /// (stack-allocated byte arrays). Provided for convenience.
    pub fn gen_with_defaults() -> Self {
        Self::gen()
    }
}

impl<
    'a,
    PublicKey: ByteArray<CRYPTO_BOX_PUBLICKEYBYTES> + std::convert::TryFrom<&'a [u8]> + Zeroize,
    SecretKey: ByteArray<CRYPTO_BOX_SECRETKEYBYTES> + std::convert::TryFrom<&'a [u8]> + Zeroize,
> KeyPair<PublicKey, SecretKey>
{
    /// Constructs a new keypair from key slices, consuming them. Does not check
    /// validity or authenticity of keypair.
    pub fn from_slices(public_key: &'a [u8], secret_key: &'a [u8]) -> Result<Self, Error> {
        Ok(Self {
            public_key: PublicKey::try_from(public_key)
                .map_err(|_e| dryoc_error!("invalid public key"))?,
            secret_key: SecretKey::try_from(secret_key)
                .map_err(|_e| dryoc_error!("invalid secret key"))?,
        })
    }
}

impl<
    PublicKey: ByteArray<CRYPTO_BOX_PUBLICKEYBYTES> + Zeroize,
    SecretKey: ByteArray<CRYPTO_BOX_SECRETKEYBYTES> + Zeroize,
> KeyPair<PublicKey, SecretKey>
{
    /// Checks if the given public key is valid according to X25519 rules.
    ///
    /// For X25519 ([`crypto_box`](`crate::classic::crypto_box`),
    /// [`DryocBox`](`crate::dryocbox::DryocBox`)), a public key is considered
    /// valid if:
    /// - It is not the all-zero point `[0, ..., 0]`.
    /// - The high bit of the last byte is 0.
    ///
    /// This function verifies these conditions.
    ///
    /// **Note:** This validation is specific to X25519 keys used in
    /// Diffie-Hellman key exchange (`crypto_box`). It primarily aims to
    /// exclude degenerate keys and does **not** explicitly verify that the
    /// point lies on the underlying curve, unlike stricter Ed25519 point
    /// validation (see
    /// [`crypto_core_ed25519_is_valid_point`](`crate::classic::crypto_core::crypto_core_ed25519_is_valid_point`)).
    ///
    /// ## Validating Protected Keys
    ///
    /// You can validate keys stored in protected memory directly, as the
    /// validation functions operate on references.
    ///
    /// ```
    /// # #![cfg_attr(not(feature = "nightly"), ignore)]
    /// # #[cfg(feature = "nightly")]
    /// # {
    /// use dryoc::constants::{CRYPTO_BOX_PUBLICKEYBYTES, CRYPTO_BOX_SECRETKEYBYTES};
    /// use dryoc::keypair::protected::{HeapByteArray, LockedRO};
    /// use dryoc::keypair::{KeyPair, PublicKey, SecretKey};
    ///
    /// // Generate a keypair stored in locked, read-only memory
    /// let protected_kp: KeyPair<
    ///     LockedRO<HeapByteArray<CRYPTO_BOX_PUBLICKEYBYTES>>,
    ///     LockedRO<HeapByteArray<CRYPTO_BOX_SECRETKEYBYTES>>,
    /// > = KeyPair::gen_readonly_locked_keypair().expect("Failed to generate locked keypair");
    ///
    /// // Validate the Ed25519 public key using the relaxed rules appropriate for
    /// // keys generated by crypto_sign_keypair (even though this is an X25519 keypair,
    /// // the validation function itself can be called).
    /// // Note: For an actual Ed25519 keypair from crypto_sign, you'd use the
    /// // crypto_core_ed25519_is_valid_point_relaxed function directly.
    /// // Here we demonstrate calling the KeyPair method.
    /// let is_valid = KeyPair::<
    ///     LockedRO<HeapByteArray<CRYPTO_BOX_PUBLICKEYBYTES>>,
    ///     LockedRO<HeapByteArray<CRYPTO_BOX_SECRETKEYBYTES>>,
    /// >::is_valid_ed25519_key(&protected_kp.public_key);
    ///
    /// // For keys generated by crypto_sign_keypair, relaxed validation should pass.
    /// // (This assertion might depend on the specific key generation details,
    /// // but illustrates the call)
    /// // assert!(is_valid, "Protected key should be valid (relaxed check)");
    ///
    /// // Similarly, validate the X25519 public key
    /// let is_x25519_valid = KeyPair::<
    ///     LockedRO<HeapByteArray<CRYPTO_BOX_PUBLICKEYBYTES>>,
    ///     LockedRO<HeapByteArray<CRYPTO_BOX_SECRETKEYBYTES>>,
    /// >::is_valid_public_key(&protected_kp.public_key);
    ///
    /// assert!(is_x25519_valid, "Protected X25519 key should be valid");
    /// # }
    /// ```
    pub fn is_valid_public_key<PK: ByteArray<CRYPTO_BOX_PUBLICKEYBYTES>>(key: &PK) -> bool {
        const ZERO_POINT: [u8; CRYPTO_BOX_PUBLICKEYBYTES] = [0u8; CRYPTO_BOX_PUBLICKEYBYTES];
        let key_array = key.as_array();

        // Check 1: Not the all-zero point
        if key_array == &ZERO_POINT {
            return false;
        }

        // Check 2: High bit of the last byte must be 0
        // Although clamping during generation usually ensures this, we check it.
        if key_array[CRYPTO_BOX_PUBLICKEYBYTES - 1] & 0x80 != 0 {
            return false;
        }

        // If both checks pass, it's considered a valid X25519 public key
        // representation.
        true
    }

    /// Checks if the given key is a valid Ed25519 public key, using relaxed
    /// validation rules that allow the high bit to be set.
    ///
    /// For Ed25519 public keys, generated by `crypto_sign_keypair()`, we need
    /// to use more permissive validation since these keys can have the high
    /// bit set.
    ///
    /// This method should be used for validating Ed25519 keys (used in
    /// signatures), while `is_valid_public_key` should be used for X25519
    /// keys (used in crypto_box).
    pub fn is_valid_ed25519_key<PK: ByteArray<CRYPTO_BOX_PUBLICKEYBYTES>>(key: &PK) -> bool {
        crate::classic::crypto_core::crypto_core_ed25519_is_valid_point_relaxed(key.as_array())
    }

    /// Creates new client session keys using this keypair and
    /// `server_public_key`, assuming this keypair is for the client.
    pub fn kx_new_client_session<SessionKey: NewByteArray<CRYPTO_KX_SESSIONKEYBYTES> + Zeroize>(
        &self,
        server_public_key: &PublicKey,
    ) -> Result<kx::Session<SessionKey>, Error> {
        kx::Session::new_client(self, server_public_key)
    }

    /// Creates new server session keys using this keypair and
    /// `client_public_key`, assuming this keypair is for the server.
    pub fn kx_new_server_session<SessionKey: NewByteArray<CRYPTO_KX_SESSIONKEYBYTES> + Zeroize>(
        &self,
        client_public_key: &PublicKey,
    ) -> Result<kx::Session<SessionKey>, Error> {
        kx::Session::new_server(self, client_public_key)
    }

    /// Computes a stack-allocated shared secret key using a secret key from
    /// this keypair and `third_party_public_key`.
    ///
    /// Compatible with libsodium's `crypto_box_beforenm`.
    #[inline]
    pub fn precalculate(
        &self,
        third_party_public_key: &PublicKey,
    ) -> PrecalcSecretKey<StackByteArray<CRYPTO_BOX_BEFORENMBYTES>> {
        PrecalcSecretKey::precalculate(third_party_public_key, &self.secret_key)
    }
}

impl<
    PublicKey: NewByteArray<CRYPTO_BOX_PUBLICKEYBYTES> + Zeroize,
    SecretKey: NewByteArray<CRYPTO_BOX_SECRETKEYBYTES> + Zeroize,
> Default for KeyPair<PublicKey, SecretKey>
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(feature = "nightly", all(doc, not(doctest))))]
#[cfg_attr(all(feature = "nightly", doc), doc(cfg(feature = "nightly")))]
pub mod protected {
    //! #  Protected memory for [`KeyPair`]
    use super::*;
    use crate::classic::crypto_box::crypto_box_keypair_inplace;
    pub use crate::protected::*;

    impl
        KeyPair<
            Locked<HeapByteArray<CRYPTO_BOX_PUBLICKEYBYTES>>,
            Locked<HeapByteArray<CRYPTO_BOX_SECRETKEYBYTES>>,
        >
    {
        /// Returns a new locked keypair.
        pub fn new_locked_keypair() -> Result<Self, std::io::Error> {
            Ok(Self {
                public_key: HeapByteArray::<CRYPTO_BOX_PUBLICKEYBYTES>::new_locked()?,
                secret_key: HeapByteArray::<CRYPTO_BOX_SECRETKEYBYTES>::new_locked()?,
            })
        }

        /// Returns a new randomly generated locked keypair.
        pub fn gen_locked_keypair() -> Result<Self, std::io::Error> {
            let mut res = Self::new_locked_keypair()?;

            crypto_box_keypair_inplace(
                res.public_key.as_mut_array(),
                res.secret_key.as_mut_array(),
            );

            Ok(res)
        }

        /// Computes a heap-allocated, page-aligned, locked shared secret key
        /// using a secret key from this keypair and
        /// `third_party_public_key`.
        ///
        /// Compatible with libsodium's `crypto_box_beforenm`.
        #[inline]
        pub fn precalculate_locked<OtherPublicKey: ByteArray<CRYPTO_BOX_PUBLICKEYBYTES>>(
            &self,
            third_party_public_key: &OtherPublicKey,
        ) -> Result<PrecalcSecretKey<Locked<HeapByteArray<CRYPTO_BOX_BEFORENMBYTES>>>, std::io::Error>
        {
            PrecalcSecretKey::precalculate_locked(third_party_public_key, &self.secret_key)
        }
    }

    impl
        KeyPair<
            LockedRO<HeapByteArray<CRYPTO_BOX_PUBLICKEYBYTES>>,
            LockedRO<HeapByteArray<CRYPTO_BOX_SECRETKEYBYTES>>,
        >
    {
        /// Returns a new randomly generated locked, read-only keypair.
        pub fn gen_readonly_locked_keypair() -> Result<Self, std::io::Error> {
            let mut public_key = HeapByteArray::<CRYPTO_BOX_PUBLICKEYBYTES>::new_locked()?;
            let mut secret_key = HeapByteArray::<CRYPTO_BOX_SECRETKEYBYTES>::new_locked()?;

            crypto_box_keypair_inplace(public_key.as_mut_array(), secret_key.as_mut_array());

            let public_key = public_key.mprotect_readonly()?;
            let secret_key = secret_key.mprotect_readonly()?;

            Ok(Self {
                public_key,
                secret_key,
            })
        }

        /// Computes a heap-allocated, page-aligned, locked, read-only shared
        /// secret key using a secret key from this keypair and
        /// `third_party_public_key`.
        ///
        /// Compatible with libsodium's `crypto_box_beforenm`.
        #[inline]
        pub fn precalculate_readonly_locked<
            OtherPublicKey: ByteArray<CRYPTO_BOX_PUBLICKEYBYTES>,
        >(
            &self,
            third_party_public_key: &OtherPublicKey,
        ) -> Result<
            PrecalcSecretKey<LockedRO<HeapByteArray<CRYPTO_BOX_BEFORENMBYTES>>>,
            std::io::Error,
        > {
            PrecalcSecretKey::precalculate_readonly_locked(third_party_public_key, &self.secret_key)
        }
    }
}

impl<
    PublicKey: ByteArray<CRYPTO_BOX_PUBLICKEYBYTES> + Zeroize,
    SecretKey: ByteArray<CRYPTO_BOX_SECRETKEYBYTES> + Zeroize,
> PartialEq<KeyPair<PublicKey, SecretKey>> for KeyPair<PublicKey, SecretKey>
{
    fn eq(&self, other: &Self) -> bool {
        self.public_key
            .as_slice()
            .ct_eq(other.public_key.as_slice())
            .unwrap_u8()
            == 1
            && self
                .secret_key
                .as_slice()
                .ct_eq(other.secret_key.as_slice())
                .unwrap_u8()
                == 1
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::kx::Session;

    fn all_eq<T>(t: &[T], v: T) -> bool
    where
        T: PartialEq,
    {
        t.iter().all(|x| *x == v)
    }

    #[test]
    fn test_new() {
        let keypair = KeyPair::<
            StackByteArray<CRYPTO_BOX_PUBLICKEYBYTES>,
            StackByteArray<CRYPTO_BOX_SECRETKEYBYTES>,
        >::new();

        assert!(all_eq(&keypair.public_key, 0));
        assert!(all_eq(&keypair.secret_key, 0));
    }

    #[test]
    fn test_default() {
        let keypair = KeyPair::<
            StackByteArray<CRYPTO_BOX_PUBLICKEYBYTES>,
            StackByteArray<CRYPTO_BOX_SECRETKEYBYTES>,
        >::default();

        assert!(all_eq(&keypair.public_key, 0));
        assert!(all_eq(&keypair.secret_key, 0));
    }

    #[test]
    fn test_gen_keypair() {
        use sodiumoxide::crypto::scalarmult::curve25519::{Scalar, scalarmult_base};

        use crate::classic::crypto_core::crypto_scalarmult_base;

        let keypair = KeyPair::<
            StackByteArray<CRYPTO_BOX_PUBLICKEYBYTES>,
            StackByteArray<CRYPTO_BOX_SECRETKEYBYTES>,
        >::gen();

        let mut public_key = [0u8; CRYPTO_BOX_PUBLICKEYBYTES];
        crypto_scalarmult_base(&mut public_key, keypair.secret_key.as_array());

        assert_eq!(keypair.public_key.as_array(), &public_key);

        let ge = scalarmult_base(&Scalar::from_slice(&keypair.secret_key).unwrap());

        assert_eq!(ge.as_ref(), public_key);
    }

    #[test]
    fn test_from_secret_key() {
        let keypair_1 = KeyPair::<
            StackByteArray<CRYPTO_BOX_PUBLICKEYBYTES>,
            StackByteArray<CRYPTO_BOX_SECRETKEYBYTES>,
        >::gen();
        let keypair_2 = KeyPair::from_secret_key(keypair_1.secret_key.clone());

        assert_eq!(keypair_1.public_key, keypair_2.public_key);
    }

    #[test]
    fn test_keypair_precalculate() {
        let kp1 = KeyPair::gen_with_defaults();
        let kp2 = KeyPair::gen_with_defaults();
        let precalc = kp1.precalculate(&kp2.public_key);
        assert_eq!(precalc.len(), crate::constants::CRYPTO_BOX_BEFORENMBYTES);
    }

    #[cfg(feature = "nightly")]
    #[test]
    fn test_keypair_precalculate_locked() {
        use crate::keypair::protected::*;
        let kp1 = KeyPair::gen_locked_keypair().unwrap();
        let kp2 = KeyPair::gen_locked_keypair().unwrap();
        let precalc = kp1.precalculate_locked(&kp2.public_key).unwrap();
        assert_eq!(precalc.len(), crate::constants::CRYPTO_BOX_BEFORENMBYTES);
    }

    #[test]
    fn test_keypair_kx_new_client_session() {
        let server_kp = KeyPair::gen_with_defaults();
        let client_kp = KeyPair::gen_with_defaults();
        let session: Session<StackByteArray<CRYPTO_KX_SESSIONKEYBYTES>> = client_kp
            .kx_new_client_session(&server_kp.public_key)
            .unwrap();
        assert_eq!(
            session.rx_as_slice().len(),
            crate::constants::CRYPTO_KX_SESSIONKEYBYTES
        );
        assert_eq!(
            session.tx_as_slice().len(),
            crate::constants::CRYPTO_KX_SESSIONKEYBYTES
        );
    }

    #[test]
    fn test_keypair_kx_new_server_session() {
        let client_kp = KeyPair::gen_with_defaults();
        let server_kp = KeyPair::gen_with_defaults();
        let session: Session<StackByteArray<CRYPTO_KX_SESSIONKEYBYTES>> = server_kp
            .kx_new_server_session(&client_kp.public_key)
            .unwrap();
        assert_eq!(
            session.rx_as_slice().len(),
            crate::constants::CRYPTO_KX_SESSIONKEYBYTES
        );
        assert_eq!(
            session.tx_as_slice().len(),
            crate::constants::CRYPTO_KX_SESSIONKEYBYTES
        );
    }

    #[test]
    fn test_keypair_from_seed() {
        let seed = [42u8; 32];
        let kp: StackKeyPair = KeyPair::from_seed(&seed);
        assert!(!kp.public_key.iter().all(|x| *x == 0));
    }

    #[test]
    fn test_keypair_gen_with_defaults() {
        let kp = KeyPair::gen_with_defaults();
        assert!(!kp.public_key.iter().all(|x| *x == 0));
    }

    #[test]
    fn test_is_valid_public_key() {
        // Known valid key (assuming it meets X25519 criteria)
        // This specific key is also a valid Ed25519 key.
        let valid_pk_bytes = [
            215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114,
            243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
        ];
        let valid_pk = PublicKey::from(valid_pk_bytes);
        assert!(
            KeyPair::<PublicKey, SecretKey>::is_valid_public_key(&valid_pk),
            "Known valid key failed validation"
        );

        // Invalid: High bit set
        let mut invalid_high_bit_bytes = [0u8; CRYPTO_BOX_PUBLICKEYBYTES];
        invalid_high_bit_bytes[31] = 0x80;
        let invalid_high_bit = PublicKey::from(invalid_high_bit_bytes);
        assert!(
            !KeyPair::<PublicKey, SecretKey>::is_valid_public_key(&invalid_high_bit),
            "Key with high bit set should be invalid"
        );

        // Invalid: Zero point
        let zero_bytes = [0u8; CRYPTO_BOX_PUBLICKEYBYTES];
        let zero_pk = PublicKey::from(zero_bytes);
        assert!(
            !KeyPair::<PublicKey, SecretKey>::is_valid_public_key(&zero_pk),
            "Zero key should be invalid"
        );

        // The identity point [1, 0, ..., 0] is NOT necessarily invalid for X25519,
        // unlike Ed25519 validation. We don't explicitly test its rejection here.

        // Generated key should be valid
        let kp = KeyPair::gen_with_defaults();
        assert!(
            KeyPair::<PublicKey, SecretKey>::is_valid_public_key(&kp.public_key),
            "Generated key failed validation"
        );
    }

    #[test]
    fn test_is_valid_ed25519_key() {
        // Get a key from crypto_sign_keypair which may have high bit set
        let (valid_pk, _) = crate::classic::crypto_sign::crypto_sign_keypair();

        // Should pass relaxed validation
        assert!(
            KeyPair::<PublicKey, SecretKey>::is_valid_ed25519_key(&valid_pk),
            "Ed25519 key from crypto_sign_keypair should pass relaxed validation"
        );

        // Keys that are invalid with any validation

        // Invalid: all zeros
        let zero_bytes = [0u8; CRYPTO_BOX_PUBLICKEYBYTES];
        let zero_pk = PublicKey::from(zero_bytes);
        assert!(
            !KeyPair::<PublicKey, SecretKey>::is_valid_ed25519_key(&zero_pk),
            "Zero key should be invalid even with relaxed validation"
        );

        // Test the identity element (small-order point)
        let identity_bytes = [
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ];
        let identity_pk = PublicKey::from(identity_bytes);
        assert!(
            !KeyPair::<PublicKey, SecretKey>::is_valid_ed25519_key(&identity_pk),
            "Identity element (small order point) should be invalid even with relaxed validation"
        );
    }
}
