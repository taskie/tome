CREATE TABLE IF NOT EXISTS `footprints` (
    `id` INTEGER NOT NULL PRIMARY KEY AUTO_INCREMENT,
    `digest` CHAR(64) NOT NULL,
    `size` BIGINT NOT NULL,
    `fast_digest` BIGINT NOT NULL,
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`digest`)
)
DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
;

CREATE INDEX `ix_footprints_created_at` ON `footprints` (`created_at`);

