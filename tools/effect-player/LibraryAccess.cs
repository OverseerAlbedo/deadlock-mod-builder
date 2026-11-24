using System.Collections;
using System.Reflection;
using ValveResourceFormat.Particles;
using ValveResourceFormat.Renderer.SceneNodes;

namespace Dmb.EffectPlayer;

/// <summary>
/// Every member of the rendering library that the player reaches by reflection, in one place. The
/// library keeps these internal or private; each lookup is made once, and every use fails soft:
/// when a member is missing (another library version, or a build that trimmed it away), the caller
/// gets null or false and the feature that needs it is reported as unavailable rather than failing.
/// The members, all of ValveResourceFormat and ValveResourceFormat.Renderer 21.1:
/// <list type="bullet">
/// <item><c>ParticleSceneNode.particleRenderer</c> and, on its type, <c>Simulation</c>,
/// <c>renderers</c>, <c>childRenderers</c> and <c>EnumerateSkippedRendererClasses()</c>: the
/// simulation and renderers behind a particle node.</item>
/// <item><c>ParticleSystemSimulation.PinRandomSeed(int)</c>: fixes the seed each replay starts from.</item>
/// <item><c>RenderModels.pool</c>, its <c>Column</c> and <c>slots</c>, and
/// <c>ParticleCollection.handleColumns</c>: which model node draws which particle.</item>
/// <item><c>GameInfo.Read(string)</c> and <c>GameFileLoader.FoundGameInfo</c>: the game's
/// gameinfo.gi settings without mounting its search paths.</item>
/// </list>
/// </summary>
static class LibraryAccess
{
    const BindingFlags Private = BindingFlags.NonPublic | BindingFlags.Instance;

    static readonly FieldInfo? ParticleRendererField = typeof(ParticleSceneNode).GetField("particleRenderer", Private);
    static readonly Type? ParticleRendererType = ParticleRendererField?.FieldType;
    static readonly PropertyInfo? SimulationProperty = ParticleRendererType?.GetProperty("Simulation");
    static readonly FieldInfo? RenderersField = ParticleRendererType?.GetField("renderers", Private);
    static readonly FieldInfo? ChildRenderersField = ParticleRendererType?.GetField("childRenderers", Private);
    static readonly MethodInfo? SkippedMethod = ParticleRendererType?.GetMethod("EnumerateSkippedRendererClasses");

    static readonly MethodInfo? PinRandomSeedMethod = typeof(ParticleSystemSimulation).GetMethod("PinRandomSeed", Private);

    static readonly Type? RenderModelsType = typeof(ValveResourceFormat.Renderer.Renderer).Assembly.GetType("ValveResourceFormat.Renderer.Particles.Renderers.RenderModels");
    static readonly FieldInfo? PoolField = RenderModelsType?.GetField("pool", Private);
    static readonly PropertyInfo? PoolColumnProperty = PoolField?.FieldType.GetProperty("Column");
    static readonly FieldInfo? PoolSlotsField = PoolField?.FieldType.GetField("slots", Private);
    static readonly FieldInfo? HandleColumnsField = typeof(ParticleCollection).GetField("handleColumns", Private);

    /// <summary>Whether everything the model material variables need was found.</summary>
    public static bool CanReadModelNodes =>
        ParticleRendererField != null && SimulationProperty != null && RenderersField != null && ChildRenderersField != null
        && RenderModelsType != null && PoolColumnProperty != null && PoolSlotsField != null && HandleColumnsField != null;

    static object? RendererOf(ParticleSceneNode node) => ParticleRendererField?.GetValue(node);

    public static ParticleSystemSimulation? SimulationOf(ParticleSceneNode node)
        => RendererOf(node) is { } renderer ? SimulationProperty?.GetValue(renderer) as ParticleSystemSimulation : null;

    public static IEnumerable<string> SkippedRenderers(ParticleSceneNode node)
        => RendererOf(node) is { } renderer && SkippedMethod?.Invoke(renderer, null) is IEnumerable<string> skipped ? skipped : [];

    /// <summary>Fixes the seed every replay of the simulation starts from. False when it cannot be done.</summary>
    public static bool PinRandomSeed(ParticleSystemSimulation simulation, int seed)
    {
        if (PinRandomSeedMethod == null)
        {
            return false;
        }

        try
        {
            PinRandomSeedMethod.Invoke(simulation, [seed]);
            return true;
        }
        catch (Exception)
        {
            return false;
        }
    }

    /// <summary>One system of a particle node: its simulation and its model renderers, in the order they were built.</summary>
    public sealed record SystemModels(ParticleSystemSimulation Simulation, IReadOnlyList<object> ModelRenderers);

    /// <summary>Every system of the node's tree with the model renderers it built (none when the members are missing).</summary>
    public static List<SystemModels> ModelRenderers(ParticleSceneNode node)
    {
        var result = new List<SystemModels>();
        if (!CanReadModelNodes || RendererOf(node) is not { } root)
        {
            return result;
        }

        void Visit(object renderer, int depth)
        {
            if (depth > 16 || SimulationProperty!.GetValue(renderer) is not ParticleSystemSimulation simulation)
            {
                return;
            }

            var models = new List<object>();
            if (RenderersField!.GetValue(renderer) is IEnumerable renderers)
            {
                foreach (var r in renderers)
                {
                    if (r != null && RenderModelsType!.IsInstanceOfType(r))
                    {
                        models.Add(r);
                    }
                }
            }

            result.Add(new SystemModels(simulation, models));
            if (ChildRenderersField!.GetValue(renderer) is IEnumerable children)
            {
                foreach (var child in children)
                {
                    if (child != null)
                    {
                        Visit(child, depth + 1);
                    }
                }
            }
        }

        Visit(root, 0);
        return result;
    }

    /// <summary>
    /// For each live particle of <paramref name="particles"/> (by index), the model node a model
    /// renderer draws it with, or null. Empty when the renderer has not drawn any particle yet.
    /// </summary>
    public static ModelSceneNode?[] NodesByParticle(object modelRenderer, ParticleCollection particles)
    {
        try
        {
            var pool = PoolField!.GetValue(modelRenderer);
            if (pool == null || PoolColumnProperty!.GetValue(pool) is not int column
                || PoolSlotsField!.GetValue(pool) is not IList slots
                || HandleColumnsField!.GetValue(particles) is not IList columns || column >= columns.Count
                || columns[column] is not int[] handles)
            {
                return [];
            }

            var count = Math.Min(particles.Count, handles.Length);
            var nodes = new ModelSceneNode?[count];
            for (var i = 0; i < count; i++)
            {
                var slot = handles[i] - 1;
                if (slot >= 0 && slot < slots.Count && slots[slot] is ValueTuple<ModelSceneNode?, int, int> entry)
                {
                    nodes[i] = entry.Item1;
                }
            }

            return nodes;
        }
        catch (Exception)
        {
            return [];
        }
    }

    /// <summary>The game's gameinfo.gi read and set on the loader, without mounting its search paths. False when it cannot be done.</summary>
    public static bool SetGameInfo(ValveResourceFormat.IO.GameFileLoader loader, string gameInfoPath)
    {
        var read = typeof(ValveResourceFormat.IO.GameInfo).GetMethod("Read", BindingFlags.NonPublic | BindingFlags.Static);
        var field = typeof(ValveResourceFormat.IO.GameFileLoader).GetField("FoundGameInfo", Private);
        var info = read?.Invoke(null, [gameInfoPath]);
        if (field == null || info == null)
        {
            return false;
        }

        field.SetValue(loader, info);
        return true;
    }
}
