using System.Globalization;
using System.Numerics;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;
using ValveKeyValue;
using ValveResourceFormat.IO;
using ValveResourceFormat.Renderer.SceneNodes;
using ValveResourceFormat.ResourceTypes;
using ValveResourceFormat.Serialization.KeyValues;

namespace Dmb.EffectPlayer;

/// <summary>One particle system of an effect tree with its upgraded data.</summary>
sealed record TreePart(string Path, int Depth, KVObject Data);

/// <summary>
/// Rules that make an effect show in a preview without the game driving it, and the notes a preview
/// owes its viewer about what it cannot show. Each rule is a small table read from the effect's own
/// data, never a list of effect names:
///
/// 1. Play the effect the caller names, which should be a top-level effect, so its children inherit
///    its preview control-point configuration (the caller's job; nothing is changed here).
/// 2. Radius inputs (<see cref="RadiusTargets"/>) that read a control-point component nothing sets
///    get that component set so the mapped value is about <see cref="DefaultRadius"/> units.
/// 3. Path and beam functions (<see cref="PathClasses"/>) whose two ends sit at the same point get the
///    far end moved <see cref="PathLength"/> units along +X.
/// 4. Functions that place on or collide with the ground (<see cref="GroundClasses"/>) get the
///    library's ground plane at z = 0.
/// 5. Projectiles and tracers (<see cref="MovingWords"/> in the file name, or a rope or trail fed by
///    a continuous emitter with no path to follow) get control point 0 moving along +X.
/// </summary>
static class EffectRules
{
    public const float DefaultRadius = 300f;
    public const float PathLength = 400f;

    /// <summary>Number inputs that set a radius or a spread; (class, field).</summary>
    public static readonly (string Class, string Field)[] RadiusTargets =
    [
        ("C_INIT_RingWave", "m_flInitialRadius"),
        ("C_INIT_CreateWithinSphereTransform", "m_fRadiusMax"),
        ("C_INIT_CreateSpiralSphere", "m_flInitialRadius"),
        ("C_INIT_CreateInEpitrochoid", "m_flRadius1"),
        ("C_INIT_CreateInEpitrochoid", "m_flRadius2"),
        ("C_OP_ConstrainDistance", "m_fMaxDistance"),
        // Only when the output field is the radius (3). Compiled files leave the field out when it
        // holds its default; across the game's files the explicit values never include 3, so an
        // absent field is read as the radius.
        ("C_INIT_InitFloat", "m_InputValue"),
        ("C_OP_SetFloat", "m_InputValue"),
    ];

    /// <summary>Functions that lay particles along a path between two control points.</summary>
    public static readonly string[] PathClasses =
    [
        "C_INIT_CreateSequentialPath", "C_INIT_CreateSequentialPathV2", "C_OP_MaintainSequentialPath",
        "C_OP_LockToSavedSequentialPathV2", "C_INIT_CreateAlongPath", "C_INIT_MoveBetweenPoints",
        "C_OP_PercentageBetweenTransforms", "C_OP_DistanceBetweenTransforms",
    ];

    /// <summary>Functions that trace to, collide with or place on the ground.</summary>
    public static readonly string[] GroundClasses =
    [
        "C_INIT_PositionPlaceOnGround", "C_OP_MovementPlaceOnGround", "C_OP_WorldTraceConstraint",
        "C_OP_WorldCollideConstraint",
    ];

    /// <summary>Words in an effect's file name that mark something the game moves (fast for tracers and bullets).</summary>
    public static readonly (string Word, float Speed, float Period)[] MovingWords =
    [
        ("tracer", 3000f, 0.2f),
        ("bullet", 3000f, 0.2f),
        ("projectile", 800f, 0.75f),
        ("missile", 800f, 0.75f),
    ];

    /// <summary>Functions whose control-point index fields name a control point they set, not one they read.</summary>
    public static readonly string[] ControlPointWriters =
    [
        "C_OP_SetControlPoint", "C_OP_SetSingleControlPointPosition", "C_OP_SetChildControlPoints",
        "C_OP_SetPerChildControlPoint", "C_OP_SetCPOrientation", "C_OP_RemapCPtoCP", "C_OP_SetControlPointsToParticle",
        "C_OP_SetControlPointToCenter", "C_OP_SetControlPointFromObjectScale", "C_OP_SetControlPointPositions",
        "C_OP_SetControlPointRotation", "C_OP_SetControlPointOrientation", "C_OP_RampCPLinearRandom",
        "C_OP_SetControlPointsToModelParticles", "C_OP_SetFloatAttributeToVectorExpression",
        "C_OP_SetVectorAttributeToVectorExpression", "C_OP_SetControlPointFieldToScalarExpression",
        "C_OP_SetControlPointFieldFromVectorExpression", "C_OP_SetControlPointToHand", "C_OP_SetControlPointToPlayer",
        "C_OP_SetControlPointToImpactPoint", "C_OP_SetControlPointToWaterSurface", "C_OP_SetCPtoVector",
        "C_OP_DistanceBetweenCPsToCP", "C_OP_RemapControlPointOrientationToRotation", "C_OP_RemapSpeedtoCP",
    ];

    /// <summary>Functions that turn the effect's speed into a value: they only show something while control point 0 moves.</summary>
    public static readonly string[] SpeedClasses = ["C_OP_RemapSpeedtoCP", "C_OP_RemapSpeed"];

    /// <summary>Functions that place particles on a character model.</summary>
    public static readonly string[] ModelClasses =
    [
        "C_INIT_CreateOnModel", "C_INIT_CreateOnModelAtHeight", "C_INIT_SetHitboxToModel", "C_OP_SetControlPointsToModelParticles",
        "C_INIT_RandomNamedModelSequence", "C_OP_ModelCull",
    ];

    /// <summary>Renderers that only give off light.</summary>
    public static readonly string[] LightRenderers =
    [
        "C_OP_RenderDeferredLight", "C_OP_RenderOmni2Light", "C_OP_RenderStandardLight", "C_OP_RenderLightBeam",
        "C_OP_RenderEnvironmentLight",
    ];

    /// <summary>Words in an effect's file name that mark something happening in one place.</summary>
    public static readonly string[] StaticWords =
    [
        "impact", "hit", "explode", "explosion", "muzzle", "flash", "splash", "death", "cast", "precast",
        "debuff", "buff", "glow", "light", "screen",
    ];

    static readonly string[] FunctionLists =
    [
        "m_PreEmissionOperators", "m_Emitters", "m_Initializers", "m_Operators",
        "m_ForceGenerators", "m_Constraints", "m_Renderers",
    ];

    /// <summary>The effect and its enabled children, depth first, each once.</summary>
    public static List<TreePart> Tree(IFileLoader loader, string effect)
    {
        var parts = new List<TreePart>();
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

        void Visit(string path, int depth)
        {
            if (depth > 8 || !seen.Add(path))
            {
                return;
            }

            using var resource = loader.LoadFile(path);
            if (resource?.DataBlock is not ParticleSystem system)
            {
                return;
            }

            parts.Add(new TreePart(path, depth, system.GetUpgradedData()));
            foreach (var child in system.GetChildParticleNames(enabledOnly: true))
            {
                Visit(child.EndsWith("_c", StringComparison.Ordinal) ? child : child + "_c", depth + 1);
            }
        }

        Visit(effect, 0);
        return parts;
    }

    public static IEnumerable<(string List, KVObject Function, string Class)> Functions(TreePart part)
    {
        foreach (var list in FunctionLists)
        {
            foreach (var function in part.Data.GetArray(list) ?? [])
            {
                yield return (list, function, function.GetStringProperty("_class") ?? "?");
            }
        }
    }

    public sealed record RadiusFill(int ControlPoint, int Component, float Value, string Reason, string What = "radius");

    public sealed record PathEnds(int Start, int End, string Reason);

    public sealed record Movement(float Speed, float Period, string Reason);

    /// <summary>What the rules want for one effect tree, and the notes for its viewer.</summary>
    public sealed class Plan
    {
        public List<RadiusFill> Radius { get; } = [];
        public List<PathEnds> Paths { get; } = [];
        public string? Ground { get; set; }
        public Movement? Moving { get; set; }
        public List<string> UntintedModels { get; } = [];
        public int UntintedModelRenderers { get; set; }
        public List<string> Decals { get; } = [];
        public int DecalRenderers { get; set; }
        public List<string> Unsupported { get; } = [];
        public SortedDictionary<int, SortedSet<string>> ControlPointReads { get; } = [];
        public SortedSet<int> WrittenControlPoints { get; } = [];
        public SortedDictionary<string, int> Renderers { get; } = new(StringComparer.Ordinal);
        public string? NeedsModel { get; set; }
        public string? SpeedDriven { get; set; }
        public float? SpriteFadeStart { get; set; }

        /// <summary>What the rules changed on the last node they were applied to.</summary>
        public List<string> Applied { get; } = [];
    }

    public static Plan Analyse(IFileLoader loader, string effect, float radius, IReadOnlyList<TreePart>? tree = null, bool rulesOn = true)
    {
        var plan = new Plan();
        tree ??= Tree(loader, effect);
        string? ropeTrail = null;

        foreach (var part in tree)
        {
            var name = ShortName(part.Path);
            var hasRope = false;
            var hasContinuous = false;
            var hasPath = false;

            foreach (var (list, function, className) in Functions(part))
            {
                if (!IsSupported(list, className))
                {
                    plan.Unsupported.Add($"{className} ({name})");
                }

                if (className == "C_OP_RenderModels" && function.ContainsKey("m_MaterialVars") && (function.GetArray("m_MaterialVars")?.Count ?? 0) > 0)
                {
                    plan.UntintedModelRenderers++;
                    if (!plan.UntintedModels.Contains(part.Path))
                    {
                        plan.UntintedModels.Add(part.Path);
                    }
                }

                if (className == "C_OP_RenderProjected")
                {
                    plan.DecalRenderers++;
                    if (!plan.Decals.Contains(part.Path))
                    {
                        plan.Decals.Add(part.Path);
                    }
                }

                if (GroundClasses.Contains(className))
                {
                    plan.Ground ??= $"{className} in {name}";
                }

                if (list == "m_Renderers")
                {
                    plan.Renderers[className] = plan.Renderers.GetValueOrDefault(className) + 1;

                    // Sprites fade out once they cover this share of the screen; framing keeps them below it.
                    if (className == "C_OP_RenderSprites" && Literal(function, "m_flStartFadeSize") is { } fade && fade > 0f && fade < 1f)
                    {
                        plan.SpriteFadeStart = MathF.Min(plan.SpriteFadeStart ?? 1f, fade);
                    }
                }

                if (ModelClasses.Contains(className))
                {
                    plan.NeedsModel ??= $"{className} in {name}";
                }

                if (SpeedClasses.Contains(className))
                {
                    plan.SpeedDriven ??= $"{className} in {name}";
                }

                if (className is "C_OP_RenderRopes" or "C_OP_RenderTrails")
                {
                    hasRope = true;
                }

                if (className == "C_OP_ContinuousEmitter")
                {
                    hasContinuous = true;
                }

                if (PathClasses.Contains(className))
                {
                    hasPath = true;
                    var (start, end) = PathControlPoints(function, className);
                    if (start != end && !plan.Paths.Any(p => p.Start == start && p.End == end))
                    {
                        plan.Paths.Add(new PathEnds(start, end, $"{className} in {name}"));
                    }
                }
            }

            if (hasRope && hasContinuous && !hasPath)
            {
                ropeTrail ??= name;
            }
        }

        // An effect named for something that happens in one place (an impact, a flash) stays put,
        // even when a tracer or projectile is part of its name.
        var top = Path.GetFileNameWithoutExtension(effect).ToLowerInvariant();
        bool HasWord(string word) => Regex.IsMatch(top, $@"(^|_){word}(_|$)");
        var staticName = StaticWords.FirstOrDefault(HasWord);

        // A rope or trail that is emitted continuously and follows no path only shows a line when
        // the point it is emitted from moves, the way a bullet carries its trail.
        if (ropeTrail != null && plan.Paths.Count == 0 && staticName == null)
        {
            plan.Moving = new Movement(3000f, 0.2f, $"a rope or trail fed by a continuous emitter in {ropeTrail}, with no path to follow");
        }

        // Something that reads the effect's speed only shows while it moves, as a running character does.
        if (plan.SpeedDriven != null && plan.Moving == null)
        {
            plan.Moving = new Movement(500f, 1.2f, $"{plan.SpeedDriven}, which reads how fast the effect moves");
        }

        foreach (var (word, speed, period) in MovingWords)
        {
            if (staticName == null && HasWord(word))
            {
                plan.Moving = new Movement(speed, period, $"\"{word}\" in the effect's name");
                break;
            }
        }

        // Control point reads (all kinds), and the radius rule's candidates.
        foreach (var use in ControlPointScan.Scan(loader, effect))
        {
            if (use.ControlPoint < 0 || use.ControlPoint > 63)
            {
                continue;
            }

            var functionClass = use.Function.Contains(' ', StringComparison.Ordinal) ? use.Function[(use.Function.IndexOf(' ', StringComparison.Ordinal) + 1)..] : use.Function;
            if (use.Kind == "cp-index-field" && ControlPointWriters.Any(w => functionClass.StartsWith(w, StringComparison.Ordinal)))
            {
                // A function that sets this control point: the effect drives it itself.
                plan.WrittenControlPoints.Add(use.ControlPoint);
                continue;
            }

            var kind = use.Kind == "number-from-cp-component" ? "number" : use.Kind == "cp-index-field" ? "position" : "value";
            if (use.ControlPoint > 0 || kind == "number")
            {
                if (!plan.ControlPointReads.TryGetValue(use.ControlPoint, out var reads))
                {
                    plan.ControlPointReads[use.ControlPoint] = reads = new SortedSet<string>(StringComparer.Ordinal);
                }

                reads.Add($"{kind}: {functionClass} {use.Field} ({ShortName(use.File)})");
            }
        }

        if (rulesOn)
        {
            foreach (var part in tree)
            {
                foreach (var (_, function, className) in Functions(part))
                {
                    foreach (var (targetClass, field) in RadiusTargets)
                    {
                        if (className != targetClass || !function.ContainsKey(field))
                        {
                            continue;
                        }

                        // Radius (3) gets the radius; alpha (7) gets full strength, 1, since an
                        // undriven strength input left at 0 hides the part completely.
                        var wanted = radius;
                        var what = "radius";
                        if (className is "C_INIT_InitFloat" or "C_OP_SetFloat")
                        {
                            var output = function.GetInt32Property("m_nOutputField", 3);
                            if (output == 7)
                            {
                                wanted = 1f;
                                what = "alpha";
                            }
                            else if (output != 3)
                            {
                                continue;
                            }
                        }

                        var input = function.GetSubCollection(field);
                        if (input == null || input.GetStringProperty("m_nType") != "PF_TYPE_CONTROL_POINT_COMPONENT")
                        {
                            continue;
                        }

                        var cp = input.GetInt32Property("m_nControlPoint", 0);
                        var component = Math.Clamp(input.GetInt32Property("m_nVectorComponent", 0), 0, 2);
                        // A control point some function of the tree sets (often a parent on its
                        // children) is driven, even when the top system's copy stays at zero.
                        if (cp <= 0 || plan.WrittenControlPoints.Contains(cp) || plan.Radius.Any(r => r.ControlPoint == cp && r.Component == component))
                        {
                            continue;
                        }

                        plan.Radius.Add(new RadiusFill(cp, component, Invert(input, wanted), $"{className} {field} in {ShortName(part.Path)}", what));
                    }
                }
            }
        }
        else
        {
            plan.Paths.Clear();
            plan.Ground = null;
            plan.Moving = null;
        }

        return plan;
    }

    /// <summary>A number field that is a plain value or a literal number input; null otherwise.</summary>
    static float? Literal(KVObject function, string key)
    {
        if (!function.ContainsKey(key))
        {
            return null;
        }

        var value = function[key];
        if (value.IsCollection)
        {
            var input = function.GetSubCollection(key);
            return input?.GetStringProperty("m_nType") == "PF_TYPE_LITERAL" && input.ContainsKey("m_flLiteralValue") ? input.GetFloatProperty("m_flLiteralValue") : null;
        }

        try
        {
            return function.GetFloatProperty(key);
        }
        catch (Exception)
        {
            return null;
        }
    }

    /// <summary>The control point value that makes a number input come out at <paramref name="wanted"/>.</summary>
    static float Invert(KVObject input, float wanted)
    {
        var mapType = input.GetStringProperty("m_nMapType") ?? "PF_MAP_TYPE_DIRECT";
        float F(string key, float fallback) => input.ContainsKey(key) ? input.GetFloatProperty(key) : fallback;

        switch (mapType)
        {
            case "PF_MAP_TYPE_MULT":
                var factor = F("m_flMultFactor", 1f);
                return MathF.Abs(factor) > 1e-6f ? wanted / factor : wanted;
            case "PF_MAP_TYPE_REMAP":
            case "PF_MAP_TYPE_REMAP_BIASED":
                var in0 = F("m_flInput0", 0f);
                var in1 = F("m_flInput1", 1f);
                var out0 = F("m_flOutput0", 0f);
                var out1 = F("m_flOutput1", 1f);
                if (MathF.Abs(out1 - out0) < 1e-6f)
                {
                    return in1;
                }

                var value = in0 + (wanted - out0) * (in1 - in0) / (out1 - out0);
                var lo = MathF.Min(in0, in1);
                var hi = MathF.Max(in0, in1);
                value = Math.Clamp(value, lo, hi);
                return value == 0f ? (in1 != 0f ? in1 : in0) : value;
            default:
                return wanted;
        }
    }

    static (int Start, int End) PathControlPoints(KVObject function, string className)
    {
        var path = function.GetSubCollection("m_PathParams");
        if (path != null)
        {
            return (path.GetInt32Property("m_nStartControlPointNumber", 0), path.GetInt32Property("m_nEndControlPointNumber", 0));
        }

        if (className == "C_INIT_MoveBetweenPoints")
        {
            return (0, function.GetInt32Property("m_nEndControlPointNumber", 0));
        }

        var start = function.GetSubCollection("m_TransformStart")?.GetInt32Property("m_nControlPoint", 0) ?? 0;
        var end = function.GetSubCollection("m_TransformEnd")?.GetInt32Property("m_nControlPoint", 1) ?? 1;
        return (start, end);
    }

    /// <summary>
    /// Applies rules 2 and 3 to a freshly built node (its preview drivers and the library's defaults
    /// already in place). Returns what was changed.
    /// </summary>
    public static List<string> ApplyControlPoints(Plan plan, ParticleSceneNode node)
    {
        var applied = new List<string>();

        foreach (var fill in plan.Radius)
        {
            var point = node.GetControlPoint(fill.ControlPoint);
            var position = point.Position;
            if (Component(position, fill.Component) != 0f)
            {
                continue;
            }

            point.Position = WithComponent(position, fill.Component, fill.Value);
            applied.Add(FormattableString.Invariant($"{fill.What}: control point {fill.ControlPoint} {"xyz"[fill.Component]} set to {fill.Value:0.###} for {fill.Reason}"));
        }

        foreach (var path in plan.Paths)
        {
            var start = node.GetControlPoint(path.Start).Position;
            var endPoint = node.GetControlPoint(path.End);
            if (Vector3.Distance(start, endPoint.Position) >= 1f)
            {
                continue;
            }

            // The far end is the one that is not control point 0, which the effect is placed by.
            var far = path.End != 0 ? endPoint : node.GetControlPoint(path.Start);
            var near = path.End != 0 ? start : endPoint.Position;
            far.Position = near + new Vector3(PathLength, 0, 0);
            applied.Add(FormattableString.Invariant($"path: control point {(path.End != 0 ? path.End : path.Start)} placed {PathLength:0} units along +X for {path.Reason}"));
        }

        return applied;
    }

    static float Component(Vector3 v, int c) => c switch { 0 => v.X, 1 => v.Y, _ => v.Z };

    static Vector3 WithComponent(Vector3 v, int c, float value) => c switch
    {
        0 => v with { X = value },
        1 => v with { Y = value },
        _ => v with { Z = value },
    };

    public static string ShortName(string path) => Path.GetFileNameWithoutExtension(path);

    static bool IsSupported(string list, string className)
    {
        try
        {
            return list switch
            {
                "m_PreEmissionOperators" => ValveResourceFormat.Particles.ParticleSupportInfo.IsPreEmissionOperatorSupported(className),
                "m_Emitters" => ValveResourceFormat.Particles.ParticleSupportInfo.IsEmitterSupported(className),
                "m_Initializers" => ValveResourceFormat.Particles.ParticleSupportInfo.IsInitializerSupported(className),
                "m_Operators" => ValveResourceFormat.Particles.ParticleSupportInfo.IsOperatorSupported(className),
                "m_ForceGenerators" => ValveResourceFormat.Particles.ParticleSupportInfo.IsForceGeneratorSupported(className),
                "m_Constraints" => ValveResourceFormat.Particles.ParticleSupportInfo.IsConstraintSupported(className),
                _ => true, // renderers: the node reports the ones it skipped
            };
        }
        catch
        {
            return true;
        }
    }

    public static JsonArray ToArray(IEnumerable<string> values)
    {
        var array = new JsonArray();
        foreach (var v in values)
        {
            array.Add((JsonNode?)v);
        }

        return array;
    }

    public static string F(float v) => v.ToString("0.###", CultureInfo.InvariantCulture);
}
