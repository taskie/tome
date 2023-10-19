CREATE TABLE "footprints" (
    "id" bigint NOT NULL PRIMARY KEY,
    "digest" bytea NOT NULL,
    "size" bigint NOT NULL,
    "fast_digest" bigint NOT NULL,
    "created_at" timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE ("digest")
)
;

CREATE INDEX "ix_footprints_created_at" ON "footprints" ("created_at");
