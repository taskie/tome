CREATE TABLE IF NOT EXISTS `groups` (
    `id` VARCHAR(128) NOT NULL,
    `url` VARCHAR(512) NOT NULL,
    `type` INTEGER NOT NULL,
    `history_id` INTEGER,           -- FK,
    `version` INTEGER,              -- cached from history
    `status` INTEGER,               -- cached from history
    `mtime` DATETIME,               -- cached from history
    `footprint_id` INTEGER,         -- cached from history, FK
    `digest` CHAR(64),              -- cached from footprint
    `size` BIGINT,                  -- cached from footprint
    `fast_digest` BIGINT,           -- cached from footprint
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`),
    UNIQUE (`digest`)
);

CREATE INDEX `groups_url_id` ON `groups` (`url`, `id`);
CREATE INDEX `groups_status_id` ON `groups` (`status`, `id`);
CREATE INDEX `groups_updated_at_id` ON `groups` (`updated_at`, `id`);
