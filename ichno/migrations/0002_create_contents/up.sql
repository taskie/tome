CREATE TABLE IF NOT EXISTS `contents` (
    `id` INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    `footprint_id` INTEGER NOT NULL,  -- FK
    `body` BLOB NOT NULL,
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`footprint_id`)
)
-- DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
;

-- ALTER TABLE `contents` ADD CONSTRAINT FOREIGN KEY `fk_contents_footprint_id` (`footprint_id`) REFERENCES `footprints` (`id`);

CREATE INDEX `ix_contents_created_at` ON `contents` (`created_at`);
