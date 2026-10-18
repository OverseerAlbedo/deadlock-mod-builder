# Deadlock Mod Builder 0.13.0-beta.10

Fixes for **Set up model compiler…**, the page that downloads the model compiler with DepotDownloader and your Steam account (reported in issue #2). Everything from the earlier betas is still in; see `CHANGELOG.md`.

## What changed

- **The Steam QR code appears on screen.** With Steam mobile QR sign-in, the code pops up as soon as Steam asks for it, renews with Steam's code and closes when you are signed in. Before, it was only in Logs, where nothing told you to look and the code was unreadable, so the sign-in timed out with "Failed to authenticate with Steam".
- **Fewer scans:** Download now asks for one QR scan instead of three, so the whole setup takes two (Fetch manifests and plan, then Download).
- **Sign in to Steam…** for a locally authenticated Steam account: opens DepotDownloader once in its own window, where you type your password and Steam Guard code. DepotDownloader keeps the sign-in and the app's downloads use it; the app never sees your password. Before, the download crashed with "LogOn requires a username and password or access token".
- **Clearer errors:** Steam sign-in failures say what happened and what to click, and choosing the wrong folder as the model toolset is explained (a Community SDK folder such as Reduced_CSDK_12\game, or Deadlock's own game files). Choosing the toolset's game folder or the acquisition folder selects the toolset itself.
- **Manual download steps** (Or use files you already have → Show manual download steps): five numbered steps with a Copy button for each command, a QR or username switch, and an explanation of why the download command only appears after Export exact file plan.

## Good to know

- The model compiler download is about 4 GB, not the whole Counter-Strike 2 game. Your Steam account needs Counter-Strike 2 in its library (it is free and does not have to be installed).

## Checked

- Checked on October 18, 2026: QR sign-in with the on-screen code fetched the manifests with one scan.
- Not yet checked with a real Steam account: Download with one scan, and Sign in to Steam….

## After updating the app

- Nothing to redo. Projects, settings, prepared models and themes are kept.
