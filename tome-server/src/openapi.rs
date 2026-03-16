use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "tome API",
        version = "0.1.0",
        description = "HTTP API for the tome file change tracking system."
    ),
    paths(
        // repositories
        crate::routes::repositories::list_repositories,
        crate::routes::repositories::get_repository,
        crate::routes::repositories::list_snapshots,
        crate::routes::repositories::get_latest_snapshot,
        crate::routes::repositories::diff_snapshots,
        crate::routes::repositories::list_files,
        crate::routes::repositories::path_history,
        crate::routes::repositories::diff_repos,
        // snapshots
        crate::routes::snapshots::list_entries,
        // objects
        crate::routes::objects::get_object,
        crate::routes::objects::list_object_entries,
        // machines
        crate::routes::machines::list_machines,
        crate::routes::machines::register_machine,
        crate::routes::machines::update_machine,
        // admin
        crate::routes::admin::list_stores,
        crate::routes::admin::list_all_tags,
        crate::routes::admin::list_all_sync_peers,
        // sync
        crate::routes::sync::pull,
        crate::routes::sync::push,
    ),
    components(schemas(
        // shared response types
        crate::routes::responses::ErrorResponse,
        crate::routes::responses::RepositoryResponse,
        crate::routes::responses::SnapshotResponse,
        crate::routes::responses::EntryResponse,
        crate::routes::responses::ObjectResponse,
        crate::routes::responses::SnapshotEntry,
        crate::routes::responses::CacheEntryResponse,
        crate::routes::responses::MachineResponse,
        crate::routes::responses::StoreResponse,
        crate::routes::responses::TagResponse,
        crate::routes::responses::SyncPeerResponse,
        // repository diff/files responses
        crate::routes::repositories::DiffResponse,
        crate::routes::repositories::FilesResponse,
        crate::routes::repositories::RepoDiffResponse,
        // machine request
        crate::routes::machines::RegisterMachineRequest,
        // sync types
        crate::routes::sync::SyncEntry,
        crate::routes::sync::SyncReplica,
        crate::routes::sync::SyncSnapshot,
        crate::routes::sync::PullResponse,
        crate::routes::sync::PushRequest,
        crate::routes::sync::PushResponse,
    )),
    tags(
        (name = "repositories", description = "Repository and snapshot management"),
        (name = "snapshots",    description = "Snapshot entry queries"),
        (name = "objects",       description = "Object (blob / tree content) queries"),
        (name = "machines",     description = "Machine registration for sync"),
        (name = "admin",        description = "Stores, tags, and sync peers"),
        (name = "sync",         description = "Incremental sync protocol"),
    )
)]
pub struct ApiDoc;
