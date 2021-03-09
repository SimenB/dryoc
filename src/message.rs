use crate::types::InputBase;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use zeroize::Zeroize;

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize, Zeroize, Debug, PartialEq)
)]
#[cfg_attr(not(feature = "serde"), derive(Zeroize, Debug, PartialEq))]
#[zeroize(drop)]
/// Message container, for use with unencrypted messages
pub struct Message(pub Box<InputBase>);

impl From<Vec<u8>> for Message {
    fn from(v: Vec<u8>) -> Self {
        Self(Box::from(v.as_slice()))
    }
}

impl From<&[u8]> for Message {
    fn from(a: &[u8]) -> Self {
        Self(Box::from(a))
    }
}

impl From<String> for Message {
    fn from(s: String) -> Self {
        Self(Box::from(s.as_bytes()))
    }
}

impl From<&str> for Message {
    fn from(s: &str) -> Self {
        Self(Box::from(s.as_bytes()))
    }
}