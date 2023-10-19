CREATE TABLE `contents` (
    `id` BIGINT NOT NULL PRIMARY KEY,
    `footprint_id` BIGINT NOT NULL,  -- FK
    `body` BLOB NOT NULL,
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`footprint_id`)
)
;

CREATE INDEX `ix_contents_created_at` ON `contents` (`created_at`);
