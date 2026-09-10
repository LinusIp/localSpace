# localSpace — Marketplace and Customisation

Companion to the Harness Plugin System and Organisation Deployment specs. Covers how individuals and organisations customise localSpace by purchasing and composing tools and environments from the marketplace: what can be customised, what is sold, how purchases work offline, how the org catalog fits, and how localSpace ships harnesses.

**All harnesses are built by localSpace.** The marketplace is a first-party catalog delivered like a package manager (plugin spec §17), not an open store. There are no third-party publishers, no revenue share, no untrusted uploads. Organisations that need something custom commission it from localSpace and receive it as a private listing. Everything below follows from that.

---

## 1. Principles

- **Core is empty; the catalog is the product.** A fresh install is Core, the client shell, and exactly one pre-installed harness: **chat** (plus the store and settings panels needed to install more). The whiteboard, physics simulation, CAD/sketching, planning board and every other tool are packages downloaded from the catalog on demand — never bundled with the installer. The base download stays small, an organisation's employees start with a chat window, and everything else appears only when the user or the org catalog adds it.
- **An environment is the unit of customisation.** A physics researcher, an architect and a planner do not configure features; they install environments — and an environment template is a listing like any other.
- **Purchases are entitlements, verified offline.** No component needs to be online to know what a user or organisation is allowed to run.
- **Organisations decide what their people see.** The catalog is the supply; the org catalog is the shelf.
- **One author, one quality bar.** Because localSpace writes every harness, every listing meets the same budgets, uses the same interchange types, ships the same evals, and is tested against every hardware profile before release. The sandbox (plugin spec §6) stays — for fault isolation and memory budgets, not because the code is untrusted.

---

## 2. What can be customised

| Layer | What the user or org changes | Where it lives |
|---|---|---|
| Harness set | which tools exist in an environment | environment |
| Model | which resident models and placement reservations | environment (org: worker assignment) |
| Layout | panel arrangement, docking, default views per harness | environment; per-user overrides |
| Theme | colours, type scale, density, icon set — all harnesses inherit | environment; org can lock to its brand |
| Keymap | shortcuts, command palette entries | per-user |
| Agent policy | system prompt additions, persona, confirm levels, tool exposure defaults, network mode | environment (org: ceiling) |
| Prompts and skills | reusable instructions the agent loads on demand, per domain | environment |
| Connectors | which company sources are indexed | workspace |
| Branding | logo, product name in the title bar, login page | org |

Every layer is a document in the store with a DAG history, so a customisation can be undone, diffed, exported and shared inside the organisation.

### 2.1 Environment templates

An environment saved as a template captures: the harness list with pinned versions (a lockfile, plugin spec §17.2), model requirements expressed as a profile, layout, theme, keymap, agent policy, prompts, and optionally seed documents (a starter board, example scenes). Installing a template creates a new environment, resolves each harness through the catalog (installing, purchasing or prompting as needed), and applies the rest. localSpace ships the field templates — "Physics Lab", "Architecture Studio", "Product Planning", "Legal Research" — and users and org admins save their own for reuse inside the org.

---

## 3. The catalog

### 3.1 Listing types

| Type | Package | Sold as |
|---|---|---|
| Harness | `.hpack` (plugin spec §12) | included in a tier, one-time, subscription, per-seat |
| Environment template | `.lsenv` — template document + lockfile | bundle price (below the sum of its harnesses) |
| Model pack | a curated model with placement plans pre-computed for W32/W96/S, licence text, draft-model pairing, eval results | free (open weights) — the value is the tested placement |
| Interchange types and libraries | `kind = "types"` / `"library"` packages | free; installed automatically as dependencies |
| Skill | agent instructions for a domain task, optional tools | free or one-time |
| Theme / keymap / prompt pack | small documents | free |

### 3.2 Listing metadata

Id, version and channel (`stable`, `beta`), title, description, screenshots rendered by the store harness from the actual surface, capabilities in plain language, tier and `native_reason`, `[resources]` memory declaration, supported hardware profiles, `harness-api` range, dependencies and provided interfaces, price and model, and **eval score per reference model and profile**.

### 3.3 Compatibility

A listing declares `profiles = ["W32", "W96", "S"]` and the store filters by the machine it is running on. A template declares its model needs as a profile requirement and the planner resolves it against what is installed; the store shows "runs on this machine: yes / hybrid / no" before purchase, the same verdict as the model catalog.

### 3.4 Feedback

Ratings and reviews from verified installs, tagged with hardware profile and model, go to localSpace's product team and are visible in the store. A "report a problem" button attaches the harness version, plan, and (with consent) the last agent trace, and opens a support case; there is no takedown process because there is no one to take down.

---

## 4. Commerce

### 4.1 Buyers

- **Individuals**: a localSpace account, card via the merchant of record. Purchases are bound to the account; the account can be signed in on up to three machines.
- **Organisations**: the admin buys in the console with card, or by purchase order and invoice for volume. Purchases are bound to the organisation and assigned to seats or to all seats. Volume tiers per listing: 1–10, 11–100, 101–1000, 1000+.
- **Resellers and partners** buy organisation licences on behalf of a customer; the entitlement names the end organisation.

### 4.2 Pricing models

- **Tiers**: Personal, Team (W32 team mode) and Enterprise licences each include a base set of harnesses; the catalog marks what is included in the buyer's tier.
- **Per listing**: one-time (perpetual for the purchased major version, updates within it included); subscription (updates included, converts to read-only-use on lapse — the harness keeps working, stops updating, never bricks a document); per-seat for organisations (subscription; seats counted as users with the harness in an active environment in the last 30 days).
- **Trials**: 14 days, one per listing per account, full function.
- **Commissioned harnesses**: fixed-price or time-and-materials development, delivered as a private listing in the org's namespace with an exclusivity period the contract sets, after which localSpace may generalise it into the public catalog.

No metered pricing. The product is offline; there is nothing to meter and nothing to phone home about.

### 4.3 Refunds

Fourteen days for one-time purchases, prorated for subscriptions.

---

## 5. Entitlements — purchases that work offline

An entitlement is a signed, self-contained token: `{subject: account | org, listing id, version range, model (one-time | subscription), seats, issued, expires, features}` signed by the registry key, with the organisation or account public key inside so the token cannot be moved.

- **Verification is local.** Core checks the signature and expiry against its clock; no call is made. Subscriptions carry a 30-day expiry and are refreshed opportunistically when the machine happens to be online, or by importing a renewed entitlement file when it is not.
- **Air-gapped organisations** receive entitlements inside the offline bundle (deployment spec §3.1) alongside packages; renewals are a new bundle. Nothing about an air-gapped site's usage leaves it.
- **Seats** are counted by Core, shown in the console, and reported nowhere; an org over its seat count is warned for 30 days, then new users cannot activate that listing until seats are added.
- **Revocation** (fraud, chargeback) is a signed revocation list shipped with every registry index and every bundle. Revoked entitlements stop the listing from starting, never delete anything.
- **Commissioned private listings** are entitled to the one org and published only to that org's namespace; they are in the org's offline bundle like anything else.

---

## 6. The organisation catalog

The org catalog is the only thing employees see. Admins:

- **Curate**: approve listings and versions from the localSpace catalog (deployment spec §8.1), assign purchased seats, set which templates appear on an employee's first login, and pin a default environment per department. Approval here is the org's change-control step — deciding when a new version reaches employees — not a security review of the code.
- **Commission**: request custom harnesses, templates or connectors from localSpace with a brief; delivery lands as a private listing in `corp.example/*`, versioned, entitled and updated through the same channel as everything else.
- **Mirror**: the org registry mirrors approved listings so employees install from the intranet at LAN speed and the public registry sees one download per version, not one per employee.
- **Templates and policy**: save org-wide templates, lock the theme to the brand, set agent-policy and network ceilings per department.

---

## 7. In-app experience

- **Store** is a harness with a `widgets` surface: browse by field (Physics, Architecture, Design, Planning, Research, Legal, Software, …), by type, by "runs on this machine", by "included in my tier". A listing page shows the live-rendered surface, the eval score for the model the user actually has, memory declaration, capabilities in plain language, and the price for the buyer type.
- **One click** installs into the current environment (or creates one from a template), resolves dependencies, prompts for capability grants, and shows the planner's re-plan if the harness reserves GPU or memory.
- **Environment switcher** in the title bar; environments are cheap (a document), so a user keeps several — one per project or field.
- **Customise panel** exposes every layer in §2 for the current environment, with "save as template" and "share with org" at the bottom.
- **Updates** are a badge, never automatic; the diff of capabilities and resources is shown before applying; rollback to the previous version is one click because the old package is retained.

---

## 8. How localSpace ships harnesses

The harness catalog is a monorepo built and released by localSpace on the same cadence as Core, using the SDK any harness needs:

```
localspace dev new harness io.localspace.cfd    # scaffold: manifest, logic crate, surface crate, evals.json
localspace dev run                              # hot-reload against a local Core; shows exactly what the model sees, including the task ledger
localspace dev eval --profile W32 --model <id>  # run evals.json the way the store will score it
localspace dev check                            # lint: manifest, tool budgets, memory declaration, capabilities, types
localspace dev release --channel beta           # sign with the localSpace key, push to the registry
```

Release gates in CI for every harness: manifest and tool-description lint, memory budget held under the eval run, sandbox tests, eval score on each reference model per profile, the three-harness handoff bench (plugin spec §18) for any harness that `accepts` or `produces` a type, reproducible build, signature. Native (Tier B) harnesses additionally pass the sandbox profile tests on each OS. `beta` channel first, `stable` after one cycle without a P1.

Internal standards every harness follows, because the catalog reads as one product: the interchange types in §18.3 for anything it exchanges, the host theme (no custom colours), the shared command-palette and keymap conventions, one context provider that produces artifact summaries the ledger can reuse, and a specialist skill where the harness has more than ~10 tools.

---

## 9. Build order

1. Entitlement format, signing, local verification, revocation list — before the store UI, because org offline bundles depend on it.
2. Registry service: index, packages, channels, signing; offline bundle export/import including entitlements; org mirror.
3. Store harness (`widgets` surface) with install, dependency resolution, update, rollback; environment templates (`.lsenv`).
4. Org catalog: approval, seat assignment, private namespace for commissioned listings, templates and policy.
5. Commerce: merchant of record integration, individual and org purchase flows, PO/invoice, tiers.
6. Internal release pipeline and SDK hardening (`dev run` with the ledger view, per-profile evals).
7. Commission workflow in the console.

Steps 1–4 ship with the product; an organisation can run fully offline with a bundled catalog before any payment flow exists.
