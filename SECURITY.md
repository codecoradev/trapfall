# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| v0.2.x (latest release) | ✅ |
| Previous release line | ✅ (critical only) |
| Older versions | ❌ |

## Reporting a Vulnerability

If you discover a security vulnerability in TrapFall, please report it responsibly.

**Do NOT** open a public issue for security vulnerabilities.

### How to Report

Use [GitHub Private Vulnerability Reporting](https://github.com/codecoradev/trapfall/security/advisories/new). This keeps the report confidential and visible only to maintainers.

Include as much detail as possible:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### What to Expect

- **Acknowledgment** within 48 hours
- **Initial assessment** within 5 business days
- **Fix timeline** depends on severity:
  - Critical: 7 days
  - High: 14 days
  - Medium: 30 days
  - Low: next minor release

### Security in the Development Process

TrapFall runs automated security checks on every PR and weekly (Monday 06:00 UTC):

- **Cargo Audit** — Rust dependency advisories scan on the workspace
- **Trivy Secrets** — scans for committed credentials and keys
- **Trivy Filesystem** — misconfiguration and vulnerability scan
- **npm Audit** — frontend (`web/`) dependency advisories, tracked on PRs and the weekly schedule

### Deployment Surface Notes

The production Docker image is built `FROM scratch` with a statically linked MUSL binary and rustls (no OpenSSL). There is no shell, no package manager, and no OS layer inside the container, which keeps the runtime attack surface minimal. Report anything that contradicts that expectation.
