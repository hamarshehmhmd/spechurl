# Security Policy

## Supported versions

| Version | Supported |
| ------- | --------- |
| 0.1.x   | Yes       |

## Reporting a vulnerability

spechurl is a local command-line tool. It reads an OpenAPI document you point it at and writes Hurl files. It does not phone home, and it does not send your spec anywhere.

If you believe you have found a security issue — for example a parser crash that could disrupt CI, or a case where generated Hurl executes something other than the request described by the spec — please report it privately:

[Open a GitHub security advisory](https://github.com/hamarshehmhmd/spechurl/security/advisories/new)

Do not open a public issue for an undisclosed vulnerability.

Include the spechurl version (`spechurl --version`), the smallest spec that reproduces the problem, and what you expected to happen. You should hear back within a few days. Once a fix is released, the report can be disclosed in the changelog.
