# The localSpace plugin SDK

**This folder is licensed under the Apache License, Version 2.0** (`LICENSE`, `NOTICE`). The rest of this repository is not yet. Its licence is in the root `LICENSE`.

Here is what developers need to build plugins for localSpace (internal plan 47):
- the plugin manifest, `plugin.toml`, and its schema; plugins are packed as `.lspkg` files;
- the tool contract: a name, a description, an input schema and permissions;
- the `dev` commands' templates;
- the two example plugins: a unit converter (WebAssembly) and a CSV/spreadsheet reader (read-only);
- the developer docs, in English first.

The contents arrive with the plugin kit's PRs.

**The rule for this folder:** nothing in it comes from outside it.
- No path leaves the folder.
- No dependency on the app's crates (`localspace-core`, `localspace-proto`, `localspace-server` and the others) or its web packages. They still hold the organisation-server code, which stays closed.
- `scripts/check-sdk-boundary.mjs` checks this in CI.

What a plugin needs from the app comes through the published contract, never from the app's code.
