CREATE TABLE `workspaces` (
    `id` BIGINT NOT NULL PRIMARY KEY,
    `name` TEXT NOT NULL,
    `description` TEXT NOT NULL,
    `status` INTEGER NOT NULL,
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`name`)
)
;
