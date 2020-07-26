CREATE TABLE IF NOT EXISTS `attrs` (
    `id` INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    `workspace_id` INTEGER NOT NULL,         -- FK
    `target_footprint_id` INTEGER NOT NULL,  -- FK
    `target_digest` CHAR(64) NOT NULL,       -- cached from footprint
    `key` VARCHAR(128) NOT NULL,
    `value_footprint_id` INTEGER NOT NULL,   -- FK
    `value_digest` CHAR(64) NOT NULL,        -- cached from footprint
    `value_content_type` INTEGER NOT NULL,
    `value_summary` VARCHAR(512),
    `status` INTEGER NOT NULL,
    `attr_stat_id` INTEGER,                  -- FK
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`workspace_id`, `target_footprint_id`, `key`)
)
-- DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
;

-- ALTER TABLE `attrs` ADD FOREIGN KEY `fk_attrs_workspace_id` (`workspace_id`) REFERENCES `workspaces` (`id`);
-- ALTER TABLE `attrs` ADD FOREIGN KEY `fk_attrs_target_footprint_id` (`target_footprint_id`) REFERENCES `footprints` (`id`);
-- ALTER TABLE `attrs` ADD FOREIGN KEY `fk_attrs_value_footprint_id` (`value_footprint_id`) REFERENCES `footprints` (`id`);
-- ALTER TABLE `attrs` ADD FOREIGN KEY `fk_attrs_value_content_id` (`value_content_id`) REFERENCES `contents` (`id`);

CREATE INDEX `ix_attrs_workspace_id_updated_at` ON `attrs` (`workspace_id`, `updated_at`);
CREATE INDEX `ix_attrs_workspace_id_value_footprint_id_target_footprint_id_key` ON `attrs` (`workspace_id`, `value_footprint_id`, `target_footprint_id`, `key`);
CREATE INDEX `ix_attrs_workspace_id_value_summary_target_footprint_id_key` ON `attrs` (`workspace_id`, `value_summary`, `target_footprint_id`, `key`);
