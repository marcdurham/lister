# Assumptions

## Test Database (backend tests)

Backend API tests (`backend/src/api.rs`) require a reachable Postgres instance.
By default they connect to `postgres://marc@127.0.0.1:5433/lister_test`.

Override via env var `LISTER_TEST_DB`.

A standalone test cluster can be brought up with:
```bash
initdb -D /home/marc/pgtest -U marc --auth=trust --auth-host=trust
echo "port = 5433" >> /home/marc/pgtest/postgresql.conf
echo "unix_socket_directories = '/home/marc/pgtest'" >> /home/marc/pgtest/postgresql.conf
pg_ctl -D /home/marc/pgtest start
psql -h /home/marc/pgtest -p 5433 -U marc -d postgres -c "CREATE DATABASE lister_test;"
cat backend/migrations/*.sql | psql -h /home/marc/pgtest -p 5433 -U marc -d lister_test
```

`cargo test` must be invoked with `DATABASE_URL` set (required by sqlx `query!` macros at compile time):
```bash
DATABASE_URL=postgres://marc@127.0.0.1:5433/lister_test cargo test
```
