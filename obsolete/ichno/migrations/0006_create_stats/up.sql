CREATE TABLE `stats` (
    `id` BIGINT NOT NULL PRIMARY KEY,
    `workspace_id` BIGINT NOT NULL,  -- cached from group, FK
    `group_id` BIGINT NOT NULL,      -- FK
    `path` TEXT NOT NULL,
    `history_id` BIGINT NOT NULL,    -- FK
    `version` INTEGER NOT NULL,      -- cached from history
    `status` INTEGER NOT NULL,       -- cached from history
    `mtime` DATETIME,                -- cached from history
    `footprint_id` BIGINT,           -- cached from history, FK
    `digest` BLOB,               -- cached from footprint
    `size` BIGINT,                   -- cached from footprint
    `fast_digest` BIGINT,            -- cached from footprint
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`group_id`, `path`)
)
;

CREATE INDEX `ix_stats_workspace_id_updated_at` ON `stats` (`workspace_id`, `updated_at`);
CREATE INDEX `ix_stats_workspace_id_footprint_id_path_version` ON `stats` (`workspace_id`, `footprint_id`, `path`, `version`);
CREATE INDEX `ix_stats_workspace_id_footprint_id_mtime` ON `stats` (`workspace_id`, `footprint_id`, `mtime`);
