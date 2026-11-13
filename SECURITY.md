# Security

SSD Workspace runs system commands (`diskutil`, `tmutil`, `lsof`, `herd`) on your behalf, so
security reports are taken seriously.

## Reporting a vulnerability

Please do **not** open a public issue. Use GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
on this repository instead, with steps to reproduce and the macOS version. You should get a
reply within a week.

## Scope

Of particular interest:

- a way to make the app run a command other than the ones listed in the README, or with
  arguments an attacker controls (for example through `config.json` or a volume name);
- a way for the Detail window's web view to do more than call the app's own commands;
- an eject or a stop of Time Machine or Herd without the user's confirmation.

Only the latest version is supported.
