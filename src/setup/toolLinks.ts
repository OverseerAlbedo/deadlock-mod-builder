// i18n-area: settings
import { m } from '../i18n';

/** Stable publisher pages, with versions constrained by the existing tool bridges. */
export const toolLinks = {
  steam: { name: 'Steam', action: m('Get Steam'), url: 'https://store.steampowered.com/about/', requirement: m('Required'), help: m('Install Steam and sign in to the account you use for Deadlock.') },
  deadlock: { name: 'Deadlock', action: m('Open Deadlock on Steam'), url: 'https://store.steampowered.com/app/1422450/Deadlock/', requirement: m('Required'), help: m('Install the game through your Steam account. Game access is managed by Steam.') },
  source2viewer: { name: 'Source 2 Viewer CLI', action: m('Get Source 2 Viewer CLI'), url: 'https://github.com/ValveResourceFormat/ValveResourceFormat/releases', requirement: m('Required'), help: m('Choose the Windows x64 command-line download, extract it, then select Source2Viewer-CLI.exe.') },
  csdk: { name: 'Reduced CSDK 13', action: m('Get Reduced CSDK 13'), url: 'https://deadlockmodding.pages.dev/modding-tools/csdk-13', requirement: m('Required for material and model compilation'), help: m('Open the community page, download the ZIP and extract the complete Reduced_CSDK_13 folder to a short path outside OneDrive. Its resource compiler builds the materials and the hero model, so nothing else is needed.') },
  depotdownloader: { name: 'DepotDownloader 3.4.0', action: m('Get DepotDownloader'), url: 'https://github.com/SteamRE/DepotDownloader/releases/tag/DepotDownloader_3.4.0', requirement: m('For model-tool acquisition'), help: m('Choose DepotDownloader-windows-x64.zip and extract it. Needed when the builder downloads the selected model toolset.') },
  cs2: { name: 'Counter-Strike 2', action: m('Open Counter-Strike 2 on Steam'), url: 'https://store.steampowered.com/app/730/CounterStrike_2/', requirement: m('For model-tool acquisition'), help: m('The acquisition workflow uses your Steam access to the CS2 tools. A full game installation is optional when using the selected toolset download.') },
  blender: { name: 'Blender 5.2 LTS', action: m('Get Blender 5.2 LTS'), url: 'https://www.blender.org/releases/5-2/', requirement: m('Required to prepare Blender, FBX, glTF and MMD models'), help: m('Needed for model conversion and Blender/FBX previews. This app supports Blender 4.2–4.5, 5.0, 5.2 LTS and 5.3; 5.2 LTS is recommended. MMD models (.pmx, .pmd) also need the MMD Tools add-on in that Blender: Edit → Preferences → Get Extensions.') },
  // Not one of the four installs: only MMD models need it (Settings → Blender executable).
  mmdtools: { name: 'MMD Tools', action: m('Get MMD Tools'), url: 'https://extensions.blender.org/add-ons/mmd-tools/', requirement: m('Only for MMD models (.pmx, .pmd)'), help: m('The Blender add-on that imports MMD models. Its page explains how to install it in the Blender set in Settings.') },
  vcredist: { name: 'Microsoft Visual C++ Runtime', action: m('Get the Visual C++ runtime'), url: 'https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist/', requirement: m('Compiler prerequisite'), help: m('Use the x64 runtime from Microsoft if your compiler tools need it. It may already be installed.') },
  webview2: { name: 'Microsoft Edge WebView2', action: m('Get WebView2 Runtime'), url: 'https://developer.microsoft.com/en-us/microsoft-edge/webview2/', requirement: m('App prerequisite'), help: m('Use the Evergreen Runtime. The app installer handles this when needed; portable users may need to install it separately.') },
} as const;
export type ToolLinkId = keyof typeof toolLinks;
