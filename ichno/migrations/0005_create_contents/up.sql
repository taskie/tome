CREATE TABLE IF NOT EXISTS `contents` (
    `footprint_id` INTEGER NOT NULL,  -- FK
    `digest` CHAR(64) NOT NULL,       -- cached from footprint
    `body` BLOB NOT NULL,
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`footprint_id`),
    UNIQUE (`digest`)
);

CREATE INDEX `contents_updated_at` ON `contents` (`updated_at`, `footprint_id`);
