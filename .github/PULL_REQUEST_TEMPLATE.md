# Summary

-

# Validation

- [ ] `npm run test`
- [ ] `npm run build`
- [ ] `npm run release:check`
- [ ] `git diff --check`
- [ ] Rust/Tauri checks, if applicable:
  - [ ] `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
  - [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`
  - [ ] `cargo test --manifest-path src-tauri/Cargo.toml`

# Boundaries

- [ ] No credentials, cookies, tokens, private keys, Telegram sessions, private media, or local absolute paths are included.
- [ ] This PR preserves local-first defaults unless the change explicitly documents a fork-level responsibility.
- [ ] This PR does not bypass source permissions, paid access, platform terms, DRM, copyright controls, or review-first downloads.
- [ ] New release or packaging claims are backed by artifact evidence, not just source changes.
