# M8 — macOS distribution

## Summary

Ship fluentb as a signed, notarized macOS app that users can install from a DMG or Homebrew, built automatically by a release workflow.

## Goals

- Installing fluentb produces no Gatekeeper warnings.
- Permissions persist across updates (stable code signature and bundle ID).
- A tagged release produces artifacts with no manual steps.

## Non-goals

- Mac App Store (its sandbox conflicts with global hotkeys and Accessibility).
- Windows and Linux packaging (M9, M10).

## User stories

- As a user, I `brew install --cask fluentb` or drag the app from a DMG, and it opens without security warnings.
- As a user, I'm told when a new version is available.

## Requirements

### Functional

1. **Bundle** — `fluentb.app` built with `cargo-bundle` or a custom script:
   - Bundle ID `com.fluentb.app` (to be confirmed).
   - `Info.plist`: `LSUIElement`, `NSMicrophoneUsageDescription` ("fluentb transcribes your speech locally on this Mac."), and an Accessibility usage explanation.
   - App icon and template tray icons.
   - Universal binary (arm64 + x86_64) or arm64 only (see Open questions).
2. **Signing** — Developer ID Application certificate, hardened runtime, and entitlements for audio input only.
3. **Notarization** — `notarytool submit --wait`, then `stapler staple`.
4. **DMG** — `create-dmg` with an Applications symlink.
5. **Homebrew cask** — in a `fluentb/homebrew-tap` repository, updated by the release workflow.
6. **Release workflow** — `.github/workflows/release.yml` on `v*` tags:
   - Build, sign, notarize, create the DMG, upload to GitHub Releases, generate a SHA-256 checksum, and bump the cask.
   - Secrets: certificate p12, its password, and the App Store Connect API key.
7. **Update check** — once a day (opt-out in config), query the GitHub Releases API for the latest tag and show "Update available" in the tray menu. The request sends only a standard version check, with no identifiers.
8. **Version** — shown in the tray menu and taken from `Cargo.toml`.

### Non-functional

- Reproducible release builds from CI only. Releases are never built on a laptop.
- Minimum macOS version declared (13.0, because of `SMAppService`).

## Testing

- CI dry-run job on PRs: build and bundle without signing, and check `Info.plist` keys.
- Release candidate checklist: a clean VM install, Gatekeeper check (`spctl -a -vv`), permissions granted, update from the previous version keeps those permissions.

## Acceptance criteria

- [ ] A tagged release produces a notarized DMG on GitHub Releases.
- [ ] `brew install --cask` works.
- [ ] Upgrading keeps the Microphone and Accessibility grants.
- [ ] The tray shows "Update available" when a newer release exists.

## Open questions

- Is an Apple Developer account ($99/yr) available? It's required for signing and notarization.
- Universal binary or arm64 only?
- Auto-update (Sparkle) or notify only? Notify only is planned for M8.
