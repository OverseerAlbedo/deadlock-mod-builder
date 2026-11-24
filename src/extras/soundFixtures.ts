// A small neutral sound catalog for the Sounds tests: hero and ability names
// from the game, no project data.
import type { CatalogEntry, SoundInfo } from "../components/AssetCatalog";

type Info = Partial<SoundInfo> & Pick<SoundInfo, "name" | "group" | "group_name" | "tags">;
const ORDER: Record<string, number> = { ability: 100, weapon: 200, voice: 300, about: 380, other: 400, shared: 900 };
export function soundEntry(path: string, info: Info, extra: Partial<CatalogEntry> = {}): CatalogEntry {
  const stem = path.split("/").pop()!.replace(/\.vsnd_c$/, "");
  return {
    path,
    target: path,
    label: stem.replace(/_/g, " "),
    supported: true,
    wav_supported: true,
    original_sha256: `sha-${stem}`,
    metadata: { duration: 1.5, channels: 2 },
    association: "hero_directory",
    shared: false,
    ...extra,
    sound: {
      group_order: ORDER[info.group.split(":")[0]] ?? 400,
      family: path.replace(/_\d+\.vsnd_c$/, "").replace(/\.vsnd_c$/, ""),
      family_source: "name",
      ...info,
    },
  };
}
const fire = (n: number, extra: Partial<CatalogEntry> = {}) =>
  soundEntry(
    `sounds/abilities/hornet/vindicta_assassinate_fire_0${n}.vsnd_c`,
    {
      name: "Assassinate · fire",
      group: "ability:assassinate",
      group_name: "Assassinate",
      tags: ["hero", "ability"],
      family_source: "event",
      variation: n,
      events: ["Hornet.Assassinate.Cast"],
    },
    { metadata: { duration: 1.6 + n / 10, channels: 2 }, ...extra },
  );
export const soundCatalog: CatalogEntry[] = [
  fire(1),
  fire(2),
  fire(3, { shared: true, association: "resource_reference" }),
  fire(4),
  fire(5, { supported: false, reason: "Integrity check failed: Unsupported sound codec." }),
  soundEntry("sounds/abilities/hornet/vindicta_assassinate_scope_in.vsnd_c", {
    name: "Assassinate · scope in",
    group: "ability:assassinate",
    group_name: "Assassinate",
    tags: ["hero", "ability"],
  }),
  soundEntry("sounds/abilities/hornet/vindicta_flight_cast.vsnd_c", {
    name: "Flight · cast",
    group: "ability:flight",
    group_name: "Flight",
    tags: ["hero", "ability"],
    events: ["Vindicta.Leap.Cast"],
  }),
  soundEntry("sounds/weapons/hornet/reload/vindicta_wpn_reload_start_01.vsnd_c", {
    name: "Reload · start",
    group: "weapon:reload",
    group_name: "Reload",
    tags: ["hero", "weapon"],
  }),
  soundEntry("sounds/vo/hornet/ping/hornet_ping_careful_haze_01.vsnd_c", {
    name: "Careful Haze",
    group: "voice:pings",
    group_name: "Pings",
    tags: ["hero", "voice"],
    voice: ["ping"],
    variation: 1,
  }),
  soundEntry("sounds/vo/hornet/ping/hornet_ping_careful_haze_02.vsnd_c", {
    name: "Careful Haze",
    group: "voice:pings",
    group_name: "Pings",
    tags: ["hero", "voice"],
    voice: ["ping"],
    variation: 2,
  }),
  soundEntry("sounds/vo/hornet/hornet_use_power4_01.vsnd_c", {
    name: "Uses ability 4",
    group: "voice:ability_lines",
    group_name: "Ability voice lines",
    tags: ["hero", "ability", "voice"],
    voice: ["ability"],
    variation: 1,
  }),
  soundEntry(
    "sounds/vo/ghost/ghost_kill_hornet_01.vsnd_c",
    {
      name: "Kill Vindicta",
      group: "about:ghost",
      group_name: "Lines from Lady Geist",
      tags: ["voice"],
      voice: ["about", "kill"],
    },
    { shared: true, association: "other_hero_voice" },
  ),
  soundEntry("sounds/player/footsteps/hornet/vindicta_footstep_01.vsnd_c", {
    name: "Footstep",
    group: "other:footsteps",
    group_name: "Footsteps",
    tags: ["hero"],
  }),
];
