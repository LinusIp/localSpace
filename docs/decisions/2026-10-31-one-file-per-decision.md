# 2026-10-31: one file per decision

Several branches adding entries to the top of `docs/DECISIONS.md` kept colliding when merged.

- **`docs/DECISIONS.md` is frozen.** Its entries stay where they are, and older code comments that cite it stay valid.
- **Every new decision goes in its own file,** `docs/decisions/YYYY-MM-DD-topic.md`. Two branches never edit the same file.
- **The wording stays neutral,** as in the log before.
- **A code comment** that cites a new decision names its file.
