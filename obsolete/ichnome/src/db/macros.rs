#[macro_export]
macro_rules! impl_find {
    ( $conn: ty, $table: ident, $t: ty ) => {
        pub fn find(conn: &mut $conn, id: i64) -> Result<::std::option::Option<$t>, Box<dyn ::std::error::Error>> {
            use crate::db::schema::$table::dsl;
            let q = dsl::$table.find(id);
            Ok(q.first::<$t>(conn).optional()?)
        }
    };
    ( $conn: ty, $table: ident, $t: ty; $n: ident, $( $arg: ident : $arg_t: ty ),+ ) => {
        pub fn $n(conn: &mut $conn, $($arg: $arg_t ,)+) -> Result<::std::option::Option<$t>, Box<dyn ::std::error::Error>> {
            use crate::db::schema::$table::dsl;
            let q = dsl::$table
            $(
                .filter(dsl::$arg.eq($arg))
            )+
            ;
            Ok(q.first::<$t>(conn).optional()?)
        }
    };
}

#[macro_export]
macro_rules! impl_select {
    ( $conn: ty, $table: ident, $t: ty ) => {
        pub fn select(conn: &mut $conn, ids: &Vec<i64>) -> Result<::std::vec::Vec<$t>, Box<dyn Error>> {
            use crate::db::schema::$table::dsl;
            let q = dsl::$table.filter(dsl::id.eq_any(ids));
            Ok(q.load::<$t>(conn)?)
        }
    };
    ( $conn: ty, $table: ident, $t: ty; $n: ident, $( $arg: ident : $arg_t: ty ),+ ) => {
        pub fn $n(conn: &mut $conn, $($arg: $arg_t ,)+) -> Result<::std::vec::Vec<$t>, Box<dyn Error>> {
            use crate::db::schema::$table::dsl;
            let q = dsl::$table
            $(
                .filter(dsl::$arg.eq($arg))
            )+
            ;
            Ok(q.load::<$t>(conn)?)
        }
    };
}

#[macro_export]
macro_rules! impl_insert {
    ( $conn: ty, $table: ident, $t: ty ) => {
        pub fn insert(conn: &mut $conn, insert_form: &$t) -> Result<(), Box<dyn Error>> {
            use crate::db::schema::$table::dsl;
            let q = ::diesel::insert_into(dsl::$table).values(insert_form);
            q.execute(conn)?;
            Ok(())
        }
    };
}

#[macro_export]
macro_rules! impl_insert_and_find {
    ( $conn: ty, $table: ident, $t: ty, $r: ty; $n: ident, $( $arg: ident ),+ ) => {
        pub fn insert_and_find(conn: &mut $conn, insert_form: &$t) -> Result<$r, Box<dyn Error>> {
            Self::insert(conn, insert_form)?;
            let inserted = Self::$n(conn, $(insert_form.$arg),+)?;
            Ok(inserted.unwrap())
        }
    };
    ( $conn: ty, $table: ident, $t: ty, $r: ty ) => {
        impl_insert_and_find!($conn, $table, $t, $r, find, id);
    };
}

#[macro_export]
macro_rules! impl_update {
    ( $conn: ty, $table: ident, $t: ty ) => {
        pub fn update(conn: &mut $conn, id: i64, update_form: &$t) -> Result<(), Box<dyn Error>> {
            use crate::db::schema::$table::dsl;
            let q = ::diesel::update(dsl::$table.find(id)).set(update_form);
            let n = q.execute(conn)?;
            assert_eq!(1, n);
            Ok(())
        }
    };
}

#[macro_export]
macro_rules! impl_update_and_find {
    ( $conn: ty, $table: ident, $t: ty, $r: ty ) => {
        pub fn update_and_find(conn: &mut $conn, id: i64, update_form: &$t) -> Result<$r, Box<dyn Error>> {
            Self::update(conn, id, update_form)?;
            let updated = Self::find(conn, id)?;
            Ok(updated.unwrap())
        }
    };
}

#[macro_export]
macro_rules! impl_find_by_main_keys {
    ( $conn: ty, $table: ident, $t: ty, $i: ty; $n: ident, $( $arg: ident : $arg_t: ty ),+ ) => {
        $crate::impl_find!($conn, $table, $t; $n, $($arg: $arg_t),+);
        $crate::impl_insert_and_find!($conn, $table, $i, $t; $n, $($arg),+);
    };
}

#[macro_export]
macro_rules! impl_crud {
    ( $conn: ty, $table: ident, $t: ty ) => {
        $crate::impl_find!($conn, $table, $t);
        $crate::impl_select!($conn, $table, $t);
    };
    ( $conn: ty, $table: ident, $t: ty, $i: ty ) => {
        $crate::impl_crud!($conn, $table, $t);
        $crate::impl_insert!($conn, $table, $i);
    };
    ( $conn: ty, $table: ident, $t: ty, $i: ty; $n: ident, $( $arg: ident : $arg_t: ty ),+ ) => {
        $crate::impl_crud!($conn, $table, $t, $i);
        $crate::impl_find_by_main_keys!($conn, $table, $t, $i; $n, $($arg: $arg_t),+);
    };
    ( $conn: ty, $table: ident, $t: ty, $i: ty, $u: ty ) => {
        $crate::impl_crud!($conn, $table, $t, $i);
        $crate::impl_update!($conn, $table, $u);
        $crate::impl_update_and_find!($conn, $table, $u, $t);
    };
    ( $conn: ty, $table: ident, $t: ty, $i: ty, $u: ty; $n: ident, $( $arg: ident : $arg_t: ty ),+ ) => {
        $crate::impl_crud!($conn, $table, $t, $i, $u);
        $crate::impl_find_by_main_keys!($conn, $table, $t, $i; $n, $($arg: $arg_t),+);
    };
}
