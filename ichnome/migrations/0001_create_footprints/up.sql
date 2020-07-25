CREATE TABLE IF NOT EXISTS `footprints` (
    `id` INTEGER NOT NULL AUTO_INCREMENT,
    `digest` CHAR(64) NOT NULL,
    `size` BIGINT NOT NULL,
    `fast_digest` BIGINT NOT NULL,
    `git_object_id` CHAR(40) NOT NULL,
    PRIMARY KEY (`id`),
    UNIQUE (`digest`)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE INDEX `footprints_git_object_id` ON `footprints` (`git_object_id`);
