# Security Policy

## Supported Versions

vMACS is actively developed, and security fixes are primarily focused on the latest release.

| Version | Security Support |
|---|---|
| Latest release | Supported |
| Older releases | Not guaranteed |

Users are encouraged to update to the latest release to receive available bug fixes and security improvements.

## Reporting a Vulnerability

If you discover a potential security vulnerability in vMACS, please report it responsibly.

**Please do not disclose exploitable vulnerabilities publicly before they can be investigated.**

Use GitHub's **Private Vulnerability Reporting** feature, if enabled for this repository, to submit a report privately.

If private vulnerability reporting is unavailable, contact the repository maintainer through a private communication channel listed on the maintainer's GitHub profile, if one is available.

Please do not include sensitive information in public issues or discussions.

## What to Include

When reporting a vulnerability, please include as much of the following information as possible:

- A description of the vulnerability and its potential impact.
- The affected vMACS version or commit.
- The operating system and terminal emulator, if relevant.
- Steps to reproduce the issue.
- A proof of concept or minimal example, if available.
- Any suggested mitigations or fixes.

Reports do not need to include every item above to be considered.

## Areas of Security Interest

The following areas are particularly relevant to vMACS:

- **File integrity and data loss:** Unexpected modification, corruption, or deletion of user files.
- **File handling:** Unsafe path handling or unintended access to files.
- **Configuration parsing:** Vulnerabilities triggered by malformed or specially crafted configuration files.
- **Custom language definitions:** Unexpected behavior or resource exhaustion caused by crafted language configurations.
- **Custom commands:** Unintended command execution or other security issues involving user-defined commands.
- **Terminal interaction:** Terminal escape-sequence injection or unintended terminal behavior caused by file contents.
- **Resource exhaustion:** Excessive memory or CPU consumption caused by crafted input or configuration data.
- **Dependency vulnerabilities:** Security issues originating in third-party crates used by vMACS.

This list is not exhaustive. Other security issues are welcome.

## Response and Disclosure

Reports will be reviewed as time and availability permit. Please understand that vMACS is an independently developed project, and response or remediation timelines cannot be guaranteed.

If a vulnerability is confirmed, the maintainer will assess its severity and determine appropriate mitigation or remediation.

When practical, security fixes will be included in a subsequent release, and relevant users will be encouraged to update.

Please coordinate public disclosure with the maintainer to allow reasonable time for investigation and remediation.

## Scope and Limitations

This policy covers security vulnerabilities in vMACS itself and its maintained source code.

It does not guarantee security support for:

- Unofficial builds or modified versions.
- Unsupported operating systems or terminal environments.
- Vulnerabilities originating exclusively in external software or dependencies, although their impact on vMACS may still be relevant.
- Issues caused by intentional changes to the source code or configuration that disable existing safeguards.

Please report potentially relevant issues even if you are unsure whether they fall within this scope.

Thank you for helping keep vMACS and its users safe.
