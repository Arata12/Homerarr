# Migrating from Homarr v1/v2

Homerarr v3 can take over the SQLite database of upstream Homarr v1/v2 (the TypeScript version). You change the
image, keep your volume and encryption key, and the database is converted on the first start.

PostgreSQL databases from upstream Homarr are not migrated yet: v3 refuses to start on them. MySQL/MariaDB is not
supported by v3 at all.

## Before you start

1. **Back up the whole `/appdata` volume** (or your bind-mounted folder), for example:
   ```bash
   docker compose stop homarr
   docker run --rm -v homarr_appdata:/appdata -v "$PWD":/backup busybox tar czf /backup/homarr-appdata.tgz /appdata
   ```
2. **Write down your `SECRET_ENCRYPTION_KEY`.** It is mandatory and must be the exact same value: it decrypts the
   integration credentials (API keys, passwords) stored in the database. With a different key every integration has
   to be set up again.

## Switch the container

Edit your `docker-compose.yml`:

```yaml
services:
  homarr:
    image: ghcr.io/arata12/homerarr:latest   # was ghcr.io/homarr-labs/homarr
    restart: unless-stopped
    ports:
      - "7575:7575"
    volumes:
      - homarr_appdata:/appdata              # keep the same volume
      - /var/run/docker.sock:/var/run/docker.sock:ro
    group_add:
      - "988"                                # GID of the docker socket: stat -c %g /var/run/docker.sock
    environment:
      SECRET_ENCRYPTION_KEY: "<same 64-hex value as before>"
      REDIS_DISABLED: "true"                 # optional: run without Redis

volumes:
  homarr_appdata:
```

Upstream Homarr ran as root; the v3 image runs as user `65532`. Give it the volume once:

```bash
docker run --rm -v homarr_appdata:/appdata busybox chown -R 65532:65532 /appdata
```

(For a bind mount: `sudo chown -R 65532:65532 ./appdata`.) Drop `group_add` if you do not mount the Docker socket.
Redis is optional; without it v3 uses an in-memory cache.

Then start it: `docker compose up -d && docker compose logs -f homarr`.

## What happens on the first start

- v3 sees the upstream database at `/appdata/db/db.sqlite` and logs that it is migrating it.
- A copy of the original is saved next to it as `db.sqlite.v2-backup-<timestamp>`.
- A new v3 database is built from your data (users, groups, boards, apps, integrations, settings, ...), checked, and
  then swapped in. If anything fails, the original file is not touched and the container exits with an error.
- This happens only once. Set `LEGACY_DB_AUTO_MIGRATE=false` if you would rather start fresh with a new `DB_URL`.

You can also import manually into a new, empty database:
`homarr db import-legacy /path/to/old/db.sqlite` (with `DB_URL` pointing at the new database).

## What changes

- **Everyone logs in again.** Sessions are not carried over; pending invite links stop working.
- **Custom CSS must be rewritten.** The page no longer uses Mantine class names, so old selectors do not match.
- **No OIDC or LDAP login yet.** Only username/password (and a trusted reverse-proxy header) work.
- **Widget text is in English**, whatever language was set before.

## Rollback

1. Stop v3: `docker compose stop homarr`.
2. Put the backup back:
   ```bash
   docker run --rm -v homarr_appdata:/appdata busybox sh -c \
     'cd /appdata/db && rm -f db.sqlite-wal db.sqlite-shm && cp db.sqlite.v2-backup-<timestamp> db.sqlite'
   ```
3. Change the image back to your previous upstream Homarr image and start it.

Changes made while running v3 are lost on rollback. Restoring the volume backup from the first step works too.
