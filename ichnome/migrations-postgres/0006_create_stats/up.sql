CREATE TABLE "stats" (
    "id" bigint NOT NULL PRIMARY KEY,
    "workspace_id" bigint NOT NULL,  -- cached from group, FK
    "group_id" bigint NOT NULL,      -- FK
    "path" text NOT NULL,
    "history_id" bigint NOT NULL,    -- FK
    "version" integer NOT NULL,      -- cached from history
    "status" integer NOT NULL,       -- cached from history
    "mtime" timestamptz,             -- cached from history
    "footprint_id" bigint,           -- cached from history, FK
    "digest" bytea,                  -- cached from footprint
    "size" bigint,                   -- cached from footprint
    "fast_digest" bigint,            -- cached from footprint
    "created_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE ("group_id", "path")
)
;

ALTER TABLE "stats" ADD CONSTRAINT "fk_stats_workspace_id_group_id" FOREIGN KEY ("workspace_id", "group_id") REFERENCES "groups"("workspace_id", "id");
ALTER TABLE "stats" ADD CONSTRAINT "fk_stats_history_id" FOREIGN KEY ("history_id") REFERENCES "histories"("id");
ALTER TABLE "stats" ADD CONSTRAINT "fk_stats_footprint_id" FOREIGN KEY ("footprint_id") REFERENCES "footprints" ("id");

ALTER TABLE "groups" ADD CONSTRAINT "fk_groups_group_stat_id" FOREIGN KEY ("group_stat_id") REFERENCES "stats" ("id");

CREATE INDEX "ix_stats_workspace_id_updated_at" ON "stats" ("workspace_id", "updated_at");
CREATE INDEX "ix_stats_workspace_id_footprint_id_path_version" ON "stats" ("workspace_id", "footprint_id", "path", "version");
CREATE INDEX "ix_stats_workspace_id_footprint_id_mtime" ON "stats" ("workspace_id", "footprint_id", "mtime");
