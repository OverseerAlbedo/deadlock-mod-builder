# Deadlock Mod Builder 0.14.5

The app now speaks Simplified Chinese (简体中文). This release also updates the library that reads source package ZIP files to a version without known security problems, raises the limit on kept moving-part bones, and fixes one fault. Everything else is as in 0.14.0. See `CHANGELOG.md`.

## New

- **Simplified Chinese (简体中文).** The whole interface is available in Simplified Chinese. Choose the language at the top of **Settings → Appearance**, or on the first card of the new-user guide. On a first start the app follows the Windows display language. Switching takes effect at once, without a restart, and keeps unsaved work.
- **Hero and item names in Chinese.** With the Chinese interface, heroes and shop items are named as the game's own Chinese text names them. The hero picker and the item search find them by their Chinese or English name.
- **One project for both languages.** Projects and settings are the same in either language: a project made with the Chinese interface opens with the English one, and the other way round.
- **Website and README in Chinese.** The website's home page and tutorial and the README are available in Simplified Chinese; the website has a language button in its header.

### What is still in English

With the Chinese interface, these stay in English for now:

- the text of error messages and their help steps;
- the notes under a settings check, a preparation result or an analysis;
- the log and the step timings;
- names the app makes from the game's file names: sounds, effects, animations and ability objects.

File, bone and material names are never translated.

## Changed

- **More kept moving-part bones.** A model can keep up to 1,024 moving-part bones (512 before), so models with a lot of hair, cloth and accessories can be prepared. Prepare model → Moving parts shows the running total, and a model over the limit is told its total and its largest groups.
- **Source package ZIP files** are read by an updated library that fixes known security problems. Packages import as before, with one difference: file names with non-English letters that a ZIP stores without its Unicode mark are now read correctly, so such a package is no longer refused.
- The libraries the app is built on were updated to their latest fix releases.

## Fixed

- If another program asked the app to close while it was minimized (for example the Windows command `taskkill` without `/F`), the app stayed open but could no longer show its questions. Such a request now closes the app like its own close button, with the usual question about unsaved changes.

## After updating the app

- Nothing to do: projects, settings, themes and installed mods are unaffected.
- On a Chinese Windows the app offers Simplified Chinese once after the update; it stays in English unless you switch.
- Coming from 0.13.0 or earlier? The large changes are in 0.14.0: see its [release notes](https://github.com/OverseerAlbedo/deadlock-mod-builder/releases/tag/v0.14.0).

## License

Deadlock Mod Builder is licensed under the GNU Affero General Public License v3.0 or later (see `LICENSE` and `NOTICE`). Versions up to and including 0.13.0 stay under the MIT License.
