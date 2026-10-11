import { useState } from "react";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";
const native = vi.hoisted(() => ({
  validateAssetPath: vi.fn(),
  previewExtra: vi.fn(),
}));
vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  api: native,
}));
vi.mock("./AudioPlayer", () => ({
  AudioPlayer: ({ label, volume }: { label: string; volume: number }) => (
    <div data-testid={`player-${label}`}>{volume ?? 100}% audition</div>
  ),
}));
const original = "panorama/images/heroes/hero_test.vtex_c",
  sound = "sounds/heroes/test/voice.vsnd_c";
const initial = {
  ...reference.cases[1].expected,
  hero_name: "Test hero",
  customization: {
    enabled: true,
    future: { keep: 1 },
    physics: { enabled: true, source: "cloth.vmdl_c" },
  },
} as unknown as Project;
const catalog: ComponentCatalog = {
  hero_path: initial.original_model,
  icons: [
    {
      path: original,
      target: original,
      label: "Hero portrait",
      width: 128,
      height: 128,
      supported: true,
      original_sha256: "a".repeat(64),
    },
    {
      path: "panorama/images/atlas.vtex_c",
      target: "panorama/images/atlas.vtex_c",
      label: "Ability atlas",
      supported: false,
      reason: "This is a sprite sheet.",
    },
  ],
  audio: [
    {
      path: sound,
      target: sound,
      label: "Voice line",
      supported: true,
      shared: true,
      metadata: { duration: 2.5, channels: 1 },
      original_sha256: "b".repeat(64),
    },
  ],
  effects: [],
  animations: [],
  variants: [],
  animation_unavailable: null,
  warnings: [],
};
function packet(mime_type: string) {
  const h = new TextEncoder().encode(
    JSON.stringify({ mime_type, details: {} }),
  );
  const buffer = new ArrayBuffer(4 + h.length + 8);
  new DataView(buffer).setUint32(0, h.length, true);
  new Uint8Array(buffer, 4, h.length).set(h);
  return buffer;
}
beforeEach(() => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  native.validateAssetPath.mockImplementation(async (path) => path);
  native.previewExtra.mockImplementation(async ({ kind }) =>
    packet(kind === "icons" ? "image/png" : "audio/wav"),
  );
  vi.stubGlobal(
    "URL",
    Object.assign(URL, {
      createObjectURL: vi.fn(() => `blob:test`),
      revokeObjectURL: vi.fn(),
    }),
  );
});
function Harness({
  start = initial,
  busy = false,
  assets = catalog,
}: {
  start?: Project;
  busy?: boolean;
  assets?: ComponentCatalog;
}) {
  const [p, set] = useState(structuredClone(start));
  return (
    <>
      <ExtrasWorkspace
        project={p}
        update={set}
        disabled={busy}
        catalog={assets}
        fail={vi.fn()}
      />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () =>
  JSON.parse(screen.getByTestId("saved").textContent!) as Project;
async function drop(path: string) {
  const node = document.querySelector("[data-native-drop]");
  expect(node).toBeTruthy();
  await act(async () =>
    node!.dispatchEvent(new CustomEvent("dmb-file-drop", { detail: [path] })),
  );
}
describe("Extras workspace", () => {
  it("displays vector originals as preview-only images", async () => {
    native.previewExtra.mockResolvedValue(packet("image/svg+xml"));
    const target = "panorama/images/heroes/hero_names/abrams.vsvg_c";
    render(<ExtrasWorkspace project={initial} update={vi.fn()} disabled={false} fail={vi.fn()} catalog={{...catalog,icons:[{path:target,target,label:"Abrams logo",supported:false,reason:"Vector image: preview available. Replacing compiled vector artwork is not supported yet.",original_sha256:"c".repeat(64)}]}}/>);
    fireEvent.click(screen.getByRole("button",{name:/Abrams logo/}));
    expect(await screen.findByAltText(/icon preview/)).toHaveAttribute("src","blob:test");
    expect(screen.getByText(/Replacing compiled vector artwork/)).toBeInTheDocument();
    expect(screen.queryByText("Drop replacement here")).not.toBeInTheDocument();
  });
  it("enables replacement after a reload qualifies a previously view-only Panorama icon", async () => {
    const entry = {...catalog.icons[0], supported: false, reason: "Unsupported texture processing flags"};
    const view = render(<Harness assets={{...catalog, icons: [entry]}} />);
    fireEvent.click(screen.getByRole("button", {name: /Hero portrait/}));
    expect(document.querySelector("[data-native-drop]")).toBeNull();
    view.rerender(<Harness assets={{...catalog, icons: [{...entry, supported: true, reason: "", metadata: {format: 4, flags: 395, extra_types: [1,3], display_width: 137, display_height: 137}}]}} />);
    await drop("C:\\Art\\ability.png");
    await waitFor(() => expect(saved().customization.icons?.[0]).toMatchObject({target: original, source: "C:\\Art\\ability.png", fit: "contain"}));
    expect(screen.queryByText("Original kept")).not.toBeInTheDocument();
    expect(saved().customization.physics).toEqual(initial.customization.physics);
  });
  it("loads current-character assets automatically once and keeps explicit reload available", async () => {
    const discover = vi.fn();
    const props = {
      project: initial,
      update: vi.fn(),
      disabled: false,
      fail: vi.fn(),
      discover,
    };
    const view = render(<ExtrasWorkspace {...props} />);
    await waitFor(() => expect(discover).toHaveBeenCalledTimes(1));
    view.rerender(<ExtrasWorkspace {...props} discover={() => discover()} />);
    expect(discover).toHaveBeenCalledTimes(1);
    fireEvent.click(
      screen.getAllByRole("button", { name: /Load character assets/ })[0],
    );
    expect(discover).toHaveBeenCalledTimes(2);
  });
  it("previews the original before importing and saves a validated drop without losing other choices", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /Hero portrait/ }));
    await waitFor(() =>
      expect(screen.getByAltText("Original icon preview")).toBeVisible(),
    );
    await drop("C:\\Art\\portrait.png");
    await waitFor(() =>
      expect(saved().customization.icons?.[0]).toMatchObject({
        source: "C:\\Art\\portrait.png",
        target: original,
        original_sha256: "a".repeat(64),
        fit: "contain",
      }),
    );
    expect(native.validateAssetPath).toHaveBeenCalledWith(
      "C:\\Art\\portrait.png",
      "icon",
    );
    expect(saved().customization.future).toEqual({ keep: 1 });
    expect(saved().customization.physics?.source).toBe("cloth.vmdl_c");
    fireEvent.click(screen.getByRole("button", { name: /Fill & crop/ }));
    expect(saved().customization.icons?.[0].fit).toBe("cover");
    await waitFor(() =>
      expect(native.previewExtra).toHaveBeenCalledWith(
        expect.objectContaining({
          source: "C:\\Art\\portrait.png",
          fit: "cover",
        }),
      ),
    );
    fireEvent.click(screen.getByLabelText("Include in build"));
    expect(saved().customization.icons?.[0].enabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Remove replacement" }));
    expect(saved().customization.icons).toEqual([]);
    expect(saved().customization.physics?.enabled).toBe(true);
  });
  it("saves sound gain from zero to two hundred with unity as default and previews the same gain", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
    fireEvent.click(screen.getByRole("button", { name: /Voice line/ }));
    await drop("C:\\voices\\new.mp3");
    await waitFor(() =>
      expect(saved().customization.audio?.[0]).toMatchObject({
        volume_percent: 100,
        audio_fit: "pad",
        allow_shared: false,
      }),
    );
    fireEvent.change(screen.getByLabelText("Replacement volume"), {
      target: { value: "200" },
    });
    expect(saved().customization.audio?.[0].volume_percent).toBe(200);
    await waitFor(() =>
      expect(screen.getByTestId("player-replacement")).toHaveTextContent(
        "200% audition",
      ),
    );
    fireEvent.change(screen.getByLabelText("Replacement volume percent"), {
      target: { value: "0" },
    });
    expect(saved().customization.audio?.[0].volume_percent).toBe(0);
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(saved().customization.audio?.[0].volume_percent).toBe(100);
    fireEvent.change(screen.getByLabelText("Replacement volume percent"), {
      target: { value: "999" },
    });
    fireEvent.blur(screen.getByLabelText("Replacement volume percent"));
    expect(saved().customization.audio?.[0].volume_percent).toBe(200);
    fireEvent.click(screen.getByLabelText("Include this shared replacement"));
    expect(saved().customization.audio?.[0].allow_shared).toBe(true);
    fireEvent.change(screen.getByLabelText("Sound timing"), {
      target: { value: "trim" },
    });
    expect(saved().customization.audio?.[0].audio_fit).toBe("trim");
    expect(saved().customization.physics?.enabled).toBe(true);
  });
  it("keeps unsupported originals without a warning or unusable replacement controls", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /Ability atlas/ }));
    await waitFor(() =>
      expect(screen.getByAltText("Original icon preview")).toBeVisible(),
    );
    expect(screen.getByText("Original kept")).toBeVisible();
    expect(screen.getByText(/No action needed/)).toBeVisible();
    expect(screen.getByText("This is a sprite sheet.")).not.toBeVisible();
    expect(document.querySelector(".extra-editor-notice")).toBeNull();
    fireEvent.click(screen.getByText("File details"));
    expect(screen.getByText("This is a sprite sheet.")).toBeVisible();
    expect(screen.queryByText("Your replacement")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Choose file" })).not.toBeInTheDocument();
    expect(document.querySelector("[data-native-drop]")).toBeNull();
    expect(saved()).toEqual(initial);
  });
  it("keeps an unsupported saved replacement visible and removable", () => {
    const start = {
      ...initial,
      customization: {
        ...initial.customization,
        icons: [{ target: catalog.icons[1].target, source: "C:\\Art\\atlas.png", enabled: true }],
      },
    };
    render(<Harness start={start} />);
    fireEvent.click(screen.getByRole("button", { name: /Ability atlas/ }));
    expect(screen.getByText("This saved replacement is unavailable")).toBeVisible();
    expect(screen.queryByText("Original kept")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Include in build")).toBeChecked();
    fireEvent.click(screen.getByLabelText("Include in build"));
    expect(saved().customization.icons?.[0].enabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: /Remove replacement/ }));
    expect(saved().customization.icons).toEqual([]);
    expect(screen.getByText("Original kept")).toBeVisible();
    expect(saved().customization.physics).toEqual(initial.customization.physics);
  });
  it("bounds large catalogs, filters replacements and supports keyboard category navigation", () => {
    const icons = Array.from({ length: 1200 }, (_, i) => ({
      path: `icons/${i}.vtex_c`,
      target: `icons/${i}.vtex_c`,
      label: `Artwork ${i}`,
      supported: true,
    }));
    render(<Harness assets={{ ...catalog, icons }} />);
    const library = within(
      screen.getByRole("region", { name: "Icons assets" }),
    );
    expect(library.getAllByRole("listitem")).toHaveLength(48);
    fireEvent.change(screen.getByLabelText("Search icons"), {
      target: { value: "Artwork 1199" },
    });
    expect(library.getAllByRole("listitem")).toHaveLength(1);
    fireEvent.keyDown(screen.getByRole("tab", { name: /Icons/ }), {
      key: "ArrowRight",
    });
    expect(screen.getByRole("tab", { name: /Sounds/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    fireEvent.click(screen.getByRole("button", { name: "Changed (0)" }));
    expect(screen.getByText("No replacements yet")).toBeVisible();
  });
  it("keeps missing saved destinations recoverable without inventing a game path", () => {
    const start = {
      ...initial,
      customization: {
        ...initial.customization,
        icons: [
          {
            source: "old.png",
            target: "panorama/images/missing.vtex_c",
            enabled: true,
          },
        ],
      },
    };
    render(<Harness start={start} />);
    fireEvent.click(screen.getByRole("button", { name: "Changed (1)" }));
    fireEvent.click(screen.getByRole("button", { name: /missing/ }));
    fireEvent.click(screen.getByRole("button", { name: "Choose destination" }));
    fireEvent.click(screen.getByRole("button", { name: /Hero portrait/ }));
    expect(saved().customization.icons?.[0]).toMatchObject({
      source: "old.png",
      target: original,
      allow_shared: false,
      original_sha256: "a".repeat(64),
    });
  });
  it("cannot import while a build is running and never displays a different hero catalog", () => {
    const view = render(<Harness busy />);
    fireEvent.click(screen.getByRole("button", { name: /Hero portrait/ }));
    expect(screen.getByRole("button", { name: "Choose file" })).toBeDisabled();
    expect(document.querySelector("[data-native-drop]")).toBeNull();
    view.unmount();
    render(
      <ExtrasWorkspace
        project={{ ...initial, original_model: "models/another.vmdl" }}
        update={vi.fn()}
        disabled={false}
        catalog={catalog}
        fail={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("button", { name: /Hero portrait/ }),
    ).not.toBeInTheDocument();
  });
});
