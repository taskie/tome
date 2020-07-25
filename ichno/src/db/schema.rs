table! {
    attrs (id) {
        id -> Integer,
        entity_type -> Integer,
        entity_id -> Integer,
        group_id -> Nullable<Text>,
        path -> Nullable<Text>,
        version -> Nullable<Integer>,
        digest -> Nullable<Text>,
        key -> Text,
        value_footprint_id -> Integer,
        value_digest -> Text,
        value_content_type -> Integer,
        status -> Integer,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    contents (footprint_id) {
        footprint_id -> Integer,
        digest -> Text,
        body -> Binary,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    footprints (id) {
        id -> Integer,
        digest -> Text,
        size -> BigInt,
        fast_digest -> BigInt,
        git_object_id -> Text,
    }
}

table! {
    groups (id) {
        id -> Text,
        url -> Text,
        #[sql_name = "type"]
        type_ -> Integer,
        history_id -> Nullable<Integer>,
        version -> Nullable<Integer>,
        status -> Nullable<Integer>,
        mtime -> Nullable<Timestamp>,
        footprint_id -> Nullable<Integer>,
        digest -> Nullable<Text>,
        size -> Nullable<BigInt>,
        fast_digest -> Nullable<BigInt>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    histories (id) {
        id -> Integer,
        group_id -> Text,
        path -> Text,
        version -> Integer,
        status -> Integer,
        mtime -> Nullable<Timestamp>,
        footprint_id -> Nullable<Integer>,
        digest -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    stats (id) {
        id -> Integer,
        group_id -> Text,
        path -> Text,
        history_id -> Integer,
        version -> Integer,
        status -> Integer,
        mtime -> Nullable<Timestamp>,
        footprint_id -> Nullable<Integer>,
        digest -> Nullable<Text>,
        size -> Nullable<BigInt>,
        fast_digest -> Nullable<BigInt>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
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
