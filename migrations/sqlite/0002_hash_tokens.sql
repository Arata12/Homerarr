-- Session and invite tokens are now stored as SHA-256 hex digests; existing plaintext rows are invalidated.
DELETE FROM "session";
DELETE FROM "invite";
