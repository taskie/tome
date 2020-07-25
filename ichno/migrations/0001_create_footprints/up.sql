CREATE TABLE IF NOT EXISTS `footprints` (
    `id` INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    `digest` CHAR(64) NOT NULL,
    `size` BIGINT NOT NULL,
    `fast_digest` BIGINT NOT NULL,
    `git_object_id` CHAR(40) NOT NULL,
    UNIQUE (`digest`)
);

CREATE INDEX `footprints_git_object_id` ON `footprints` (`git_object_id`);
