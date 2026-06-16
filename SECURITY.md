# Security Policy

## Supported Versions

Duroos Watcher is pre-1.0. Security fixes apply to the current mainline code until release branches exist.

## Reporting A Vulnerability

Use GitHub private vulnerability reporting when it is enabled for the public repository. If private reporting is not available, open a minimal public issue asking for a private security contact without exploit details, proof-of-concept payloads, private data, or exploitable steps. Include:

- affected version or commit
- platform and operating system
- steps to reproduce
- impact on local files, credentials, manifests, or media integrity
- whether the issue requires a malicious feed, manifest, media file, or local filesystem access

Do not include private Telegram sessions, API keys, cookies, local absolute paths, or user media in reports.

Reports about third-party channels, feeds, manifests, media, curators, downloads, external websites, or platform accounts should be directed to the relevant content owner, platform, or service provider unless they also demonstrate a Duroos Watcher software vulnerability.

## Security Model

- The app is local-first: subscriptions, watch state, manifests, and downloaded media stay on the user's machine.
- Remote feeds are untrusted input and must not execute commands, write outside the app library, or export credentials.
- Duroos v2 manifests can be signed with Ed25519; a valid signature proves integrity for the included public key, not that the curator is trusted by the user.
- Media hashes are verified when a feed provides a sha256 hash. Hash mismatches must not attach a file as ready media.
- Nostr, IPFS, and BitTorrent references are not default redistribution channels in v1.
