CREATE TABLE IF NOT EXISTS `stats` (
    `id` INTEGER NOT NULL AUTO_INCREMENT,
    `group_id` VARCHAR(128) NOT NULL,  -- FK
    `path` VARCHAR(512) NOT NULL,
    `history_id` INTEGER NOT NULL,         -- FK
    `version` INTEGER NOT NULL,            -- cached from history
    `status` INTEGER NOT NULL,             -- cached from history
    `mtime` DATETIME,                      -- cached from history
    `footprint_id` INTEGER,                -- cached from history, FK
    `digest` CHAR(64),                     -- cached from footprint
    `size` BIGINT,                         -- cached from footprint
    `fast_digest` BIGINT,                  -- cached from footprint
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`),
    UNIQUE (`group_id`, `path`)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE INDEX `stats_status_path` ON `stats` (`status`, `path`);
CREATE INDEX `stats_updated_at_path` ON `stats` (`updated_at`, `path`);
