//! Starknet primitives.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

#[doc(hidden)]
pub use codec;
#[doc(hidden)]
pub use scale_info;
#[cfg(feature = "std")]
#[doc(hidden)]
pub use serde;
#[doc(hidden)]
pub use sp_std;
#[doc(hidden)]
pub extern crate alloc;
// Include modules.
/// Starknet block related functionality.
pub mod starknet_block;

/// Starknet transaction related functionality.
pub mod transaction;

/// Starknet crypto related functionality.
pub mod crypto;

/// Starknet storage primitives.
pub mod storage;

/// Starknet state related functionality.
pub mod state;

/// Serializations and deserializations.
pub mod starknet_serde;

/// Starknet primitives traits.
pub mod traits;

/// Starknet Execution related functionality.
pub mod execution;
