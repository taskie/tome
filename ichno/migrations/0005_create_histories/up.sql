CREATE TABLE IF NOT EXISTS `histories` (
    `id` INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    `workspace_id` INTEGER NOT NULL,  -- cached from group, FK
    `group_id` INTEGER NOT NULL,      -- FK
    `path` VARCHAR(512) NOT NULL,
    `version` INTEGER NOT NULL,
    `status` INTEGER NOT NULL,
    `mtime` DATETIME,
    `footprint_id` INTEGER,           -- FK
    `digest` CHAR(64),                -- cached from footprint
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`group_id`, `path`, `version`)
)
-- DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
;

-- ALTER TABLE `histories` ADD FOREIGN KEY `fk_histories_workspace_id_group_id` (`workspace_id`, `group_id`) REFERENCES `groups` (`workspace_id`, `id`);
-- ALTER TABLE `histories` ADD FOREIGN KEY `fk_histories_footprint_id` (`footprint_id`) REFERENCES `footprints` (`id`);

CREATE INDEX `ix_histories_workspace_id_updated_at` ON `histories` (`workspace_id`, `updated_at`);
CREATE INDEX `ix_histories_workspace_id_footprint_id_path_version` ON `histories` (`workspace_id`, `footprint_id`, `path`, `version`);
CREATE INDEX `ix_histories_workspace_id_footprint_id_mtime` ON `histories` (`workspace_id`, `footprint_id`, `mtime`);
