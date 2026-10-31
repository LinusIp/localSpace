# 2026-10-31: switched features in Core

A feature still being built ships off, and is turned on per installation. **A switch never turns off a protection.**

- **In the proto:**
  - `Feature`: a fixed list, never a free string. For now it holds `themes` and `translation`; each feature's variant is added with its first code.
  - `Features { on, locked }`, reported in the environment.
  - `Request::SetFeature { feature, on }`, refused for a locked switch.
  - `Event::FeaturesChanged`.
- **Who may turn one:** on an organisation's server, only an administrator; on a person's own computer, that person.
- **What is on, in order:**
  1. the build's channel, read from `features.toml` when Core is compiled (`LOCALSPACE_CHANNEL`: `dev`, `preview` or `stable`; a build that names none is `stable`);
  2. what the installation has stored once a switch was turned (redb `settings`, `features`);
  3. in a development build only, `LOCALSPACE_FEATURES`, for example `+themes,-translation`.
- **Every channel is off for now.** Which features a release has on is decided per release.
- **`locked` is empty** until an organisation's policy exists.
- **Not yet:** running the tests and browser walks twice (every switch on, every switch off), and a test that every feature is checked somewhere. Both come with the first feature Core itself checks; `themes` and `translation` are the client's to show or hide.
- **Tests:**
  - the checked-in file names only known features, and every channel starts off;
  - the development override;
  - a switch turned is kept and reported;
  - an organisation's member is refused.
