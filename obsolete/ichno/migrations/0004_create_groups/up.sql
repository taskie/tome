CREATE TABLE `groups` (
    `id` BIGINT NOT NULL PRIMARY KEY,
    `workspace_id` BIGINT NOT NULL,      -- FK
    `name` TEXT NOT NULL,
    `url` TEXT NOT NULL,
    `type` INTEGER NOT NULL,
    `description` TEXT NOT NULL,
    `status` INTEGER NOT NULL,
    `group_stat_id` BIGINT,              -- FK
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`workspace_id`, `id`),
    UNIQUE (`workspace_id`, `name`)
)
;
