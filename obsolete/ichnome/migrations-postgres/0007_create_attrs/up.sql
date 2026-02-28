CREATE TABLE "attrs" (
    "id" bigint NOT NULL PRIMARY KEY,
    "workspace_id" bigint NOT NULL,         -- FK
    "target_footprint_id" bigint NOT NULL,  -- FK
    "target_digest" bytea NOT NULL,         -- cached from footprint
    "key" text NOT NULL,
    "value_type" integer NOT NULL,
    "value_footprint_id" bigint NOT NULL,   -- FK
    "value_digest" bytea NOT NULL,          -- cached from footprint
    "value_text" text,
    "status" integer NOT NULL,
    "attr_stat_id" bigint,                  -- FK
    "created_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE ("workspace_id", "target_footprint_id", "key")
)
;

ALTER TABLE "attrs" ADD CONSTRAINT "fk_attrs_workspace_id" FOREIGN KEY ("workspace_id") REFERENCES "workspaces" ("id");
ALTER TABLE "attrs" ADD CONSTRAINT "fk_attrs_target_footprint_id" FOREIGN KEY ("target_footprint_id") REFERENCES "footprints" ("id");
ALTER TABLE "attrs" ADD CONSTRAINT "fk_attrs_value_footprint_id" FOREIGN KEY ("value_footprint_id") REFERENCES "footprints" ("id");
ALTER TABLE "attrs" ADD CONSTRAINT "fk_attrs_attr_stat_id" FOREIGN KEY ("attr_stat_id") REFERENCES "stats" ("id");

CREATE INDEX "ix_attrs_workspace_id_updated_at" ON "attrs" ("workspace_id", "updated_at");
CREATE INDEX "ix_attrs_workspace_id_value_footprint_id_target_footprint_id_key" ON "attrs" ("workspace_id", "value_footprint_id", "target_footprint_id", "key");
CREATE INDEX "ix_attrs_workspace_id_value_text_target_footprint_id_key" ON "attrs" ("workspace_id", "value_text", "target_footprint_id", "key");
