CREATE TABLE IF NOT EXISTS `groups` (
    `id` INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    `workspace_id` INTEGER NOT NULL,      -- FK
    `name` VARCHAR(128) NOT NULL,
    `url` VARCHAR(512) NOT NULL,
    `type` INTEGER NOT NULL,
    `description` VARCHAR(512) NOT NULL,
    `status` INTEGER NOT NULL,
    `group_stat_id` INTEGER,              -- FK
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`workspace_id`, `id`),
    UNIQUE (`workspace_id`, `name`)
)
-- DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
;

-- ALTER TABLE `groups` ADD CONSTRAINT FOREIGN KEY `fk_groups_workspace_id` (`workspace_id`) REFERENCES `workspaces` (`id`);
