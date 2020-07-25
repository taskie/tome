table! {
    attrs (id) {
        id -> Integer,
        entity_type -> Integer,
        entity_id -> Integer,
        group_id -> Nullable<Varchar>,
        path -> Nullable<Varchar>,
        version -> Nullable<Integer>,
        digest -> Nullable<Char>,
        key -> Varchar,
        value_footprint_id -> Integer,
        value_digest -> Char,
        value_content_type -> Integer,
        status -> Integer,
        created_at -> Datetime,
        updated_at -> Datetime,
    }
}

table! {
    contents (footprint_id) {
        footprint_id -> Integer,
        digest -> Char,
        body -> Blob,
        created_at -> Datetime,
        updated_at -> Datetime,
    }
}

table! {
    footprints (id) {
        id -> Integer,
        digest -> Char,
        size -> Bigint,
        fast_digest -> Bigint,
        git_object_id -> Char,
    }
}

table! {
    groups (id) {
        id -> Varchar,
        url -> Varchar,
        #[sql_name = "type"]
        type_ -> Integer,
        history_id -> Nullable<Integer>,
        version -> Nullable<Integer>,
        status -> Nullable<Integer>,
        mtime -> Nullable<Datetime>,
        footprint_id -> Nullable<Integer>,
        digest -> Nullable<Char>,
        size -> Nullable<Bigint>,
        fast_digest -> Nullable<Bigint>,
        created_at -> Datetime,
        updated_at -> Datetime,
    }
}

table! {
    histories (id) {
        id -> Integer,
        group_id -> Varchar,
        path -> Varchar,
        version -> Integer,
        status -> Integer,
        mtime -> Nullable<Datetime>,
        footprint_id -> Nullable<Integer>,
        digest -> Nullable<Char>,
        created_at -> Datetime,
        updated_at -> Datetime,
    }
}

table! {
    stats (id) {
        id -> Integer,
        group_id -> Varchar,
        path -> Varchar,
        history_id -> Integer,
        version -> Integer,
        status -> Integer,
        mtime -> Nullable<Datetime>,
        footprint_id -> Nullable<Integer>,
        digest -> Nullable<Char>,
        size -> Nullable<Bigint>,
        fast_digest -> Nullable<Bigint>,
        created_at -> Datetime,
        updated_at -> Datetime,
    }
}

allow_tables_to_appear_in_same_query!(
    attrs,
    contents,
    footprints,
    groups,
    histories,
    stats,
);
