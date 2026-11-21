-- Integrations saved by non-admins use the SSRF-guarded HTTP client.
ALTER TABLE "integration" ADD COLUMN "restricted" integer NOT NULL DEFAULT 0;
