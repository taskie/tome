CREATE TABLE `histories` (
    `id` BIGINT NOT NULL PRIMARY KEY,
    `workspace_id` BIGINT NOT NULL,  -- cached from group, FK
    `group_id` BIGINT NOT NULL,      -- FK
    `path` TEXT NOT NULL,
    `version` INTEGER NOT NULL,
    `status` INTEGER NOT NULL,
    `mtime` DATETIME,
    `footprint_id` BIGINT,           -- FK
    `digest` BLOB,               -- cached from footprint
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`group_id`, `path`, `version`)
)
;

CREATE INDEX `ix_histories_workspace_id_updated_at` ON `histories` (`workspace_id`, `updated_at`);
CREATE INDEX `ix_histories_workspace_id_footprint_id_path_version` ON `histories` (`workspace_id`, `footprint_id`, `path`, `version`);
CREATE INDEX `ix_histories_workspace_id_footprint_id_mtime` ON `histories` (`workspace_id`, `footprint_id`, `mtime`);
