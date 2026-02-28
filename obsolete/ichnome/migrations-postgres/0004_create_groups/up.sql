CREATE TABLE "groups" (
    "id" bigint NOT NULL PRIMARY KEY,
    "workspace_id" bigint NOT NULL,      -- FK
    "name" text NOT NULL,
    "url" text NOT NULL,
    "type" integer NOT NULL,
    "description" text NOT NULL,
    "status" integer NOT NULL,
    "group_stat_id" bigint,              -- FK
    "created_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE ("workspace_id", "id"),
    UNIQUE ("workspace_id", "name")
)
;

ALTER TABLE "groups" ADD CONSTRAINT "fk_groups_workspace_id" FOREIGN KEY ("workspace_id") REFERENCES "workspaces" ("id");
