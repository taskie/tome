CREATE TABLE IF NOT EXISTS `contents` (
    `id` INTEGER NOT NULL PRIMARY KEY AUTO_INCREMENT,
    `footprint_id` INTEGER NOT NULL,  -- FK
    `body` BLOB NOT NULL,
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`footprint_id`)
)
DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
;

ALTER TABLE `contents` ADD CONSTRAINT `fk_contents_footprint_id` FOREIGN KEY (`footprint_id`) REFERENCES `footprints` (`id`);

CREATE INDEX `ix_contents_created_at` ON `contents` (`created_at`);
