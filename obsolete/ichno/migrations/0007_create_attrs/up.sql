CREATE TABLE `attrs` (
    `id` BIGINT NOT NULL PRIMARY KEY,
    `workspace_id` BIGINT NOT NULL,         -- FK
    `target_footprint_id` BIGINT NOT NULL,  -- FK
    `target_digest` BLOB NOT NULL,          -- cached from footprint
    `key` TEXT NOT NULL,
    `value_type` BIGINT NOT NULL,
    `value_footprint_id` BIGINT NOT NULL,   -- FK
    `value_digest` BLOB NOT NULL,           -- cached from footprint
    `value_text` TEXT,
    `status` BIGINT NOT NULL,
    `attr_stat_id` BIGINT,                  -- FK
    `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (`workspace_id`, `target_footprint_id`, `key`)
)
;

CREATE INDEX `ix_attrs_workspace_id_updated_at` ON `attrs` (`workspace_id`, `updated_at`);
CREATE INDEX `ix_attrs_workspace_id_value_footprint_id_target_footprint_id_key` ON `attrs` (`workspace_id`, `value_footprint_id`, `target_footprint_id`, `key`);
CREATE INDEX `ix_attrs_workspace_id_value_text_target_footprint_id_key` ON `attrs` (`workspace_id`, `value_text`, `target_footprint_id`, `key`);
