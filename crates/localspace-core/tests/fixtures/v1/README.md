# A v1 data directory, as the previous binary wrote it

`db/dag.redb` and `conversations.json` were produced on 2026-09-12 by the
server binary built from commit `5db9fcf` — the last commit before the
database gained a schema version (`9f3c184`) — not by the migration code.
They are the input of `tests/migration.rs`, which proves that the v1 → v3
migration carries a real file through intact and never writes to it.

How they were made, with that binary running on an empty data directory
(`--personal --registry harnesses --registry registry --data <dir>`):

1. `install_harness` from `harnesses/whiteboard` (1.2.0, which pulled in
   `io.localspace.types` 1.0.0): two commits on `environment_lock`.
2. `canvas.add_sticky` three times: "Design" (yellow), "Build" (green),
   "Launch" (blue): three commits on `io_localspace_whiteboard`, the
   board's document under its v1 name.
3. `send_message` "What is on the board?" with no model loaded: one
   conversation with two messages, the question and the honest reply.
4. Two exports through `POST /api/v1/artifacts`: a 1×1 PNG of 70 bytes as
   `image.v1` and a 111-byte SVG as `svg.v1`, named by the binary
   `board-fixture-c7b43d8.png` and `.svg` after the board's head commit;
   one commit each on their `blob:<blake3>` documents.

So the DAG holds 7 commits, the documents table 2 records (the board had
none in v1), and the file has no schema version. The exact bytes of the
two exports are in the test, so their hashes can be checked after the
migration.
