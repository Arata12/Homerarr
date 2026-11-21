-- Integrations saved by non-admins use the SSRF-guarded HTTP client.
ALTER TABLE "integration" ADD COLUMN IF NOT EXISTS "restricted" boolean NOT NULL DEFAULT false;
