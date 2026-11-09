# Security policy

REX Harness runs AI models against your code and machine, so we take security
reports seriously. The security model, and what REX does not protect against, is
in [docs/security.md](docs/security.md).

## Supported versions

Security fixes go into the latest release. Until a 1.0, only the most recent
`0.x` release is supported.

## Reporting a vulnerability

**Please do not open a public issue for a vulnerability.**

Use GitHub's private vulnerability reporting: on the repository, open the
**Security** tab and choose **Report a vulnerability**. If that option is not
available, open an issue that says only that you have a security report and asks
for a private channel, with no technical details, and a maintainer will reach out.

Please include what you found, how to reproduce it, the version and platform, and
the impact you expect. Never include real keys or tokens in a report.

## What to expect

We aim to acknowledge a report within a few days, to tell you whether we consider
it a vulnerability, and to credit you in the fix if you wish. We ask for
reasonable time to fix a problem before it is disclosed publicly.

## In scope

- Anything that lets a model, a file's contents, or tool output bypass an
  approval, escape the workspace, or run a denied command.
- Leaking a stored credential or ChatGPT token: to the interface, a log, a
  receipt, run state, or another user.
- Flaws in the ChatGPT sign-in flow (the loopback listener, the token handling).
- Receipts that verify when they should not, or fail to when they should.
- Problems in the installer or release workflow that could ship a tampered build.

## Out of scope

- Malware already running as your user (it can read your files and ask the OS to
  decrypt what REX encrypted). This is described in the security model.
- A model provider's own behaviour or terms.
- Issues that need `--yes` plus a malicious task you chose to run.
- The absence of code signing on the Windows installer (known, and noted in the
  release notes).
