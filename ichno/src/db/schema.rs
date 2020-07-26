table! {
    attrs (id) {
        id -> Integer,
        workspace_id -> Integer,
        target_footprint_id -> Integer,
        target_digest -> Text,
        key -> Text,
        value_footprint_id -> Integer,
        value_digest -> Text,
        value_content_type -> Integer,
        value_summary -> Nullable<Text>,
        status -> Integer,
        attr_stat_id -> Nullable<Integer>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    contents (id) {
        id -> Integer,
        footprint_id -> Integer,
        body -> Binary,
        created_at -> Timestamp,
    }
}

table! {
    footprints (id) {
        id -> Integer,
        digest -> Text,
        size -> BigInt,
        fast_digest -> BigInt,
        created_at -> Timestamp,
    }
}

table! {
    groups (id) {
        id -> Integer,
        workspace_id -> Integer,
        name -> Text,
        url -> Text,
        #[sql_name = "type"]
        type_ -> Integer,
        description -> Text,
        status -> Integer,
        group_stat_id -> Nullable<Integer>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    histories (id) {
        id -> Integer,
        workspace_id -> Integer,
        group_id -> Integer,
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
        workspace_id -> Integer,
        group_id -> Integer,
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

table! {
    workspaces (id) {
        id -> Integer,
        name -> Text,
        description -> Text,
        status -> Integer,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

allow_tables_to_appear_in_same_query!(attrs, contents, footprints, groups, histories, stats, workspaces,);
