//! sqlite-vec extension registration.
//!
//! This module isolates the unsafe SQLite extension registration required by
//! sqlite-vec. Query, row mapping, and vector validation stay in the index
//! module so the unsafe boundary remains small and auditable.

use std::sync::Once;
use std::{ffi::c_char, os::raw::c_int};

use rusqlite::ffi::{sqlite3, sqlite3_api_routines, sqlite3_auto_extension};
use sqlite_vec::sqlite3_vec_init;

static REGISTER_SQLITE_VEC: Once = Once::new();

/// Registers sqlite-vec once for rusqlite connections in this process.
///
/// Registration is process-global in SQLite, so repeated index construction
/// calls this helper safely through `Once`.
pub(crate) fn register_sqlite_vec() {
    REGISTER_SQLITE_VEC.call_once(|| {
        // SAFETY: sqlite-vec exposes `sqlite3_vec_init` with the SQLite
        // extension ABI. This follows the crate's own rusqlite registration
        // example and is isolated here so unsafe extension registration does
        // not leak into adapter logic.
        unsafe {
            type ExtensionInit = unsafe extern "C" fn(
                *mut sqlite3,
                *mut *mut c_char,
                *const sqlite3_api_routines,
            ) -> c_int;
            let init =
                std::mem::transmute::<*const (), ExtensionInit>(sqlite3_vec_init as *const ());
            sqlite3_auto_extension(Some(init));
        }
    });
}
