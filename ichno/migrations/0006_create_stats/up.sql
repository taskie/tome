CREATE TABLE IF NOT EXISTS `stats` (
    `id` INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    `workspace_id` INTEGER NOT NULL,  -- cached from group, FK
    `group_id` INTEGER NOT NULL,      -- FK
    `path` VARCHAR(512) NOT NULL,
    `history_id` INTEGER NOT NULL,    -- FK
    `version` INTEGER NOT NULL,       -- cached from history
    `status` INTEGER NOT NULL,        -- cached from history
    `mtime` DATETIME,                 -- cached from history
    `footprint_id` INTEGER,           -- cached from history, FK
    `digest` CHAR(64),                -- cached from footprint
    `size` BIGINT,                    -- cached from footprint
    `fast_digest` BIGINT,             -- cached from footprint
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`group_id`, `path`)
)
-- DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
;

-- ALTER TABLE `stats` ADD FOREIGN KEY `fk_stats_workspace_id_group_id` (`workspace_id`, `group_id`) REFERENCES `groups`(`workspace_id`, `id`);
-- ALTER TABLE `stats` ADD FOREIGN KEY `fk_stats_history_id` (`history_id`) REFERENCES `histories`(`id`);
-- ALTER TABLE `stats` ADD FOREIGN KEY `fk_stats_footprint_id` (`footprint_id`) REFERENCES `footprints` (`id`);

-- ALTER TABLE `groups` ADD FOREIGN KEY `fk_groups_group_stat_id` (`group_stat_id`) REFERENCES `stats` (`id`);

CREATE INDEX `ix_stats_workspace_id_updated_at` ON `stats` (`workspace_id`, `updated_at`);
CREATE INDEX `ix_stats_workspace_id_footprint_id_path_version` ON `stats` (`workspace_id`, `footprint_id`, `path`, `version`);
CREATE INDEX `ix_stats_workspace_id_footprint_id_mtime` ON `stats` (`workspace_id`, `footprint_id`, `mtime`);
