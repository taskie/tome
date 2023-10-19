table! {
    attrs (id) {
        id -> BigInt,
        workspace_id -> BigInt,
        target_footprint_id -> BigInt,
        target_digest -> Binary,
        key -> Text,
        value_type -> Integer,
        value_footprint_id -> BigInt,
        value_digest -> Binary,
        value_text -> Nullable<Text>,
        status -> Integer,
        attr_stat_id -> Nullable<BigInt>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

table! {
    contents (id) {
        id -> BigInt,
        footprint_id -> BigInt,
        body -> Binary,
        created_at -> Timestamptz,
    }
}

table! {
    footprints (id) {
        id -> BigInt,
        digest -> Binary,
        size -> BigInt,
        fast_digest -> BigInt,
        created_at -> Timestamptz,
    }
}

table! {
    groups (id) {
        id -> BigInt,
        workspace_id -> BigInt,
        name -> Text,
        url -> Text,
        #[sql_name = "type"]
        type_ -> Integer,
        description -> Text,
        status -> Integer,
        group_stat_id -> Nullable<BigInt>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

table! {
    histories (id) {
        id -> BigInt,
        workspace_id -> BigInt,
        group_id -> BigInt,
        path -> Text,
        version -> Integer,
        status -> Integer,
        mtime -> Nullable<Timestamptz>,
        footprint_id -> Nullable<BigInt>,
        digest -> Nullable<Binary>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

table! {
    stats (id) {
        id -> BigInt,
        workspace_id -> BigInt,
        group_id -> BigInt,
        path -> Text,
        history_id -> BigInt,
        version -> Integer,
        status -> Integer,
        mtime -> Nullable<Timestamptz>,
        footprint_id -> Nullable<BigInt>,
        digest -> Nullable<Binary>,
        size -> Nullable<BigInt>,
        fast_digest -> Nullable<BigInt>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

table! {
    workspaces (id) {
        id -> BigInt,
        name -> Text,
        description -> Text,
        status -> Integer,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

allow_tables_to_appear_in_same_query!(attrs, contents, footprints, groups, histories, stats, workspaces,);
