# Releasing Sidetone

Releases are built and published by [`release.yml`](../.github/workflows/release.yml) when a
version tag is pushed. The workflow refuses a tag that doesn't match the workspace version, or a
final release whose changelog section still says "unreleased".

## Each release

1. **Main is green.** Everything committed and pushed, and [CI](../.github/workflows/ci.yml)
   passing (format, clippy, docs, tests, universal build).
2. **The README matches the build.** Retake any screenshot in `docs/images/` whose screen has
   changed, and update its alt text.
3. **Trial run.** Tag a release candidate: it publishes a pre-release you can test like a user.

   ```sh
   git tag v0.1.0-rc.1 && git push origin v0.1.0-rc.1
   ```

4. **Test the download, not your build.** With X-Plane closed:
   - Move your own `Output/preferences/Sidetone.toml` aside, so you see the first-run experience.
   - Download the zip from the release page in a browser (so macOS marks it as downloaded),
     unzip it, and copy the `Sidetone` folder into `X-Plane 12/Resources/plugins/`.
   - Start X-Plane. Check: the plugin loads (or the "Allow Anyway" step in the release notes
     works, while builds aren't notarized), the panel's welcome line, the Get set up checklist,
     Import flight, the Flight tab at sidebar and full width, and `Output/Sidetone/Sidetone.log`
     for errors.
   - If you can, have someone try it on an Intel Mac.
   - Put your settings file back.
5. **Date the changelog.** Change `## 0.1.0 — unreleased` to `## 0.1.0 — 2026-10-12` (the day you
   release), commit and push.
6. **Release.**

   ```sh
   git tag v0.1.0 && git push origin v0.1.0
   ```

   The release page gets the zip, its SHA-256 checksum, and notes taken from this version's
   changelog section. Releases are marked as pre-releases while the version is 0.x.
7. **Announce.** Upload the same zip to the X-Plane.org forums (Downloads → Plugins) with a link
   back to the GitHub release, and open a support thread there. Until VATSIM approves Sidetone
   as a pilot client, describe it as working alongside your current pilot client on VATSIM's
   public data, as the README does.
8. **Start the next version.** Bump `version` in the root `Cargo.toml` and add
   `## <next> — unreleased` at the top of the changelog.

A broken tag can be removed and pushed again: delete the release on GitHub, then
`git push origin :refs/tags/v0.1.0-rc.1 && git tag -d v0.1.0-rc.1`.

## Signing and notarization (one-time setup)

Until this is set up, builds are ad-hoc signed and macOS asks pilots to allow the plugin the
first time. Once these secrets exist, the release workflow signs the plugin with your Developer
ID (hardened runtime, secure timestamp) and notarizes it, and the "Allow Anyway" note leaves the
release notes. Local builds can be signed the same way by setting `SIDETONE_SIGN_IDENTITY`
before `cargo xtask bundle`.

1. Join the [Apple Developer Program](https://developer.apple.com/programs/).
2. In Xcode → Settings → Accounts → Manage Certificates, create a **Developer ID Application**
   certificate. In Keychain Access, export it with its private key as a `.p12` with a password.
3. At [appleid.apple.com](https://appleid.apple.com), create an app-specific password.
4. Add the repository secrets:

   ```sh
   base64 -i DeveloperID.p12 | gh secret set MACOS_CERT_P12
   gh secret set MACOS_CERT_PASSWORD          # the .p12 password
   gh secret set MACOS_SIGN_IDENTITY          # "Developer ID Application: Your Name (TEAMID)"
   gh secret set APPLE_ID                     # your Apple Developer email
   gh secret set APPLE_TEAM_ID                # the 10-character team ID
   gh secret set APPLE_APP_PASSWORD           # the app-specific password
   ```

   Then delete the exported `.p12` from your disk.

A plugin can't be stapled (it isn't an app bundle), so macOS checks the notarization online the
first time it loads.
