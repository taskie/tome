CREATE TABLE IF NOT EXISTS `histories` (
    `id` INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    `group_id` VARCHAR(128) NOT NULL,  -- FK
    `path` VARCHAR(512) NOT NULL,
    `version` INTEGER NOT NULL,
    `status` INTEGER NOT NULL,
    `mtime` DATETIME,
    `footprint_id` INTEGER,            -- FK
    `digest` CHAR(64),                 -- cached from footprint
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`group_id`, `path`, `version`)
);

CREATE INDEX `histories_mtime_path_version` ON `histories` (`mtime`, `path`, `version`);
CREATE INDEX `histories_footprint_id_path_version` ON `histories` (`footprint_id`, `path`, `version`);
CREATE INDEX `histories_footprint_id_mtime` ON `histories` (`footprint_id`, `mtime`);
