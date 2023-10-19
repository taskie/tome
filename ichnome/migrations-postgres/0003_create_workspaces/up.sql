CREATE TABLE "workspaces" (
    "id" bigint NOT NULL PRIMARY KEY,
    "name" text NOT NULL,
    "description" text NOT NULL,
    "status" integer NOT NULL,
    "created_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE ("name")
)
;
