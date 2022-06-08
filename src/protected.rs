//! This is a basic wrapper for secret/hidden values
//!
//! It implements zeroize-on-drop, meaning the data is securely erased from memory once it goes out of scope.
//! You may call `drop()` prematurely if you wish to erase it sooner.
//!
//! `Protected` values are also hidden from `fmt::Debug`, and will display `[REDACTED]` instead.
//!
//! The only way to access the data within a `Protected` value is to call `.expose()` - this is to prevent accidental leakage.
//! This also makes any `Protected` value easier to audit, as you are able to quickly view wherever the data is accessed.
//!
//! `Protected` values are not able to be copied or cloned within memory, to prevent accidental leakage.
//!
//! I'd like to give a huge thank you to the authors of the [secrecy crate](https://crates.io/crates/secrecy),
//! as that crate's functionality inspired this implementation.

use std::fmt::Debug;
use zeroize::Zeroize;

pub struct Protected<T>
where
    T: Zeroize,
{
    data: T,
}

impl<T> Protected<T>
where
    T: Zeroize,
{
    pub fn new(value: T) -> Self {
        Protected { data: value }
    }

    pub fn expose(&self) -> &T {
        &self.data
    }
}

impl<T> Drop for Protected<T>
where
    T: Zeroize,
{
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

impl<T> Debug for Protected<T>
where
    T: Zeroize,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}
