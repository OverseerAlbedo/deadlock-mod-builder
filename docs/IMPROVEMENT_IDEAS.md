# Improvement ideas

Ideas from using the app as a new and as a returning user. Nothing here is
implemented. Each idea says roughly how big it is (S = an afternoon,
M = a few days, L = a week or more). Numbers stay as first listed.

**Planned, in this order:** 2, 5, 12, 28, 31, 33, 36 (quick wins); 3, 29, 9;
19, 20; then 10 with 18 (one movement preview on the Physics page) and 21.
Dropped after review: 6 (neither compiler supports Windows long paths), 7, 8
(the repository is private), 14, 16, 17, 30 and 32.

## Getting set up (new users)

1. **Download the free tools for me** (M). With one consent click, fetch the
   pinned Source 2 Viewer CLI and DepotDownloader releases from GitHub, extract
   them to `C:\DeadlockModdingTools`, and fill in Settings. Reduced CSDK 12 still
   needs its community page, but the app could watch the Downloads folder and
   offer to extract the ZIP to `C:\CSDK12` when it appears.
2. **Open the setup guide by itself** (S) on first start, and whenever a
   required path turns red, instead of waiting for the user to find Settings.
3. **"Test my setup" button** (M): compile one small material with the
   Community SDK and the original hero with the model toolset, then report
   pass or fail with the new fix-it steps. Catches crossed compilers before
   the first real build.
4. **Support bundle** (S): one button that zips the latest log, timing report,
   settings (with `C:\Users\<name>` replaced) and the error help into a file
   to send on Discord. Most "it doesn't work" messages would arrive with
   everything needed.
5. **Check before building** (S): warn when Deadlock is running (it locks the
   addon VPKs), when the drive has less than about 10 GB free, and when the
   workspace or SDK is inside OneDrive.

## Preparing models

9. **Keep non-English names readable** (M): bone and material names in
   Chinese or Japanese are currently stripped to underscores (`_2_0f3d75fa`,
   `dmb_secondary_______2_L` in the friend's build). Transliterate them
   (pinyin, romaji) so logs, the Physics table and error messages name real parts.
10. **Animation preview** (L): play a few hero animations (idle, run, shoot,
    jump, melee) on the prepared model in the 3D view, so stretching, clipping
    and finger grip show up before a 3-minute build and a game launch.
11. **Clipping finder** (L): run those animations and highlight vertices that
    pass into the body (or arms into the torso), with a list of the worst spots.
12. **Hero proportion notice at hero selection** (S): say "Shiv has a long
    right arm" when the hero is picked, not only in the preparation warnings,
    and suggest Match hero proportions or Keep source proportions.
13. **Side-by-side skeleton overlay** (M): show the hero's skeleton over the
    model in the preview, with joints that are far apart marked in red.
15. **Prepare for several heroes at once** (L): batch preparation of one model
    for a list of heroes, for people who make a character for many slots.

## Physics and motion

18. **Live motion preview** (L): simulate the moving parts in the 3D view with
    the same goal, drag, gravity and travel values, so amounts can be tuned
    without building.
19. **Sliders instead of three presets** (M): stiffness, swing, gravity and
    travel per part, with the automatic values as the starting point.
20. **Save my physics preset** (S): reuse a tuned set of part types and
    amounts on the next project.
21. **Read MMD and VRM spring settings** (M): use the source rig's own spring
    or rigid-body stiffness as the starting values instead of generic ones.

## Building and installing

22. **Installed mods manager** (M): list every VPK in the addons folder with
    the hero it replaces and the project that built it; enable, disable or
    remove with one click; flag two mods replacing the same hero.
23. **Faster partial rebuilds** (M): rebuild only materials, or only the
    model, when only one side changed.
24. **Smaller VPKs** (M): offer 2K textures (the friend's VPKs are about
    110 MB with many 5.5 MB textures) and share identical textures between
    materials.
25. **Share package** (S): export a ZIP with the VPK, a preview image and a
    short readme (hero, slot, author) ready for GameBanana.
26. **Screenshot of the result** (M): render a front and side image of the
    built character for the project card and the share package.
27. **Build finished notification** (S): a Windows notification (and sound)
    when a long build ends while the app is in the background.

## Everyday use (returning users)

28. **What changed since the last build** (S): a short list (gun fit, 3
    materials, breast motion on) at the top of Build & install.
29. **Autosave and recovery** (M): save the project every few minutes and
    offer to restore after a crash.
31. **Keyboard shortcuts** (S): Ctrl+S save, Ctrl+B build, Ctrl+1–5 for the
    five steps.
33. **ETA on long steps** (S): use earlier timing reports to show "about 2
    minutes left" during preparation and builds.

## Quality and support

34. **Error help that learns** (S, ongoing): every new support question about
    a message becomes a rule in `troubleshooting.rs` with its real text as a test.
35. **Known-good hero list** (S): mark heroes whose builds were checked in game
    (Haze, Vindicta, Lady Geist …) and which still need checks (Shiv, Wraith),
    so users pick a well-tested hero for their first mod.
36. **Privacy-safe logs** (S): replace the Windows user name in every log line
    the app shows or exports.
