# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| >= 1.0  | ✅ Active |
| < 1.0   | ⚠️ Dev-only (report anyway) |

## Reporting a Vulnerability

**DO NOT file a public GitHub issue for security vulnerabilities.**

Send details to **security@justapi.dev** with:

1. **Subject**: `[JustAPI Security] <brief description>`
2. **Affected version(s)**
3. **Type of vulnerability** (e.g., XSS, RCE, SQL injection, information disclosure)
4. **Steps to reproduce** (minimal proof of concept preferred)
5. **Impact assessment**
6. **Suggested fix** (optional but appreciated)

### Response timeline

- **24h**: Acknowledgment of receipt
- **7 days**: Triage and severity assessment
- **90 days**: Fix released (or extension negotiated)
- **Disclosure**: Coordinated public disclosure after fix is available

## Scope

In-scope:

- JustAPI framework source code (all crates)
- Official Docker images (`ghcr.io/justapi/*`)
- Official Helm chart
- Build pipeline and CI/CD configuration

Out-of-scope:

- Third-party plugins
- Applications built _with_ JustAPI (report to the app maintainer)
- Infrastructure not owned by the JustAPI project

## Bug Bounty

This project does not currently offer a paid bug bounty program.
Contributors will be credited in the release notes and the
[SECURITY.md](./hall-of-fame.md) hall of fame (opt-out available).

## Hall of Fame

We thank the following researchers for their responsible disclosures:

_None yet — be the first!_
