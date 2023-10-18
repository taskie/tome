#[cfg(feature = "postgres")]
type OmTimestamp = diesel::sql_types::Timestamptz;
#[cfg(feature = "mysql")]
type OmTimestamp = diesel::sql_types::Datetime;

table! {
    attrs (id) {
        id -> Integer,
        workspace_id -> Integer,
        target_footprint_id -> Integer,
        target_digest -> Char,
        key -> Varchar,
        value_footprint_id -> Integer,
        value_digest -> Char,
        value_content_type -> Integer,
        value_summary -> Nullable<Varchar>,
        status -> Integer,
        attr_stat_id -> Nullable<Integer>,
        created_at -> crate::db::schema::OmTimestamp,
        updated_at -> crate::db::schema::OmTimestamp,
    }
}

table! {
    contents (id) {
        id -> Integer,
        footprint_id -> Integer,
        body -> Blob,
        created_at -> crate::db::schema::OmTimestamp,
    }
}

table! {
    footprints (id) {
        id -> Integer,
        digest -> Char,
        size -> Bigint,
        fast_digest -> Bigint,
        created_at -> crate::db::schema::OmTimestamp,
    }
}

table! {
    groups (id) {
        id -> Integer,
        workspace_id -> Integer,
        name -> Varchar,
        url -> Varchar,
        #[sql_name = "type"]
        type_ -> Integer,
        description -> Varchar,
        status -> Integer,
        group_stat_id -> Nullable<Integer>,
        created_at -> crate::db::schema::OmTimestamp,
        updated_at -> crate::db::schema::OmTimestamp,
    }
}

table! {
    histories (id) {
        id -> Integer,
        workspace_id -> Integer,
        group_id -> Integer,
        path -> Varchar,
        version -> Integer,
        status -> Integer,
        mtime -> Nullable<crate::db::schema::OmTimestamp>,
        footprint_id -> Nullable<Integer>,
        digest -> Nullable<Char>,
        created_at -> crate::db::schema::OmTimestamp,
        updated_at -> crate::db::schema::OmTimestamp,
    }
}

table! {
    stats (id) {
        id -> Integer,
        workspace_id -> Integer,
        group_id -> Integer,
        path -> Varchar,
        history_id -> Integer,
        version -> Integer,
        status -> Integer,
        mtime -> Nullable<crate::db::schema::OmTimestamp>,
        footprint_id -> Nullable<Integer>,
        digest -> Nullable<Char>,
        size -> Nullable<Bigint>,
        fast_digest -> Nullable<Bigint>,
        created_at -> crate::db::schema::OmTimestamp,
        updated_at -> crate::db::schema::OmTimestamp,
    }
}

table! {
    workspaces (id) {
        id -> Integer,
        name -> Varchar,
        description -> Varchar,
        status -> Integer,
        created_at -> crate::db::schema::OmTimestamp,
        updated_at -> crate::db::schema::OmTimestamp,
    }
}

joinable!(attrs -> stats (attr_stat_id));
joinable!(attrs -> workspaces (workspace_id));
joinable!(contents -> footprints (footprint_id));
joinable!(groups -> workspaces (workspace_id));
joinable!(histories -> footprints (footprint_id));
joinable!(stats -> footprints (footprint_id));
joinable!(stats -> histories (history_id));

allow_tables_to_appear_in_same_query!(attrs, contents, footprints, groups, histories, stats, workspaces,);
