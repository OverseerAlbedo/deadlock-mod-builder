using System.Collections.Concurrent;
using System.Reflection;
using Microsoft.Extensions.Logging;
using ValvePak;
using ValveResourceFormat.IO;

namespace Dmb.EffectPlayer;

/// <summary>
/// File loader for one game package and an optional overlay package.
///
/// The overlay is the loader's current package, so files in it win over the game's. The game's
/// package is added as the only search package: the game's gameinfo.gi is read for its settings
/// (tone mapping and game identity) but its search paths are not mounted, because they list the
/// addons folder and would load installed mods over the game's own files.
///
/// <see cref="GameOnly"/> hides the overlay for the duration of a scope, which is how the game's
/// version of an effect is built next to the edited one from the same loader.
/// </summary>
sealed class EffectLoader : GameFileLoader
{
    /// <summary>Every file looked up since the last <see cref="ResetResolved"/>, with where it came from,
    /// while the overlay is in force.</summary>
    public ConcurrentDictionary<string, string> Resolved { get; } = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>The same, for lookups made inside a <see cref="GameOnly"/> scope.</summary>
    public ConcurrentDictionary<string, string> ResolvedGame { get; } = new(StringComparer.OrdinalIgnoreCase);

    public string? GameInfoPath { get; }

    /// <summary>The path the current overlay was read from, if any.</summary>
    public string? OverlayPath { get; private set; }

    int gameOnlyDepth;

    public EffectLoader(Package? overlay, string gamePak, ILogger logger)
        : base(overlay, null, logger)
    {
        OverlayPath = overlay?.FileName;
        var gameFolder = Path.GetDirectoryName(Path.GetFullPath(gamePak))!;
        var gameInfo = Path.Combine(gameFolder, "gameinfo.gi");

        if (File.Exists(gameInfo))
        {
            try
            {
                // The loader only reads gameinfo.gi while mounting its search paths; set the result directly.
                GameInfoPath = LibraryAccess.SetGameInfo(this, gameInfo) ? gameInfo : null;
            }
            catch (Exception e)
            {
                logger.LogWarning("Could not read {GameInfo}: {Message}", gameInfo, e.Message);
            }
        }

        AddPackageToSearch(gamePak);

        // The game's own shader package and the engine's core packages (textures such as blue noise,
        // shader data), mounted the way the engine mounts them after the game folder. The addons
        // folder is deliberately left out.
        var core = Path.Combine(Path.GetDirectoryName(gameFolder)!, "core");
        foreach (var extra in new[]
        {
            Path.Combine(gameFolder, "shaders_pc_dir.vpk"),
            Path.Combine(core, "pak01_dir.vpk"),
            Path.Combine(core, "shaders_pc_dir.vpk"),
        })
        {
            if (File.Exists(extra) && !IsPackageMounted(extra))
            {
                AddPackageToSearch(extra);
            }
        }
    }

    /// <summary>
    /// Replaces the overlay package (or removes it with <c>null</c>). The previous package is
    /// returned, still open, so the caller can close it once nothing built from it is in use.
    /// </summary>
    public Package? SwapOverlay(Package? overlay)
    {
        var previous = CurrentPackage;
        CurrentPackage = overlay;
        OverlayPath = overlay?.FileName;
        return previous;
    }

    /// <summary>Hides the overlay until the returned scope is disposed.</summary>
    public IDisposable GameOnly()
    {
        gameOnlyDepth++;
        return new Scope(this);
    }

    sealed class Scope(EffectLoader loader) : IDisposable
    {
        bool disposed;

        public void Dispose()
        {
            if (!disposed)
            {
                disposed = true;
                loader.gameOnlyDepth--;
            }
        }
    }

    public void ResetResolved()
    {
        Resolved.Clear();
        ResolvedGame.Clear();
    }

    public override (string? PathOnDisk, Package? Package, PackageEntry? PackageEntry) FindFile(string file, bool logNotFound = true)
    {
        var gameOnly = gameOnlyDepth > 0;
        var overlay = CurrentPackage;
        if (gameOnly)
        {
            CurrentPackage = null;
        }

        try
        {
            var found = base.FindFile(file, logNotFound);
            var source = found.PathOnDisk
                ?? (found.Package != null ? Path.GetFileName(found.Package.FileName) + (overlay != null && ReferenceEquals(found.Package, overlay) ? " (overlay)" : string.Empty) : null)
                ?? "NOT FOUND";
            (gameOnly ? ResolvedGame : Resolved).TryAdd(file, source);
            return found;
        }
        finally
        {
            if (gameOnly)
            {
                CurrentPackage = overlay;
            }
        }
    }
}
