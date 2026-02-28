CREATE TABLE "histories" (
    "id" bigint NOT NULL PRIMARY KEY,
    "workspace_id" bigint NOT NULL,  -- cached from group, FK
    "group_id" bigint NOT NULL,      -- FK
    "path" text NOT NULL,
    "version" integer NOT NULL,
    "status" integer NOT NULL,
    "mtime" timestamptz,
    "footprint_id" bigint,           -- FK
    "digest" bytea,                  -- cached from footprint
    "created_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE ("group_id", "path", "version")
)
;

ALTER TABLE "histories" ADD CONSTRAINT "fk_histories_workspace_id_group_id" FOREIGN KEY ("workspace_id", "group_id") REFERENCES "groups" ("workspace_id", "id");
ALTER TABLE "histories" ADD CONSTRAINT "fk_histories_footprint_id" FOREIGN KEY ("footprint_id") REFERENCES "footprints" ("id");

CREATE INDEX "ix_histories_workspace_id_updated_at" ON "histories" ("workspace_id", "updated_at");
CREATE INDEX "ix_histories_workspace_id_footprint_id_path_version" ON "histories" ("workspace_id", "footprint_id", "path", "version");
CREATE INDEX "ix_histories_workspace_id_footprint_id_mtime" ON "histories" ("workspace_id", "footprint_id", "mtime");
