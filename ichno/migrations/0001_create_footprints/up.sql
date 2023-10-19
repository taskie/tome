CREATE TABLE `footprints` (
    `id` BIGINT NOT NULL PRIMARY KEY,
    `digest` BLOB NOT NULL,
    `size` BIGINT NOT NULL,
    `fast_digest` BIGINT NOT NULL,
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`digest`)
)
;

CREATE INDEX `ix_footprints_created_at` ON `footprints` (`created_at`);
