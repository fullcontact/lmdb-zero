// Copyright 2016 FullContact, Inc
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cmp::{Ord, Ordering};
use std::ffi::CString;
use std::mem;
use std::ptr;
use libc::c_int;

use ffi;

use env::{self, Environment};
use error::{self, Error, Result};
use mdb_vals::*;
use traits::*;
use tx::TxHandle;

/// Flags used when opening databases.
pub mod db {
    use ffi;
    use libc;

    bitflags! {
        /// Flags used when opening databases.
        pub flags Flags : libc::c_uint {
            /// Keys are strings to be compared in reverse order, from the end
            /// of the strings to the beginning. By default, Keys are treated
            /// as strings and compared from beginning to end.
            ///
            /// NOTE: This is *not* reverse sort, but rather right-to-left
            /// comparison.
            ///
            /// ## Example
            ///
            /// ```
            /// # include!("src/example_helpers.rs");
            /// # fn main() {
            /// # let env = create_env();
            /// let db = lmdb::Database::open(
            ///   &env, Some("reversed"), &lmdb::DatabaseOptions::new(
            ///     lmdb::db::REVERSEKEY | lmdb::db::CREATE)).unwrap();
            /// let txn = lmdb::WriteTransaction::new(&env).unwrap();
            /// {
            ///   let mut access = txn.access();
            ///   let f = lmdb::put::Flags::empty();
            ///   access.put(&db, "Germany", "Berlin", f).unwrap();
            ///   access.put(&db, "Latvia", "Rīga", f).unwrap();
            ///   access.put(&db, "France", "Paris", f).unwrap();
            ///
            ///   let mut cursor = txn.cursor(&db).unwrap();
            ///   // The keys are compared as if we had input "aivtaL", "ecnarF",
            ///   // and "ynamreG", so "Latvia" comes first and "Germany" comes
            ///   // last.
            ///   assert_eq!(("Latvia", "Rīga"), cursor.first(&access).unwrap());
            ///   assert_eq!(("Germany", "Berlin"), cursor.last(&access).unwrap());
            /// }
            /// txn.commit().unwrap();
            /// # }
            /// ```
            const REVERSEKEY = ffi::MDB_REVERSEKEY,
            /// Duplicate keys may be used in the database. (Or, from another
            /// perspective, keys may have multiple data items, stored in
            /// sorted order.) By default keys must be unique and may have only
            /// a single data item.
            ///
            /// ## Example
            /// ```
            /// # include!("src/example_helpers.rs");
            /// # fn main() {
            /// # let env = create_env();
            /// let db = lmdb::Database::open(
            ///   &env, Some("example"), &lmdb::DatabaseOptions::new(
            ///     lmdb::db::DUPSORT | lmdb::db::CREATE)).unwrap();
            /// let txn = lmdb::WriteTransaction::new(&env).unwrap();
            /// {
            ///   let mut access = txn.access();
            ///   let f = lmdb::put::Flags::empty();
            ///   access.put(&db, "Fruit", "Orange", f).unwrap();
            ///   access.put(&db, "Fruit", "Apple", f).unwrap();
            ///   access.put(&db, "Veggie", "Carrot", f).unwrap();
            ///
            ///   let mut cursor = txn.cursor(&db).unwrap();
            ///   assert_eq!(("Fruit", "Apple"),
            ///              cursor.seek_k_both(&access, "Fruit").unwrap());
            ///   assert_eq!(("Fruit", "Orange"), cursor.next(&access).unwrap());
            ///   assert_eq!(("Veggie", "Carrot"), cursor.next(&access).unwrap());
            /// }
            /// txn.commit().unwrap();
            /// # }
            /// ```
            const DUPSORT = ffi::MDB_DUPSORT,
            /// Keys are binary integers in native byte order, either
            /// `libc::c_uint` or `libc::size_t`, and will be sorted as such.
            /// The keys must all be of the same size.
            ///
            /// ## Example
            ///
            /// ```
            /// # include!("src/example_helpers.rs");
            /// # fn main() {
            /// # let env = create_env();
            /// let db = lmdb::Database::open(
            ///   &env, Some("reversed"), &lmdb::DatabaseOptions::new(
            ///     lmdb::db::INTEGERKEY | lmdb::db::CREATE)).unwrap();
            /// let txn = lmdb::WriteTransaction::new(&env).unwrap();
            /// {
            ///   let mut access = txn.access();
            ///   let f = lmdb::put::Flags::empty();
            ///   // Write the keys in native byte order.
            ///   // Note that on little-endian systems this means a
            ///   // byte-by-byte comparison would not order the keys the way
            ///   // one might expect.
            ///   access.put(&db, &42u32, "Fourty-two", f).unwrap();
            ///   access.put(&db, &65536u32, "65'536", f).unwrap();
            ///   access.put(&db, &0u32, "Zero", f).unwrap();
            ///
            ///   let mut cursor = txn.cursor(&db).unwrap();
            ///   // But because we used `INTEGERKEY`, they are in fact sorted
            ///   // ascending by integer value.
            ///   assert_eq!((&0u32, "Zero"), cursor.first(&access).unwrap());
            ///   assert_eq!((&42u32, "Fourty-two"), cursor.next(&access).unwrap());
            ///   assert_eq!((&65536u32, "65'536"), cursor.next(&access).unwrap());
            /// }
            /// txn.commit().unwrap();
            /// # }
            /// ```
            const INTEGERKEY = ffi::MDB_INTEGERKEY,
            /// This flag may only be used in combination with `DUPSORT`. This
            /// option tells the library that the data items for this database
            /// are all the same size, which allows further optimizations in
            /// storage and retrieval. When all data items are the same size,
            /// the `get_multiple` and `next_multiple` cursor operations may be
            /// used to retrieve multiple items at once.
            ///
            /// ## Notes
            ///
            /// There are no runtime checks that values are actually the same
            /// size; failing to uphold this constraint may result in
            /// unpredictable behaviour.
            ///
            /// ## Example
            ///
            /// ```
            /// # include!("src/example_helpers.rs");
            /// # fn main() {
            /// # let env = create_env();
            /// let db = lmdb::Database::open(
            ///   &env, Some("reversed"), &lmdb::DatabaseOptions::new(
            ///     lmdb::db::DUPSORT | lmdb::db::DUPFIXED |
            ///     lmdb::db::INTEGERDUP | lmdb::db::CREATE))
            ///   .unwrap();
            /// let txn = lmdb::WriteTransaction::new(&env).unwrap();
            /// {
            ///   let mut access = txn.access();
            ///   let f = lmdb::put::Flags::empty();
            ///   // Map strings to their constituent chars
            ///   for s in &["foo", "bar", "xyzzy"] {
            ///     for c in s.chars() {
            ///       access.put(&db, *s, &c, f).unwrap();
            ///     }
            ///   }
            ///
            ///   // Read in all the chars of "xyzzy" in sorted order via
            ///   // cursoring.
            ///   let mut xyzzy: Vec<char> = Vec::new();
            ///   let mut cursor = txn.cursor(&db).unwrap();
            ///   cursor.seek_k::<str,char>(&access, "xyzzy").unwrap();
            ///   loop {
            ///     let chars = if xyzzy.is_empty() {
            ///       // First page.
            ///       // Note that in this example we probably get everything
            ///       // on the first page, but with more values we'd need to
            ///       // use `next_multiple` to get the rest.
            ///       cursor.get_multiple::<[char]>(&access).unwrap()
            ///     } else {
            ///       match cursor.next_multiple::<[char]>(&access) {
            ///         // Ok if there's still more for the current key
            ///         Ok(c) => c,
            ///         // Error if at the end of the key.
            ///         // NOTE: A real program would check the error code.
            ///         Err(_) => break,
            ///       }
            ///     };
            ///     xyzzy.extend(chars);
            ///   }
            ///   // Now we've read in all the values in sorted order.
            ///   assert_eq!(&['x','y','z'], &xyzzy[..]);
            /// }
            /// txn.commit().unwrap();
            /// # }
            /// ```
            const DUPFIXED = ffi::MDB_DUPFIXED,
            /// This option specifies that duplicate data items are binary
            /// integers, similar to `INTEGERKEY` keys.
            const INTEGERDUP = ffi::MDB_INTEGERDUP,
            /// This option specifies that duplicate data items should be
            /// compared as strings in reverse order.
            ///
            /// NOTE: As with `REVERSEKEY`, this indicates a right-to-left
            /// comparison, *not* an order reversal.
            ///
            /// ## Example
            ///
            /// # include!("src/example_helpers.rs");
            /// # fn main() {
            /// # let env = create_env();
            /// let db = lmdb::Database::open(
            ///   &env, Some("reversed"), &lmdb::DatabaseOptions::new(
            ///     lmdb::db::DUPSORT | lmdb::db::REVERSEDUP |
            ///     lmdb::db::CREATE)).unwrap();
            /// let txn = lmdb::WriteTransaction::new(&env).unwrap();
            /// {
            ///   let mut access = txn.access();
            ///   let f = lmdb::put::Flags::empty();
            ///   access.put(&db, "Colorado", "Denver", f).unwrap();
            ///   access.put(&db, "Colorado", "Golden", f).unwrap();
            ///   access.put(&db, "Colorado", "Lakewood", f).unwrap();
            ///
            ///   let mut cursor = txn.cursor(&db).unwrap();
            ///   // doowekaL, nedloG, revneD
            ///   assert_eq!(("Colorado", "Lakewood"), cursor.first(&access).unwrap());
            ///   assert_eq!(("Colorado", "Golden"), cursor.next(&access).unwrap());
            ///   assert_eq!(("Colorado", "Denver"), cursor.next(&access).unwrap());
            /// }
            /// txn.commit().unwrap();
            /// # }
            /// ```
            const REVERSEDUP = ffi::MDB_REVERSEDUP,
            /// Create the named database if it doesn't exist. This option is
            /// not allowed in a read-only environment.
            const CREATE = ffi::MDB_CREATE,
        }
    }
}

#[derive(Debug)]
struct DbHandle<'a> {
    env: &'a Environment,
    dbi: ffi::MDB_dbi,
}

impl<'a> Drop for DbHandle<'a> {
    fn drop(&mut self) {
        env::dbi_close(self.env, self.dbi);
    }
}

/// A handle on an LMDB database within an environment.
///
/// Note that in many respects the RAII aspect of this struct is more a matter
/// of convenience than correctness. In particular, if holding a read
/// transaction open, it is possible to obtain a handle to a database created
/// after that transaction started, but this handle will not point to anything
/// within that transaction.
///
/// The library does, however, guarantee that there will be at most one
/// `Database` object with the same dbi and environment per process.
#[derive(Debug)]
pub struct Database<'a> {
    db: DbHandle<'a>,
}

/// Describes the options used for creating or opening a database.
#[derive(Clone,Debug)]
pub struct DatabaseOptions {
    /// The integer flags to pass to LMDB
    pub flags: db::Flags,
    key_cmp: Option<ffi::MDB_cmp_func>,
    val_cmp: Option<ffi::MDB_cmp_func>,
}

impl DatabaseOptions {
    /// Creates a new `DatabaseOptions` with the given flags, using natural key
    /// and value ordering.
    pub fn new(flags: db::Flags) -> DatabaseOptions {
        DatabaseOptions {
            flags: flags,
            key_cmp: None,
            val_cmp: None,
        }
    }

    /// Synonym for `DatabaseOptions::new(db::Flags::empty())`
    pub fn defaults() -> DatabaseOptions {
        DatabaseOptions::new(db::Flags::empty())
    }

    /// Sorts keys in the database by interpreting them as `K` and using their
    /// comparison order. Keys which fail to convert are considered equal.
    ///
    /// The comparison function is called whenever it is necessary to compare a
    /// key specified by the application with a key currently stored in the
    /// database. If no comparison function is specified, and no special key
    /// flags were specified in the options, the keys are compared lexically,
    /// with shorter keys collating before longer keys.
    ///
    /// ## Warning
    ///
    /// This function must be called before any data access functions are used,
    /// otherwise data corruption may occur. The same comparison function must
    /// be used by every program accessing the database, every time the
    /// database is used.
    ///
    /// ## Example
    ///
    /// ```
    /// # include!("src/example_helpers.rs");
    /// #[repr(C)]
    /// #[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
    /// struct MyStruct {
    ///   x: i32,
    ///   y: i32,
    /// }
    /// unsafe impl lmdb::traits::LmdbRaw for MyStruct { }
    /// unsafe impl lmdb::traits::LmdbOrdKey for MyStruct { }
    ///
    /// fn my(x: i32, y: i32) -> MyStruct {
    ///   MyStruct { x: x, y: y }
    /// }
    ///
    /// # fn main() {
    /// # let env = create_env();
    /// let mut opts = lmdb::DatabaseOptions::new(lmdb::db::CREATE);
    /// opts.sort_keys_as::<MyStruct>();
    /// let db = lmdb::Database::open(&env, Some("example"), &opts).unwrap();
    /// let txn = lmdb::WriteTransaction::new(&env).unwrap();
    /// {
    ///   let mut access = txn.access();
    ///   let f = lmdb::put::Flags::empty();
    ///   access.put(&db, &my(0, 0), "origin", f).unwrap();
    ///   access.put(&db, &my(-1, 0), "x=-1", f).unwrap();
    ///   access.put(&db, &my(1, 0), "x=+1", f).unwrap();
    ///   access.put(&db, &my(0, -1), "y=-1", f).unwrap();
    ///
    ///   let mut cursor = txn.cursor(&db).unwrap();
    ///   // The keys are sorted by the Rust-derived comparison. The default
    ///   // byte-string comparison would have resulted in (0,0), (0,-1),
    ///   // (1,0), (-1,0).
    ///   assert_eq!((&my(-1, 0), "x=-1"), cursor.first(&access).unwrap());
    ///   assert_eq!((&my(0, -1), "y=-1"), cursor.next(&access).unwrap());
    ///   assert_eq!((&my(0, 0), "origin"), cursor.next(&access).unwrap());
    ///   assert_eq!((&my(1, 0), "x=+1"), cursor.next(&access).unwrap());
    /// }
    /// txn.commit().unwrap();
    /// # }
    pub fn sort_keys_as<K : LmdbOrdKey + ?Sized>(&mut self) {
        self.key_cmp = Some(DatabaseOptions::entry_cmp_as::<K>);
    }

    /// Sorts duplicate values in the database by interpreting them as `V` and
    /// using their comparison order. Values which fail to convert are
    /// considered equal.
    ///
    /// This comparison function is called whenever it is necessary to compare
    /// a data item specified by the application with a data item currently
    /// stored in the database. This function only takes effect if the database
    /// iss opened with the `DUPSORT` flag. If no comparison function is
    /// specified, and no special key flags were specified in the flags of
    /// these options, the data items are compared lexically, with shorter
    /// items collating before longer items.
    ///
    /// ## Warning
    ///
    /// This function must be called before any data access functions are used,
    /// otherwise data corruption may occur. The same comparison function must
    /// be used by every program accessing the database, every time the
    /// database is used.
    pub fn sort_values_as<V : LmdbOrdKey + ?Sized>(&mut self) {
        self.val_cmp = Some(DatabaseOptions::entry_cmp_as::<V>);
    }

    extern fn entry_cmp_as<V : LmdbOrdKey + ?Sized>(
        ap: *const ffi::MDB_val, bp: *const ffi::MDB_val) -> c_int
    {
        match unsafe {
            V::from_lmdb_bytes(mdb_val_as_bytes(&ap, &*ap)).cmp(
                &V::from_lmdb_bytes(mdb_val_as_bytes(&bp, &*bp)))
        } {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }
}

impl<'a> Database<'a> {
    /// Open a database in the environment.
    ///
    /// A database handle denotes the name and parameters of a database,
    /// independently of whether such a database exists. The database handle is
    /// implicitly closed when the `Database` object is dropped.
    ///
    /// To use named databases (with `name != None`),
    /// `EnvBuilder::set_maxdbs()` must have been called to reserve space for
    /// the extra databases. Database names are keys in the unnamed database,
    /// and may be read but not written.
    ///
    /// Transaction-local databases are not supported because the resulting
    /// ownership semantics are not expressible in rust. This call implicitly
    /// creates a write transaction and uses it to create the database, then
    /// commits it on success.
    ///
    /// One may not open the same database handle multiple times. Attempting to
    /// do so will result in the `REOPENED` error.
    ///
    /// ## Examples
    ///
    /// ### Open the default database with default options
    ///
    /// ```
    /// # include!("src/example_helpers.rs");
    /// # #[allow(unused_vars)]
    /// # fn main() {
    /// # let env = create_env();
    /// {
    ///   let db = lmdb::Database::open(
    ///     &env, None, &lmdb::DatabaseOptions::defaults()).unwrap();
    ///   // Do stuff with `db`
    /// } // The `db` handle is released
    /// # }
    /// ```
    ///
    /// ### Open a named database, creating it if it doesn't exist
    ///
    /// ```
    /// # include!("src/example_helpers.rs");
    /// # #[allow(unused_vars)]
    /// # fn main() {
    /// # let env = create_env();
    /// // NOT SHOWN: Call `EnvBuilder::set_maxdbs()` with a value greater than
    /// // one so that there is space for the named database(s).
    /// {
    ///   let db = lmdb::Database::open(
    ///     &env, Some("example-db"), &lmdb::DatabaseOptions::new(
    ///       lmdb::db::CREATE)).unwrap();
    ///   // Do stuff with `db`
    /// } // The `db` handle is released
    /// # }
    /// ```
    ///
    /// ### Trying to open the same database more than once
    /// ```
    /// # include!("src/example_helpers.rs");
    /// # #[allow(unused_vars)]
    /// # fn main() {
    /// # let env = create_env();
    /// {
    ///   let db = lmdb::Database::open(
    ///     &env, None, &lmdb::DatabaseOptions::defaults()).unwrap();
    ///   // Can't open the same database twice
    ///   assert!(lmdb::Database::open(
    ///     &env, None, &lmdb::DatabaseOptions::defaults()).is_err());
    /// }
    /// # }
    /// ```
    pub fn open(env: &'a Environment, name: Option<&str>,
                options: &DatabaseOptions)
                -> Result<Database<'a>> {
        let mut raw: ffi::MDB_dbi = 0;
        let name_cstr = match name {
            None => None,
            Some(s) => Some(try!(CString::new(s))),
        };
        let raw = unsafe {
            // Locking the hash set here is also used to serialise calls to
            // `mdb_dbi_open()`, which are not permitted to be concurrent.
            let mut locked_dbis = env::env_open_dbis(env).lock()
                .expect("open_dbis lock poisoned");

            let mut raw_tx: *mut ffi::MDB_txn = ptr::null_mut();
            lmdb_call!(ffi::mdb_txn_begin(
                env::env_ptr(env), ptr::null_mut(), 0, &mut raw_tx));
            let mut wrapped_tx = TxHandle(raw_tx); // For auto-closing etc
            lmdb_call!(ffi::mdb_dbi_open(
                raw_tx, name_cstr.map_or(ptr::null(), |s| s.as_ptr()),
                options.flags.bits(), &mut raw));

            if !locked_dbis.insert(raw) {
                return Err(Error { code: error::REOPENED });
            }

            if let Some(fun) = options.key_cmp {
                lmdb_call!(ffi::mdb_set_compare(
                    // XXX lmdb_sys's declaration of this function is incorrect
                    // due to the `MDB_cmp_func` typedef (which is a bare
                    // function in C) being translated as a pointer.
                    raw_tx, raw, mem::transmute(fun)));
            }
            if let Some(fun) = options.val_cmp {
                lmdb_call!(ffi::mdb_set_dupsort(
                    // XXX see above
                    raw_tx, raw, mem::transmute(fun)));
            }

            try!(wrapped_tx.commit());
            raw
        };

        Ok(Database {
            db: DbHandle {
                env: env,
                dbi: raw,
            }
        })
    }

    /// Deletes this database.
    ///
    /// This call implicitly creates a new write transaction to perform the
    /// operation, so that the lifetime of the database handle does not depend
    /// on the outcome. The database handle is closed implicitly by this
    /// operation.
    ///
    /// Note that the _other_ `mdb_drop` operation which simply clears the
    /// database is exposed through `WriteAccessor` and is transactional.
    ///
    /// ## Example
    ///
    /// ```
    /// # include!("src/example_helpers.rs");
    /// # #[allow(unused_vars)]
    /// # fn main() {
    /// # let env = create_env();
    /// // NOT SHOWN: Call `EnvBuilder::set_maxdbs()` with a value greater than
    /// // one so that there is space for the named database(s).
    /// {
    ///   let db = lmdb::Database::open(
    ///     &env, Some("example-db"), &lmdb::DatabaseOptions::new(
    ///       lmdb::db::CREATE)).unwrap();
    ///   // Do stuff with `db`
    ///
    ///   // Delete the database itself. This also consumes `db`.
    ///   db.delete().unwrap();
    ///
    ///   // We can now recreate the database if we so desire.
    ///   // Note that you should not use delete+open to clear a database; use
    ///   // `WriteAccessor::clear_db()` to do that.
    ///   let db = lmdb::Database::open(
    ///     &env, Some("example-db"), &lmdb::DatabaseOptions::new(
    ///       lmdb::db::CREATE)).unwrap();
    /// }
    /// # }
    /// ```
    pub fn delete(self) -> Result<()> {
        try!(env::dbi_delete(self.db.env, self.db.dbi));
        mem::forget(self.db);
        Ok(())
    }

    /// Checks that `other_env` is the same as the environment on this
    /// `Database`.
    ///
    /// If it matches, returns `Ok(())`; otherwise, returns `Err`.
    pub fn assert_same_env(&self, other_env: &Environment)
                           -> Result<()> {
        if self.db.env as *const Environment !=
            other_env as *const Environment
        {
            Err(Error { code: error::MISMATCH })
        } else {
            Ok(())
        }
    }

    /// Returns the underlying integer handle for this database.
    pub fn dbi(&self) -> ffi::MDB_dbi {
        self.db.dbi
    }
}
