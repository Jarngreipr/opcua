//! Contains semi-opaque wrappers for various OpenSSL types. The Rust bindings for OpenSSL do
//! not mark types as implementing debug, thread safety etc. so these wrappers do that so the keys/certs
//! can be contained by structs that have those things.
//!
//! The module also contains convenience methods

use std;
use std::marker::{Send};
use std::fmt::{Debug, Result, Formatter};

use openssl::x509;
use openssl::aes;
use openssl::pkey;
use openssl::rsa;
use openssl::sign;
use openssl::hash;

use chrono::{DateTime, UTC, TimeZone};

use types::{ByteString, StatusCode};
use types::StatusCode::*;

#[derive(Debug)]
/// Used to create an X509 cert (and private key)
pub struct X509Data {
    pub key_size: u32,
    pub common_name: String,
    pub organization: String,
    pub organizational_unit: String,
    pub country: String,
    pub state: String,
    pub alt_host_names: Vec<String>,
    pub certificate_duration_days: u32,
}

/// This is a wrapper around the OpenSSL X509 cert
pub struct X509 {
    pub value: x509::X509,
}

impl Debug for X509 {
    fn fmt(&self, f: &mut Formatter) -> Result {
        // This impl will not write out the key, but it exists to keep structs happy
        // that contain a key as a field
        write!(f, "[x509]")
    }
}

/// This allows certs to be transferred between threads
unsafe impl Send for X509 {}

impl X509 {
    pub fn wrap(value: x509::X509) -> X509 {
        X509 { value }
    }

    pub fn from_byte_string(data: &ByteString) -> std::result::Result<X509, ()> {
        if data.is_null() {
            Err(())
        } else {
            if let Ok(cert) = x509::X509::from_der(&data.value.as_ref().unwrap()) {
                Ok(X509::wrap(cert))
            } else {
                Err(())
            }
        }
    }

    /// Returns a ByteString representation of the cert which is DER encoded form of X509v3
    pub fn as_byte_string(&self) -> ByteString {
        let der = self.value.to_der().unwrap();
        ByteString::from_bytes(&der)
    }

    pub fn public_key(&self) -> std::result::Result<PKey, ()> {
        if let Ok(pkey) = self.value.public_key() {
            let pkey = PKey::wrap(pkey);
            Ok(pkey)
        } else {
            Err(())
        }
    }

    pub fn is_time_valid(&self, now: &DateTime<UTC>) -> StatusCode {
        // Issuer time
        let not_before = self.not_before();
        if let Ok(not_before) = not_before {
            if now.lt(&not_before) {
                return BAD_CERTIFICATE_TIME_INVALID;
            }
        } else {
            // No before time
            return BAD_CERTIFICATE_INVALID;
        }

        // Expiration time
        let not_after = self.not_after();
        if let Ok(not_after) = not_after {
            if now.gt(&not_after) {
                return BAD_CERTIFICATE_TIME_INVALID;
            }
        } else {
            // No after time
            return BAD_CERTIFICATE_INVALID;
        }

        GOOD
    }

    /// OPC UA Part 6 MessageChunk structure
    ///
    /// The thumbprint is the SHA1 digest of the DER form of the certificate. The hash is 160 bits
    /// (20 bytes) in length and is sent in some secure conversation headers.
    ///
    /// The thumbprint might be used by the server / client for look-up purposes.
    pub fn thumbprint(&self) -> Vec<u8> {
        use openssl::hash::{MessageDigest, hash};
        let der = self.value.to_der().unwrap();
        hash(MessageDigest::sha1(), &der).unwrap()
    }

    /// Turn the Asn1 values into useful portable types
    pub fn not_before(&self) -> std::result::Result<DateTime<UTC>, ()> {
        let date = self.value.not_before().to_string();
        Self::parse_asn1_date(&date)
    }

    /// Turn the Asn1 values into useful portable types
    pub fn not_after(&self) -> std::result::Result<DateTime<UTC>, ()> {
        let date = self.value.not_after().to_string();
        Self::parse_asn1_date(&date)
    }

    fn parse_asn1_date(date: &str) -> std::result::Result<DateTime<UTC>, ()> {
        // Parse ASN1 time format
        // MMM DD HH:MM:SS YYYY [GMT]
        let date = if date.ends_with(" GMT") {
            // Not interested in GMT part, ASN1 is always GMT (i.e. UTC)
            &date[..date.len() - 4]
        } else {
            &date
        };
        let result = UTC.datetime_from_str(date, "%b %d %H:%M:%S %Y");
        if result.is_err() {
            println!("Error = {:?}", result.unwrap_err());
            Err(())
        } else {
            Ok(result.unwrap())
        }
    }
}

#[test]
fn parse_asn1_date_test() {
    use chrono::{Datelike, Timelike};

    assert!(X509::parse_asn1_date("").is_err());
    assert!(X509::parse_asn1_date("Jan 69 00:00:00 1970").is_err());
    assert!(X509::parse_asn1_date("Feb 21 00:00:00 1970").is_ok());
    assert!(X509::parse_asn1_date("Feb 21 00:00:00 1970 GMT").is_ok());

    let dt: DateTime<UTC> = X509::parse_asn1_date("Feb 21 12:45:30 1999 GMT").unwrap();
    assert_eq!(dt.month(), 2);
    assert_eq!(dt.day(), 21);
    assert_eq!(dt.hour(), 12);
    assert_eq!(dt.minute(), 45);
    assert_eq!(dt.second(), 30);
    assert_eq!(dt.year(), 1999);
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// This is a wrapper around an OpenSSL asymmetric key pair
pub struct PKey {
    pub value: pkey::PKey,
}

impl Debug for PKey {
    fn fmt(&self, f: &mut Formatter) -> Result {
        // This impl will not write out the key, but it exists to keep structs happy
        // that contain a key as a field
        write!(f, "[pkey]")
    }
}

unsafe impl Send for PKey {}

impl PKey {
    pub fn wrap(pkey: pkey::PKey) -> PKey {
        PKey { value: pkey }
    }

    pub fn new(key_size: u32) -> PKey {
        PKey {
            value: {
                let rsa = rsa::Rsa::generate(key_size).unwrap();
                pkey::PKey::from_rsa(rsa).unwrap()
            },
        }
    }

    pub fn bit_length(&self) -> u32 {
        self.value.bits()
    }

    pub fn sign_sha1(&self, data: &[u8]) -> Vec<u8> {
        let mut signer = sign::Signer::new(hash::MessageDigest::sha1(), &self.value).unwrap();
        signer.update(data).unwrap();
        signer.finish().unwrap()
    }

    pub fn verify_sha1(&self, data: &[u8], signature: &[u8]) -> bool {
        let mut verifier = sign::Verifier::new(hash::MessageDigest::sha1(), &self.value).unwrap();
        verifier.update(data).unwrap();
        verifier.finish(signature).unwrap()
    }

    pub fn sign_sha256(&self, data: &[u8]) -> Vec<u8> {
        let mut signer = sign::Signer::new(hash::MessageDigest::sha256(), &self.value).unwrap();
        signer.update(data).unwrap();
        signer.finish().unwrap()
    }

    pub fn verify_sha256(&self, data: &[u8], signature: &[u8]) -> bool {
        let mut verifier = sign::Verifier::new(hash::MessageDigest::sha256(), &self.value).unwrap();
        verifier.update(data).unwrap();
        verifier.finish(signature).unwrap()
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// This is a wrapper around the OpenSSL AesKey type
pub struct AesKey {
    pub value: aes::AesKey,
}

impl Debug for AesKey {
    fn fmt(&self, f: &mut Formatter) -> Result {
        // This impl will not write out the key, but it exists to keep structs happy
        // that contain a key as a field
        write!(f, "[aes]")
    }
}

/// This allows key to be transferred between threads
unsafe impl Send for AesKey {}

impl AesKey {
    pub fn wrap(key: aes::AesKey) -> AesKey {
        AesKey { value: key }
    }

    pub fn new_encrypt(value: &[u8]) -> AesKey {
        AesKey { value: aes::AesKey::new_encrypt(&value).unwrap() }
    }

    pub fn new_decrypt(value: &[u8]) -> AesKey {
        AesKey { value: aes::AesKey::new_decrypt(&value).unwrap() }
    }
}