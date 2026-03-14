# Security Policy

## Supported Versions

We support the latest published release of Zenvoy. Security fixes land on
`main` first and roll into the next release.

## Reporting a Vulnerability

**Please do not open a public issue for security vulnerabilities.**

Report issues privately. Include:

- A description of the issue and the impact
- Steps to reproduce or a proof of concept
- Affected versions, if known
- Any suggested remediation

We aim to acknowledge reports within 3 business days and to ship a fix or
publish a coordinated advisory within 30 days of confirmation.

## Security Model

The Zenvoy server (Axum) includes:

- Session-based authentication with rate-limited login
- Security headers middleware (CSP, X-Frame-Options, etc.)
- CORS policy enforcement
- Path traversal prevention on all vault operations
- No arbitrary shell execution
- Auth token required for non-loopback bindings

For self-hosted deployments:

- Keep Zenvoy behind a reverse proxy, VPN, or private network
- Do not expose the server directly to the public internet without TLS
- Docker containers run with read-only root filesystem and dropped capabilities
