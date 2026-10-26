# Security

localSpace runs AI models on your own computer. If you find a way to make it leak data, run code it shouldn't, or get round its permissions, please tell us privately first.

## How to report

- **Use GitHub's private reporting:** this repository's **Security** tab, then **Report a vulnerability**. Only the maintainers see it.
- **Please don't open a public issue** or post about it before a fix is out.
- **Include, if you can:**
  - the version (Settings → About);
  - what you did and what happened;
  - what an attacker gains;
  - the smallest example that shows it.

## What happens next

- **We confirm we've received it** within 5 working days, Tashkent time (UTC+5). localSpace is a small team, so this may be slower over holidays. We'll say so if it is.
- **We work out how serious it is, and tell you** within 10 working days of that confirmation.
- **Fix times:**

  | Severity | Our aim for a fixed release |
  |---|---|
  | Critical (data leaving the computer, code running outside the sandbox) | as soon as we can |
  | High | 30 days |
  | Medium and low | the next planned release |
- **We publish an advisory when the fix is released:**
  - where: GitHub Security Advisories for this repository, and the release notes;
  - what it says: what was wrong, which versions are affected, and what to do;
  - credit: we name you if you'd like.
- **We aim to publish within 90 days** of your report. We'll agree on another date with you if a fix takes longer.

There is no bug bounty.

## What is covered

- **Covered:**
  - the localSpace app: Core, the desktop window and the web client;
  - its installer and portable package.
- **Third-party parts:** the app ships the llama.cpp engine. A problem in llama.cpp itself goes to its project ([ggml-org/llama.cpp](https://github.com/ggml-org/llama.cpp/security)). We take its fixes into a new release.
- **Not covered:** models you download are third-party files. Behaviour of a model (what it says) is not a security issue for this policy, unless the app lets a model do something it shouldn't, such as reach the network or read files without your approval.

## Supported versions

Only the newest release and the newest preview get security fixes. Please update before reporting, if you can.
