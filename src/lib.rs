//! # `ffi-pool`: useful object pool types for FFI code
//!
//! This crate contains some useful object pool types for interfacing with C code (at the moment,
//! just `CStringPool`.)


#[cfg(test)]
#[macro_use]
extern crate lazy_static;

extern crate memchr;
extern crate objpool;
extern crate take_mut;

use std::error::Error;
use std::ffi::{CStr, CString};
use std::fmt;
use std::sync::Arc;

use objpool::{Item, Pool};


/// An error returned upon finding a nul byte in a string we are attempting to convert to a
/// `CString`.
#[derive(Debug, Clone, Copy)]
pub struct NulError {
    pub position: usize,
}


impl fmt::Display for NulError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "nul byte found in provided data at position: {}", self.position)
    }
}


impl Error for NulError {
    fn description(&self) -> &str { "nul byte found in data" }
}


/// A thread-safe pool of `CString`s which can be readily reused with `str`s for ease of FFI interactions.
#[derive(Debug, Clone)]
pub struct CStringPool {
    pool: Arc<Pool<CString>>,
}


impl CStringPool {
    /// Create a new pool with a given default capacity for newly allocated `CString`s.
    pub fn new(default_string_capacity: usize) -> CStringPool {
        CStringPool {
            pool: Pool::new(move || {
                let vec = Vec::with_capacity(default_string_capacity);

                // The `Vec` is empty and thus contains no nul bytes.
                unsafe { CString::from_vec_unchecked(vec) }
            }),
        }
    }


    /// Create a new pool with an additional maximum capacity. Allocating new `CString`s when the
    /// pool is at capacity will block until a new `CString` is available.
    pub fn with_capacity(pool_capacity: usize, default_string_capacity: usize) -> CStringPool {
        CStringPool {
            pool: Pool::with_capacity(pool_capacity, move || {
                let vec = Vec::with_capacity(default_string_capacity);

                // The `Vec` is empty and thus contains no nul bytes.
                unsafe { CString::from_vec_unchecked(vec) }
            }),
        }
    }


    /// Allocate a new `CString` from the pool. This will check the supplied `str` for interior nul
    /// bytes.
    pub fn get_str<T: AsRef<str>>(&self, s: T) -> Result<Item<CString>, NulError> {
        let str_ref = s.as_ref();

        // Ensure our str contains no nul bytes and is thus safe to inject into a `CString`.
        if let Some(i) = memchr::memchr(0, str_ref.as_bytes()) {
            return Err(NulError { position: i });
        }

        let mut item = self.pool.get();
        take_mut::take(&mut *item, |cstring| {
            // We are guaranteed that if a `CString` is in the pool, it is either empty or created from
            // an `&str`. Thus, it is safe to convert as it *always* contains valid unicode data.
            let mut string = unsafe { String::from_utf8_unchecked(cstring.into_bytes()) };

            string.clear();
            string.push_str(str_ref);

            // We check for nul bytes outside of this block so that we can return an error instead of
            // panicking.
            unsafe { CString::from_vec_unchecked(string.into_bytes()) }
        });

        Ok(item)
    }


    /// Allocate a new `CString` from the pool, using a `CStr` as a source.
    pub fn get_c_str<T: AsRef<CStr>>(&self, s: T) -> Item<CString> {
        let str_ref = s.as_ref();

        let mut item = self.pool.get();
        take_mut::take(&mut *item, |cstring| {
            let mut bytes = cstring.into_bytes();

            bytes.clear();
            bytes.extend(str_ref.to_bytes());

            // These bytes came from a `CStr`. There is no way they have a nul byte inside.
            unsafe { CString::from_vec_unchecked(bytes) }
        });

        item
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    lazy_static! {
        static ref POOL: CStringPool = CStringPool::new(128);
    }

    #[test]
    fn round_trip() {
        let s = "foo";
        let cstr = POOL.get_str(s).unwrap();

        assert_eq!(cstr.to_str().unwrap(), s);
    }


    #[test]
    #[should_panic]
    fn bad_string() {
        let s = "fo\0o";
        let _cstr = POOL.get_str(s).unwrap();
    }
}
