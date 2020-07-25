CREATE TABLE IF NOT EXISTS `histories` (
    `id` INTEGER NOT NULL AUTO_INCREMENT,
    `group_id` VARCHAR(128) NOT NULL,  -- FK
    `path` VARCHAR(512) NOT NULL,
    `version` INTEGER NOT NULL,
    `status` INTEGER NOT NULL,
    `mtime` DATETIME,
    `footprint_id` INTEGER,            -- FK
    `digest` CHAR(64),                 -- cached from footprint
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`),
    UNIQUE (`group_id`, `path`, `version`)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE INDEX `histories_mtime_path_version` ON `histories` (`mtime`, `path`, `version`);
CREATE INDEX `histories_footprint_id_path_version` ON `histories` (`footprint_id`, `path`, `version`);
CREATE INDEX `histories_footprint_id_mtime` ON `histories` (`footprint_id`, `mtime`);
