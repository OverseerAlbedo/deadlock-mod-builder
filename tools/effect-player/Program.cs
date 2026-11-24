using System.Diagnostics;
using System.Globalization;
using System.Numerics;
using System.Reflection;
using System.Text.Json;
using System.Text.Json.Nodes;
using Microsoft.Extensions.Logging;
using OpenTK.Graphics.OpenGL;
using SkiaSharp;
using ValvePak;
using ValveResourceFormat.Renderer;
using ValveResourceFormat.Renderer.SceneNodes;
using ValveResourceFormat.ResourceTypes;
using ValveResourceFormat.Serialization.KeyValues;

namespace Dmb.EffectPlayer;

static class ExitCodes
{
    public const int Ok = 0;
    public const int Failure = 1;
    public const int Usage = 2;
    public const int OpenGLUnavailable = 3;
    public const int EffectNotFound = 4;
}

static class Program
{
    const string Usage = """
        effect-player serve --pak <pak01_dir.vpk> [--size 640x480] [--fps 30] [--quality 80] [--msaa 4]
            [--background 0b0c10] [--exposure <stops>] [--verbose]
            (commands on stdin, frames and status on stdout; see README)
        effect-player render --pak <pak01_dir.vpk> --effect <particles/...vpcf_c> --out <folder>
            [--overlay <vpk>] [--size 640x480] [--fps 30] [--seconds 3] [--speed 1]
            [--background 0b0c10] [--exposure <stops>] [--gamma 2.2] [--msaa 4]
            [--frame auto|fixed] [--yaw <deg>] [--pitch <deg>] [--distance <units>] [--target x,y,z]
            [--pad 1.15] [--preroll <seconds>] [--cp N=x,y,z]... [--cp-velocity N=x,y,z]... [--cp-period 1] [--no-sun] [--show-model] [--mode preview|game] [--ground <z>] [--serial] [--no-frames]
            [--stream-cost] [--quality 80] [--rules] [--radius 300]
        effect-player inspect --pak <pak01_dir.vpk> --effect <path> [--overlay <vpk>]
        effect-player scan --pak <pak01_dir.vpk> --prefix <particles/abilities/ghost/> --out <file.json>
        effect-player fields --pak <pak01_dir.vpk> [--overlay <vpk>] [--prefix particles/] --class <C_OP_...,...> --key <m_...,...>
        effect-player models --pak <pak01_dir.vpk> --effect <path> [--overlay <vpk>]
        effect-player compare --pak <pak01_dir.vpk> --overlay <vpk> [--verbose]
        effect-player image-stats --out <folder of frames> [--background 0b0c10]
        effect-player --version

        Exit codes: 0 ok, 1 failure, 2 usage, 3 OpenGL 4.6 not available, 4 effect not found.
        """;

    static int Main(string[] args)
    {
        if (args.Length == 1 && args[0] == "--version")
        {
            Console.WriteLine($"effect-player {typeof(Program).Assembly.GetName().Version} (ValveResourceFormat {typeof(Renderer).Assembly.GetName().Version})");
            return ExitCodes.Ok;
        }

        if (args.Length == 0)
        {
            Console.Error.WriteLine(Usage);
            return ExitCodes.Usage;
        }

        if (args[0] == "serve")
        {
            return Server.Run(args[1..]);
        }

        Options options;
        try
        {
            options = Options.Parse(args[1..], needsPak: args[0] != "image-stats");
        }
        catch (ArgumentException e)
        {
            Console.Error.WriteLine(e.Message);
            Console.Error.WriteLine(Usage);
            return ExitCodes.Usage;
        }

        try
        {
            return args[0] switch
            {
                "render" => Render(options),
                "inspect" => Inspect(options),
                "scan" => ScanPrefix(options),
                "fields" => FieldValues(options),
                "models" => ModelMaterials(options),
                "compare" => CompareOverlay(options),
                "image-stats" => ImageStats(options),
                _ => UsageError($"Unknown command {args[0]}."),
            };
        }
        catch (OpenGLUnavailableException e)
        {
            Console.Error.WriteLine(e.Message);
            return ExitCodes.OpenGLUnavailable;
        }
        catch (EffectNotFoundException e)
        {
            Console.Error.WriteLine(e.Message);
            return ExitCodes.EffectNotFound;
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"The effect player failed: {e.GetType().Name}: {e.Message}");
            return ExitCodes.Failure;
        }
    }

    static int UsageError(string message)
    {
        Console.Error.WriteLine(message);
        Console.Error.WriteLine(Usage);
        return ExitCodes.Usage;
    }

    sealed class EffectNotFoundException(string message) : Exception(message);

    static ParticleSystem LoadEffect(EffectLoader loader, string effect, out ValveResourceFormat.Resource resource)
    {
        resource = loader.LoadFile(effect) ?? throw new EffectNotFoundException($"The effect {effect} was not found in the packages.");
        return resource.DataBlock as ParticleSystem ?? throw new EffectNotFoundException($"{effect} is not a particle system.");
    }

    static Package? OpenOverlay(string? path)
    {
        if (path == null)
        {
            return null;
        }

        var package = new Package();
        package.Read(path);
        return package;
    }

    static int Inspect(Options o)
    {
        var logger = new CollectingLogger(o.Verbose);
        using var loader = new EffectLoader(OpenOverlay(o.Overlay), o.Pak, logger);
        var system = LoadEffect(loader, o.Effect, out var resource);
        using (resource)
        {
            var result = new JsonObject
            {
                ["effect"] = o.Effect,
                ["configurations"] = ControlPointScan.Configurations(system),
                ["controlPointUses"] = ControlPointScan.ToJson(ControlPointScan.Scan(loader, o.Effect)),
                ["resolved"] = Resolved(loader),
            };
            Console.WriteLine(result.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        }

        return ExitCodes.Ok;
    }

    /// <summary>Lists every effect under a path prefix with the control point inputs it reads.</summary>
    static int ScanPrefix(Options o)
    {
        var logger = new CollectingLogger(o.Verbose);
        using var loader = new EffectLoader(null, o.Pak, logger);
        using var package = new Package();
        package.Read(o.Pak);

        var result = new JsonArray();
        if (package.Entries != null && package.Entries.TryGetValue("vpcf_c", out var entries))
        {
            foreach (var entry in entries.OrderBy(e => e.GetFullPath(), StringComparer.Ordinal))
            {
                var path = entry.GetFullPath();
                if (!path.StartsWith(o.Prefix ?? "particles/", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                var uses = ControlPointScan.Scan(loader, path, new HashSet<string>(StringComparer.OrdinalIgnoreCase), 0, followChildren: false);
                using var res = loader.LoadFile(path);
                var classes = new JsonArray();
                var children = new JsonArray();
                var configurations = new JsonArray();
                if (res?.DataBlock is ParticleSystem system)
                {
                    foreach (var child in system.GetChildParticleNames(enabledOnly: true))
                    {
                        children.Add(child.EndsWith("_c", StringComparison.Ordinal) ? child : child + "_c");
                    }

                    configurations = ControlPointScan.Configurations(system);

                    foreach (var list in new[] { "m_Initializers", "m_Operators", "m_Emitters", "m_Renderers" })
                    {
                        foreach (var f in system.GetUpgradedData().GetArray(list) ?? [])
                        {
                            classes.Add(f.GetStringProperty("_class") ?? "?");
                        }
                    }
                }

                result.Add(new JsonObject
                {
                    ["effect"] = path,
                    ["classes"] = classes,
                    ["children"] = children,
                    ["configurations"] = configurations,
                    ["uses"] = ControlPointScan.ToJson(uses),
                });
            }
        }

        File.WriteAllText(o.Out, result.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        Console.WriteLine($"{result.Count} effects written to {o.Out}");
        return ExitCodes.Ok;
    }

    /// <summary>
    /// Counts the values one key takes in every particle function of the given classes across a
    /// package. Compiled files appear to leave out keys that hold their default, so "absent" usually
    /// means the default value.
    /// </summary>
    static int FieldValues(Options o)
    {
        var logger = new CollectingLogger(o.Verbose);
        using var loader = new EffectLoader(OpenOverlay(o.Overlay), o.Pak, logger);
        using var package = new Package();
        package.Read(o.Overlay ?? o.Pak);

        var classes = (o.Classes ?? "").Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).ToHashSet(StringComparer.Ordinal);
        var keys = (o.Keys ?? "").Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        var counts = new SortedDictionary<string, int>(StringComparer.Ordinal);
        var files = 0;

        if (package.Entries != null && package.Entries.TryGetValue("vpcf_c", out var entries))
        {
            foreach (var entry in entries)
            {
                var path = entry.GetFullPath();
                if (!path.StartsWith(o.Prefix ?? "particles/", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                using var res = loader.LoadFile(path);
                if (res?.DataBlock is not ParticleSystem system)
                {
                    continue;
                }

                files++;
                var data = system.GetUpgradedData();
                foreach (var list in new[] { "m_PreEmissionOperators", "m_Emitters", "m_Initializers", "m_Operators", "m_ForceGenerators", "m_Constraints", "m_Renderers" })
                {
                    foreach (var f in data.GetArray(list) ?? [])
                    {
                        var name = f.GetStringProperty("_class") ?? "?";
                        if (classes.Count > 0 && !classes.Contains(name))
                        {
                            continue;
                        }

                        foreach (var key in keys)
                        {
                            string value;
                            if (!f.ContainsKey(key))
                            {
                                value = "absent";
                            }
                            else if (f[key].IsArray)
                            {
                                value = Describe(f[key]);
                            }
                            else if (f[key].IsCollection)
                            {
                                var sub = f.GetSubCollection(key);
                                value = sub != null && sub.ContainsKey("m_nType")
                                    ? $"{sub.GetStringProperty("m_nType")} {(sub.ContainsKey("m_flLiteralValue") ? sub.GetFloatProperty("m_flLiteralValue").ToString("0.###", CultureInfo.InvariantCulture) : "")}".Trim()
                                    : "collection";
                            }
                            else
                            {
                                value = Convert.ToString(f[key].ToString(), CultureInfo.InvariantCulture) ?? "?";
                            }

                            var bucket = $"{name}\t{key}\t{value}";
                            counts[bucket] = counts.GetValueOrDefault(bucket) + 1;
                        }
                    }
                }
            }
        }

        Console.WriteLine($"files\t{files}");
        foreach (var (bucket, count) in counts)
        {
            Console.WriteLine($"{bucket}\t{count}");
        }

        return ExitCodes.Ok;
    }

    /// <summary>
    /// For every model renderer of an effect tree: its material variables, its models and the
    /// materials they draw with (or the override material), with each material's shader, its
    /// dynamic expressions and the colour parameters it holds.
    /// </summary>
    static int ModelMaterials(Options o)
    {
        var logger = new CollectingLogger(o.Verbose);
        using var loader = new EffectLoader(OpenOverlay(o.Overlay), o.Pak, logger);
        var parts = new JsonArray();
        IEnumerable<TreePart> Files()
        {
            if (!string.IsNullOrEmpty(o.Effect))
            {
                foreach (var part in EffectRules.Tree(loader, o.Effect))
                {
                    yield return part;
                }

                yield break;
            }

            // Without an effect: every particle file under the prefix, each on its own.
            using var package = new Package();
            package.Read(o.Overlay ?? o.Pak);
            foreach (var entry in package.Entries?.GetValueOrDefault("vpcf_c") ?? [])
            {
                var path = entry.GetFullPath();
                if (!path.StartsWith(o.Prefix ?? "particles/", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                using var resource = loader.LoadFile(path);
                if (resource?.DataBlock is ParticleSystem system)
                {
                    yield return new TreePart(path, 0, system.GetUpgradedData());
                }
            }
        }

        foreach (var part in Files())
        {
            foreach (var (list, function, className) in EffectRules.Functions(part))
            {
                if (className != "C_OP_RenderModels")
                {
                    continue;
                }

                var materials = new SortedSet<string>(StringComparer.OrdinalIgnoreCase);
                var models = new JsonArray();
                foreach (var entry in function.GetArray("m_ModelList") ?? [])
                {
                    var path = entry.GetStringProperty("m_model");
                    if (string.IsNullOrEmpty(path))
                    {
                        continue;
                    }

                    models.Add(path);
                    using var model = loader.LoadFileCompiled(path);
                    foreach (var reference in model?.ExternalReferences?.ResourceRefInfoList ?? [])
                    {
                        if (reference.Name.EndsWith(".vmat", StringComparison.OrdinalIgnoreCase))
                        {
                            materials.Add(reference.Name);
                        }
                    }
                }

                var overrideMaterial = function.GetStringProperty("m_hOverrideMaterial");
                var described = new JsonObject();
                foreach (var name in string.IsNullOrEmpty(overrideMaterial) ? materials : [overrideMaterial])
                {
                    using var resource = loader.LoadFileCompiled(name);
                    if (resource?.DataBlock is not Material material)
                    {
                        described[name] = "not found";
                        continue;
                    }

                    var expressions = new JsonObject();
                    foreach (var (param, expression) in material.DynamicExpressions)
                    {
                        expressions[param] = expression;
                    }

                    var vectors = new JsonObject();
                    foreach (var (param, value) in material.VectorParams)
                    {
                        vectors[param] = FormattableString.Invariant($"{value.X:0.###},{value.Y:0.###},{value.Z:0.###},{value.W:0.###}");
                    }

                    var floats = new JsonObject();
                    foreach (var (param, value) in material.FloatParams)
                    {
                        floats[param] = Math.Round(value, 4);
                    }

                    var ints = new JsonObject();
                    foreach (var (param, value) in material.IntParams)
                    {
                        ints[param] = value;
                    }

                    var textures = new JsonObject();
                    foreach (var (param, value) in material.TextureParams)
                    {
                        textures[param] = value;
                    }

                    described[name] = new JsonObject
                    {
                        ["shader"] = material.ShaderName,
                        ["ints"] = ints,
                        ["textures"] = textures,
                        ["dynamic"] = expressions,
                        ["renderAttributesUsed"] = EffectRules.ToArray(material.RenderAttributesUsed),
                        ["vectors"] = vectors,
                        ["floats"] = floats,
                    };
                }

                parts.Add(new JsonObject
                {
                    ["part"] = part.Path,
                    ["materialVars"] = function.ContainsKey("m_MaterialVars") ? Describe(function["m_MaterialVars"]) : null,
                    ["colorBlend"] = function.GetStringProperty("m_nColorBlendType"),
                    ["suppressTint"] = function.ContainsKey("m_bSuppressTint") ? Describe(function["m_bSuppressTint"]) : null,
                    ["override"] = overrideMaterial,
                    ["models"] = models,
                    ["materials"] = described,
                });
            }
        }

        Console.WriteLine(new JsonObject { ["effect"] = o.Effect, ["renderers"] = parts }.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        return ExitCodes.Ok;
    }

    /// <summary>A value written out in one line: arrays in brackets, collections as key=value pairs in braces.</summary>
    static string Describe(ValveKeyValue.KVObject value, int depth = 0)
    {
        if (depth > 6)
        {
            return "...";
        }

        if (value.IsArray)
        {
            var items = new List<string>();
            for (var n = 0; n < value.Count; n++)
            {
                items.Add(Describe(value[n], depth + 1));
            }

            return $"[{string.Join(",", items)}]";
        }

        if (value.IsCollection)
        {
            var pairs = new List<string>();
            foreach (var (name, child) in value)
            {
                pairs.Add($"{name}={Describe(child, depth + 1)}");
            }

            return $"{{{string.Join(" ", pairs)}}}";
        }

        return Convert.ToString(value.ToString(), CultureInfo.InvariantCulture) ?? "?";
    }

    /// <summary>
    /// Compares the parsed data of every particle system in the overlay with the game's copy of the
    /// same path, as the library reads both.
    /// </summary>
    static int CompareOverlay(Options o)
    {
        if (o.Overlay == null)
        {
            return UsageError("compare needs --overlay.");
        }

        var logger = new CollectingLogger(o.Verbose);
        using var game = new EffectLoader(null, o.Pak, logger);
        using var edited = new EffectLoader(OpenOverlay(o.Overlay), o.Pak, logger);
        using var overlay = new Package();
        overlay.Read(o.Overlay);

        var same = 0;
        var different = 0;
        if (overlay.Entries != null && overlay.Entries.TryGetValue("vpcf_c", out var entries))
        {
            foreach (var entry in entries.OrderBy(e => e.GetFullPath(), StringComparer.Ordinal))
            {
                var path = entry.GetFullPath();
                using var a = game.LoadFile(path);
                using var b = edited.LoadFile(path);
                var source = edited.Resolved.TryGetValue(path, out var s) ? s : "?";
                var textA = a?.DataBlock?.ToString() ?? "";
                var textB = b?.DataBlock?.ToString() ?? "";
                var linesA = textA.Split('\n');
                var linesB = textB.Split('\n');
                var changed = 0;
                for (var i = 0; i < Math.Max(linesA.Length, linesB.Length); i++)
                {
                    if (i >= linesA.Length || i >= linesB.Length || linesA[i] != linesB[i])
                    {
                        changed++;
                    }
                }

                if (changed == 0)
                {
                    same++;
                }
                else
                {
                    different++;
                }

                Console.WriteLine($"{(changed == 0 ? "same" : "DIFFERENT")}\t{changed}\t{linesA.Length}\t{linesB.Length}\t{source}\t{path}");
                if (changed > 0 && o.Verbose)
                {
                    for (var i = 0; i < Math.Max(linesA.Length, linesB.Length) && i < 4000; i++)
                    {
                        var la = i < linesA.Length ? linesA[i] : "<none>";
                        var lb = i < linesB.Length ? linesB[i] : "<none>";
                        if (la != lb)
                        {
                            Console.WriteLine($"   game: {la.Trim()}\n   over: {lb.Trim()}");
                        }
                    }
                }
            }
        }

        Console.WriteLine($"{same} same, {different} different");
        foreach (var (level, message) in logger.Messages)
        {
            if (level >= LogLevel.Warning)
            {
                Console.WriteLine($"{level}: {message}");
            }
        }

        return ExitCodes.Ok;
    }

    /// <summary>
    /// Colour and size of what differs from the background in each PNG frame of a folder: pixel
    /// count, summed brightness above the background, the brightness-weighted mean hue of the
    /// covered pixels and the mean colour of the brightest 2% of them.
    /// </summary>
    static int ImageStats(Options o)
    {
        var background = o.Background;
        var rows = new JsonArray();
        foreach (var file in Directory.GetFiles(o.Out, "frame_*.png").OrderBy(f => f, StringComparer.Ordinal))
        {
            using var bitmap = SKBitmap.Decode(file);
            var covered = new List<(float R, float G, float B, float L)>();
            double lift = 0, hx = 0, hy = 0;
            for (var y = 0; y < bitmap.Height; y++)
            {
                for (var x = 0; x < bitmap.Width; x++)
                {
                    var c = bitmap.GetPixel(x, y);
                    if (Math.Abs(c.Red - background.R) <= 12 && Math.Abs(c.Green - background.G) <= 12 && Math.Abs(c.Blue - background.B) <= 12)
                    {
                        continue;
                    }

                    var l = 0.2126f * c.Red + 0.7152f * c.Green + 0.0722f * c.Blue;
                    covered.Add((c.Red, c.Green, c.Blue, l));
                    lift += Math.Max(0, l - (0.2126 * background.R + 0.7152 * background.G + 0.0722 * background.B));
                    c.ToHsv(out var h, out var sat, out var v);
                    var weight = sat / 100.0 * v / 100.0;
                    hx += Math.Cos(h * Math.PI / 180) * weight;
                    hy += Math.Sin(h * Math.PI / 180) * weight;
                }
            }

            var row = new JsonObject { ["frame"] = Path.GetFileName(file), ["pixels"] = covered.Count, ["brightnessSum"] = Math.Round(lift, 0) };
            if (covered.Count > 0)
            {
                var hue = Math.Atan2(hy, hx) * 180 / Math.PI;
                row["meanHue"] = Math.Round(hue < 0 ? hue + 360 : hue, 1);
                var top = covered.OrderByDescending(p => p.L).Take(Math.Max(1, covered.Count / 50)).ToList();
                var r = (byte)top.Average(p => p.R);
                var g = (byte)top.Average(p => p.G);
                var b = (byte)top.Average(p => p.B);
                new SKColor(r, g, b).ToHsv(out var th, out var ts, out var tv);
                row["brightestRgb"] = $"{r:x2}{g:x2}{b:x2}";
                row["brightestHue"] = Math.Round(th, 1);
                row["brightestSat"] = Math.Round(ts, 1);
            }

            rows.Add(row);
        }

        Console.WriteLine(rows.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        return ExitCodes.Ok;
    }

    static JsonObject Resolved(EffectLoader loader)
    {
        var resolved = new JsonObject();
        foreach (var (file, source) in loader.Resolved.OrderBy(p => p.Key, StringComparer.Ordinal))
        {
            resolved[file] = source;
        }

        return resolved;
    }

    static int Render(Options o)
    {
        var total = Stopwatch.StartNew();
        var logger = new CollectingLogger(o.Verbose);

        Directory.CreateDirectory(o.Out);

        using var loader = new EffectLoader(OpenOverlay(o.Overlay), o.Pak, logger);
        var system = LoadEffect(loader, o.Effect, out var effectResource);
        var loadedAt = total.Elapsed.TotalMilliseconds;

        using var host = new RenderHost(loader, logger, o.Width, o.Height, o.GLVersion, o.Msaa);
        var contextAt = total.Elapsed.TotalMilliseconds;

        host.Renderer.ParallelSimulation = !o.Serial;
        if (o.Ground is { } ground)
        {
            // The particle viewer's "Collide With Floor": a ground plane for effects that trace or place on the ground.
            host.Renderer.EntitySystem.PhysicsWorld.GroundPlane = ground;
        }
        host.Renderer.Postprocess.ExposureCompensation = o.Exposure;
        host.Renderer.Postprocess.FullScreenGamma = o.Gamma;

        if (!o.NoSun)
        {
            host.LoadSunLighting();
        }

        host.Renderer.Scene.Initialize();

        // Background: find the linear clear colour whose tone-mapped output is the requested colour.
        host.Renderer.Camera.SetLocation(new Vector3(200, 200, 200));
        host.Renderer.Camera.LookAt(Vector3.Zero);
        var background = CalibrateBackground(host, o.Background);
        var backgroundAt = total.Elapsed.TotalMilliseconds;

        var node = new ParticleSceneNode(host.Renderer.Scene, system, null, preview: o.Mode == "preview")
        {
            Transform = Matrix4x4.Identity,
            FrametimeMultiplier = o.Speed,
        };
        host.Renderer.Scene.Add(node, true);

        // The preview configuration may bring a model (a hero holding the effect at an attachment).
        // It keeps animating and placing the effect; only its drawing is switched off unless asked for.
        var previewModel = node.PreviewModel;
        if (previewModel != null && !o.ShowModel)
        {
            previewModel.Visible = false;
        }

        foreach (var (index, position) in o.ControlPoints)
        {
            node.GetControlPoint(index).Position = position;
        }

        // The preview rules of serve (see EffectRules), for trying them one effect at a time.
        var rulesApplied = new JsonArray();
        if (o.Rules)
        {
            var plan = EffectRules.Analyse(loader, o.Effect, o.Radius);
            foreach (var applied in EffectRules.ApplyControlPoints(plan, node))
            {
                rulesApplied.Add(applied);
            }

            if (plan.Ground != null && o.Ground == null)
            {
                host.Renderer.EntitySystem.PhysicsWorld.GroundPlane = 0f;
                rulesApplied.Add($"ground: {plan.Ground}");
            }

            if (plan.Moving is { } moving && o.ControlPointVelocities.Count == 0)
            {
                var start = new Vector3(-moving.Speed * moving.Period * 0.5f, 0, 0);
                o.ControlPoints.RemoveAll(c => c.Index == 0);
                o.ControlPoints.Add((0, start));
                o.ControlPointVelocities.Add((0, new Vector3(moving.Speed, 0, 0)));
                o.ControlPointPeriod = moving.Period;
                node.GetControlPoint(0).Position = start;
                rulesApplied.Add($"moving: {moving.Reason}");
            }
        }

        var timestep = 1f / o.Fps;
        var frameCount = Math.Max(1, (int)MathF.Round(o.Seconds * o.Fps));
        var simulation = SimulationOf(node);

        // Pre-roll: run the effect once over its life and collect the bounds its particles reach.
        var preroll = Stopwatch.StartNew();
        var prerollSteps = (int)MathF.Ceiling(o.Preroll * o.Fps);
        var prerollBounds = Bounds.Empty;
        var prerollMaxParticles = 0;
        var cpAfterPreroll = new JsonObject();

        var prerollParticleBounds = Bounds.Empty;

        for (var i = 0; i < prerollSteps; i++)
        {
            MoveControlPoints(node, o, (i + 1) * timestep);
            host.Update(timestep); LoopIfFinished(node, o);
            prerollBounds = prerollBounds.Add(node);
            prerollParticleBounds = Bounds.AddParticles(prerollParticleBounds, simulation);
            prerollMaxParticles = Math.Max(prerollMaxParticles, CountParticles(simulation));
        }

        foreach (var cp in UsedControlPoints(simulation))
        {
            var p = node.GetControlPoint(cp).Position;
            cpAfterPreroll[cp.ToString(CultureInfo.InvariantCulture)] = Vec(p);
        }

        // "bounds" frames the particles themselves (positions grown by radius); "node" frames the scene
        // node's box, which adds each system's authored bounding box padding; "auto" starts from the
        // particle box and then fits the camera to the pixels the effect actually covers.
        var framed = o.Frame == "node" || prerollParticleBounds.IsEmpty ? prerollBounds : prerollParticleBounds;
        PlaceCamera(host, o, framed, node.HasInfiniteBounds);
        var camera = host.Renderer.Camera;
        var fitPasses = new JsonArray();

        if (o.Frame == "auto" && o.Distance == null)
        {
            var scratch = new byte[o.Width * o.Height * 4];
            for (var pass = 0; pass < 2; pass++)
            {
                node.Restart();
                foreach (var (index, position) in o.ControlPoints)
                {
                    node.GetControlPoint(index).Position = position;
                }

                var box = PixelBox.Empty;
                for (var i = 0; i < prerollSteps; i++)
                {
                    MoveControlPoints(node, o, (i + 1) * timestep);
                    host.Update(timestep); LoopIfFinished(node, o);
                    if (i % 2 == 1)
                    {
                        host.Draw();
                        host.ReadBack(scratch);
                        box = box.Add(scratch, o.Width, o.Height, background.Achieved);
                    }
                }

                var fit = FitToPixels(camera, box, o.Width, o.Height, 0.8f);
                fitPasses.Add(fit);
                if (box.IsEmpty)
                {
                    break;
                }
            }
        }

        var prerollMs = preroll.Elapsed.TotalMilliseconds;

        node.Restart();

        foreach (var (index, position) in o.ControlPoints)
        {
            node.GetControlPoint(index).Position = position;
        }

        var pixels = new byte[o.Width * o.Height * 4];
        var frames = new JsonArray();
        var simulate = new List<double>();
        var draw = new List<double>();
        var readback = new List<double>();
        var encode = new List<double>();
        var streamCost = o.StreamCost ? new StreamCost(o.Quality) : null;
        var renderBounds = Bounds.Empty;
        double? firstFrameAt = null;
        double? firstVisibleFrameAt = null;
        var firstVisibleFrame = -1;
        var maxParticles = 0;
        var sw = new Stopwatch();

        for (var frame = 0; frame < frameCount; frame++)
        {
            MoveControlPoints(node, o, (frame + 1) * timestep);
            sw.Restart();
            host.Update(timestep); LoopIfFinished(node, o);
            simulate.Add(sw.Elapsed.TotalMilliseconds);

            sw.Restart();
            host.Draw();
            GL.Finish();
            draw.Add(sw.Elapsed.TotalMilliseconds);

            sw.Restart();
            host.ReadBack(pixels);
            readback.Add(sw.Elapsed.TotalMilliseconds);

            var coverage = Coverage(pixels, background.Achieved);
            firstFrameAt ??= total.Elapsed.TotalMilliseconds;
            if (coverage > 0 && firstVisibleFrameAt == null)
            {
                firstVisibleFrameAt = total.Elapsed.TotalMilliseconds;
                firstVisibleFrame = frame;
            }

            sw.Restart();
            if (!o.NoFrames)
            {
                File.WriteAllBytes(Path.Combine(o.Out, $"frame_{frame:D4}.png"), Encode(pixels, o.Width, o.Height, SKEncodedImageFormat.Png, 100));
            }

            encode.Add(sw.Elapsed.TotalMilliseconds);

            streamCost?.Measure(pixels, o.Width, o.Height);

            var particles = CountParticles(simulation);
            maxParticles = Math.Max(maxParticles, particles);
            renderBounds = renderBounds.Add(node);

            frames.Add(new JsonObject
            {
                ["frame"] = frame,
                ["particles"] = particles,
                ["coverage"] = Math.Round(coverage, 5),
                ["simulateMs"] = Math.Round(simulate[^1], 3),
                ["drawMs"] = Math.Round(draw[^1], 3),
                ["readbackMs"] = Math.Round(readback[^1], 3),
                ["encodeMs"] = Math.Round(encode[^1], 3),
                ["tonemapScalar"] = host.Renderer.Postprocess.TonemapScalar,
            });
        }

        var warnings = new JsonArray();
        foreach (var (level, message) in logger.Messages)
        {
            if (level >= LogLevel.Warning || message.Contains("unknown", StringComparison.OrdinalIgnoreCase) || message.Contains("unsupported", StringComparison.OrdinalIgnoreCase) || message.Contains("not implemented", StringComparison.OrdinalIgnoreCase))
            {
                warnings.Add($"{level}: {message}");
            }
        }

        var allMessages = new JsonArray();
        foreach (var (level, message) in logger.Messages)
        {
            allMessages.Add($"{level}: {message}");
        }

        var gameInfo = loader.GameInfo;
        var run = new JsonObject
        {
            ["effect"] = o.Effect,
            ["pak"] = o.Pak,
            ["overlay"] = o.Overlay,
            ["gl"] = new JsonObject
            {
                ["vendor"] = host.GLVendor,
                ["renderer"] = host.GLRenderer,
                ["version"] = host.GLVersion,
                ["requested"] = $"{o.GLVersion.Major}.{o.GLVersion.Minor} core",
            },
            ["gameInfo"] = new JsonObject
            {
                ["path"] = loader.GameInfoPath,
                ["name"] = gameInfo.Name,
                ["game"] = gameInfo.Game.ToString(),
                ["skipPostProcessing"] = gameInfo.SkipPostProcessing,
            },
            ["settings"] = new JsonObject
            {
                ["size"] = $"{o.Width}x{o.Height}",
                ["fps"] = o.Fps,
                ["seconds"] = o.Seconds,
                ["speed"] = o.Speed,
                ["msaa"] = o.Msaa,
                ["exposureStops"] = o.Exposure,
                ["gamma"] = o.Gamma,
                ["sun"] = !o.NoSun,
                ["mode"] = o.Mode,
                ["ground"] = o.Ground,
                ["previewModel"] = previewModel == null ? null : (o.ShowModel ? "shown" : "hidden"),
                ["parallelSimulation"] = !o.Serial,
                ["controlPointsSet"] = new JsonArray([.. o.ControlPoints.Select(c => (JsonNode)$"{c.Index}={Vec(c.Position)}")]),
                ["controlPointVelocities"] = new JsonArray([.. o.ControlPointVelocities.Select(c => (JsonNode)$"{c.Index}={Vec(c.Velocity)}")]),
                ["controlPointPeriod"] = o.ControlPointPeriod,
            },
            ["background"] = new JsonObject
            {
                ["requested"] = Hex(o.Background),
                ["achieved"] = Hex(background.Achieved),
                ["naiveSrgbToLinear"] = Hex(background.Naive),
                ["linearClear"] = Vec(background.Linear),
            },
            ["timing"] = new JsonObject
            {
                ["loadEffectMs"] = Math.Round(loadedAt, 1),
                ["contextAndRendererMs"] = Math.Round(contextAt - loadedAt, 1),
                ["backgroundCalibrationMs"] = Math.Round(backgroundAt - contextAt, 1),
                ["prerollMs"] = Math.Round(prerollMs, 1),
                ["prerollSteps"] = prerollSteps,
                ["startToFirstFrameMs"] = firstFrameAt.HasValue ? Math.Round(firstFrameAt.Value, 1) : null,
                ["startToFirstVisibleFrameMs"] = firstVisibleFrameAt.HasValue ? Math.Round(firstVisibleFrameAt.Value, 1) : null,
                ["firstVisibleFrame"] = firstVisibleFrame,
                ["simulateMs"] = Stats(simulate),
                ["drawMs"] = Stats(draw),
                ["readbackMs"] = Stats(readback),
                ["encodePngMs"] = Stats(encode),
                ["frameTotalMs"] = Stats([.. simulate.Select((s, i) => s + draw[i] + readback[i] + encode[i])]),
                ["frameWithoutEncodeMs"] = Stats([.. simulate.Select((s, i) => s + draw[i] + readback[i])]),
                ["totalMs"] = Math.Round(total.Elapsed.TotalMilliseconds, 1),
                ["workingSetMB"] = Math.Round(Environment.WorkingSet / 1048576.0, 1),
                ["peakWorkingSetMB"] = Math.Round(Process.GetCurrentProcess().PeakWorkingSet64 / 1048576.0, 1),
            },
            ["particles"] = new JsonObject
            {
                ["maxDuringPreroll"] = prerollMaxParticles,
                ["maxDuringRender"] = maxParticles,
                ["systems"] = CountSystems(simulation),
            },
            ["camera"] = new JsonObject
            {
                ["mode"] = o.Frame,
                ["infiniteBounds"] = node.HasInfiniteBounds,
                ["prerollBounds"] = prerollBounds.ToJson(),
                ["prerollParticleBounds"] = prerollParticleBounds.ToJson(),
                ["renderBounds"] = renderBounds.ToJson(),
                ["location"] = Vec(camera.Location),
                ["pitchDeg"] = Math.Round(camera.Pitch * 180 / MathF.PI, 2),
                ["yawDeg"] = Math.Round(camera.Yaw * 180 / MathF.PI, 2),
                ["nearPlane"] = camera.NearPlane,
                ["distance"] = Math.Round(lastTargetDistance, 2),
                ["pixelFit"] = fitPasses,
            },
            ["rulesApplied"] = rulesApplied,
            ["controlPointsAfterPreroll"] = cpAfterPreroll,
            ["controlPointConfigurations"] = ControlPointScan.Configurations(system),
            ["controlPointUses"] = ControlPointScan.ToJson(ControlPointScan.Scan(loader, o.Effect)),
            ["skippedRenderers"] = SkippedRenderers(node),
            ["warnings"] = warnings,
            ["log"] = allMessages,
            ["resolved"] = Resolved(loader),
            ["frames"] = frames,
        };

        if (streamCost != null)
        {
            run["streamCost"] = streamCost.ToJson(o.Fps);
        }

        File.WriteAllText(Path.Combine(o.Out, "run.json"), run.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        Console.WriteLine($"{frameCount} frames, first visible frame {firstVisibleFrame}, max particles {maxParticles}, {warnings.Count} warnings -> {o.Out}");

        effectResource.Dispose();
        return ExitCodes.Ok;
    }

    /// <summary>
    /// Moves and zooms the camera so the pixels the effect covered during a pre-roll fill
    /// <paramref name="fill"/> of the frame, centred. The shift is made in the plane through the
    /// camera target, so it is exact for content at the target's depth and close for the rest.
    /// </summary>
    static JsonObject FitToPixels(Camera camera, PixelBox box, int width, int height, float fill)
    {
        if (box.IsEmpty)
        {
            return new JsonObject { ["empty"] = true };
        }

        var tanV = MathF.Tan(camera.GetFOV() * 0.5f);
        var tanH = tanV * width / height;

        // Normalised device coordinates of the covered box (y up)
        var x0 = box.X0 / (float)width * 2f - 1f;
        var x1 = (box.X1 + 1) / (float)width * 2f - 1f;
        var y0 = 1f - (box.Y1 + 1) / (float)height * 2f;
        var y1 = 1f - box.Y0 / (float)height * 2f;
        var cx = (x0 + x1) * 0.5f;
        var cy = (y0 + y1) * 0.5f;
        var half = MathF.Max((x1 - x0) * 0.5f, (y1 - y0) * 0.5f);

        var target = camera.Location + camera.Forward * DistanceToTarget(camera);
        var distance = DistanceToTarget(camera);
        target += camera.Right * (cx * tanH * distance) + camera.Up * (cy * tanV * distance);

        var scale = Math.Clamp(half / fill, 0.02f, 4f);
        var newDistance = distance * scale;

        camera.SetLocation(target - camera.Forward * newDistance);
        camera.LookAt(target);
        lastTargetDistance = newDistance;
        camera.NearPlane = Math.Clamp(newDistance * 0.01f, 0.05f, 4f);
        camera.CreateProjectionMatrix();
        camera.RecalculateMatrices();

        return new JsonObject
        {
            ["pixelBox"] = $"{box.X0},{box.Y0}-{box.X1},{box.Y1}",
            ["scale"] = Math.Round(scale, 4),
            ["distance"] = Math.Round(newDistance, 2),
            ["target"] = Vec(target),
        };
    }

    static float lastTargetDistance = 346f;

    static float DistanceToTarget(Camera camera) => lastTargetDistance;

    /// <summary>In game mode the node does not loop by itself; start it again once it has finished,
    /// with the control points set again.</summary>
    static void LoopIfFinished(ParticleSceneNode node, Options o)
    {
        if (o.Mode == "preview" || !node.IsFinished)
        {
            return;
        }

        node.Restart();
        foreach (var (index, position) in o.ControlPoints)
        {
            node.GetControlPoint(index).Position = position;
        }
    }

    /// <summary>Moves control points at a constant velocity from where they were set, starting over each period
    /// (a bullet or projectile carrying its effect).</summary>
    static void MoveControlPoints(ParticleSceneNode node, Options o, float time)
    {
        if (o.ControlPointVelocities.Count == 0)
        {
            return;
        }

        var t = o.ControlPointPeriod > 0 ? time % o.ControlPointPeriod : time;
        foreach (var (index, velocity) in o.ControlPointVelocities)
        {
            var start = o.ControlPoints.FirstOrDefault(c => c.Index == index).Position;
            node.GetControlPoint(index).Position = start + velocity * t;
        }
    }

    internal static JsonArray SkippedRenderers(ParticleSceneNode node)
    {
        var result = new JsonArray();
        foreach (var name in LibraryAccess.SkippedRenderers(node))
        {
            result.Add(name);
        }

        return result;
    }

    internal static ValveResourceFormat.Particles.ParticleSystemSimulation? SimulationOf(ParticleSceneNode node) => LibraryAccess.SimulationOf(node);

    static int CountParticles(ValveResourceFormat.Particles.ParticleSystemSimulation? simulation)
    {
        if (simulation == null)
        {
            return -1;
        }

        var count = simulation.Particles.Count;
        foreach (var child in simulation.Children)
        {
            count += CountParticles(child);
        }

        return count;
    }

    static int CountSystems(ValveResourceFormat.Particles.ParticleSystemSimulation? simulation)
        => simulation == null ? 0 : 1 + simulation.Children.Sum(CountSystems);

    static IEnumerable<int> UsedControlPoints(ValveResourceFormat.Particles.ParticleSystemSimulation? simulation)
    {
        // Control points 0..15 cover what the surveyed effects use.
        for (var i = 0; i < 16; i++)
        {
            yield return i;
        }
    }

    static void PlaceCamera(RenderHost host, Options o, Bounds bounds, bool infinite)
    {
        var camera = host.Renderer.Camera;
        var yaw = o.Yaw * MathF.PI / 180f;
        var pitch = o.Pitch * MathF.PI / 180f;

        Vector3 target;
        float distance;

        if (o.Frame != "fixed" && !bounds.IsEmpty)
        {
            var size = Vector3.Max(bounds.Size * o.Pad, new Vector3(1f));
            target = o.Target ?? bounds.Center;
            camera.FrameObjectFromAngle(target, size.X, size.Y, size.Z, yaw, pitch);
            distance = Vector3.Distance(camera.Location, target);
        }
        else
        {
            target = o.Target ?? Vector3.Zero;
            distance = 346f; // the particle viewer's default eye at (200, 200, 200)
        }

        if (o.Distance is { } fixedDistance)
        {
            distance = fixedDistance;
        }

        camera.Yaw = yaw;
        camera.Pitch = pitch;
        camera.RecalculateDirectionVectors();
        camera.SetLocation(target - camera.Forward * distance);
        lastTargetDistance = distance;
        camera.LookAt(target);

        camera.NearPlane = Math.Clamp(distance * 0.01f, 0.05f, 4f);
        camera.SetViewportSize(o.Width, o.Height);
        camera.CreateProjectionMatrix();
        camera.RecalculateMatrices();
    }

    sealed record BackgroundResult((byte R, byte G, byte B) Achieved, (byte R, byte G, byte B) Naive, Vector3 Linear);

    /// <summary>
    /// The clear colour passes through exposure, tone mapping and gamma like the effect does. Each
    /// channel is searched (in log space) for the linear value whose output byte is the requested one.
    /// </summary>
    static BackgroundResult CalibrateBackground(RenderHost host, (byte R, byte G, byte B) target)
    {
        static float SrgbToLinear(byte c)
        {
            var v = c / 255f;
            return v <= 0.04045f ? v / 12.92f : MathF.Pow((v + 0.055f) / 1.055f, 2.4f);
        }

        (byte, byte, byte) RenderWith(Vector3 linear)
        {
            host.Scene.ClearColor = new OpenTK.Mathematics.Color4(linear.X, linear.Y, linear.Z, 1f);
            host.Update(0f);
            host.Draw();
            return host.ReadPixel(host.Width / 2, host.Height / 2);
        }

        var naiveLinear = new Vector3(SrgbToLinear(target.R), SrgbToLinear(target.G), SrgbToLinear(target.B));
        var naive = RenderWith(naiveLinear);

        var lo = new Vector3(-14f);
        var hi = new Vector3(6f);
        var want = new Vector3(target.R, target.G, target.B);

        for (var i = 0; i < 22; i++)
        {
            var mid = (lo + hi) * 0.5f;
            var linear = new Vector3(MathF.Pow(2, mid.X), MathF.Pow(2, mid.Y), MathF.Pow(2, mid.Z));
            var (r, g, b) = RenderWith(linear);
            var got = new Vector3(r, g, b);

            lo = new Vector3(got.X < want.X ? mid.X : lo.X, got.Y < want.Y ? mid.Y : lo.Y, got.Z < want.Z ? mid.Z : lo.Z);
            hi = new Vector3(got.X < want.X ? hi.X : mid.X, got.Y < want.Y ? hi.Y : mid.Y, got.Z < want.Z ? hi.Z : mid.Z);
        }

        var final = new Vector3(
            target.R == 0 ? 0 : MathF.Pow(2, hi.X),
            target.G == 0 ? 0 : MathF.Pow(2, hi.Y),
            target.B == 0 ? 0 : MathF.Pow(2, hi.Z));
        var achieved = RenderWith(final);

        return new BackgroundResult(achieved, naive, final);
    }

    static double Coverage(byte[] bgra, (byte R, byte G, byte B) background)
    {
        var changed = 0;
        var total = bgra.Length / 4;
        for (var i = 0; i < bgra.Length; i += 4)
        {
            if (Math.Abs(bgra[i] - background.B) > 6 || Math.Abs(bgra[i + 1] - background.G) > 6 || Math.Abs(bgra[i + 2] - background.R) > 6)
            {
                changed++;
            }
        }

        return (double)changed / total;
    }

    public static byte[] Encode(byte[] bgra, int width, int height, SKEncodedImageFormat format, int quality)
    {
        var info = new SKImageInfo(width, height, SKColorType.Bgra8888, SKAlphaType.Opaque);
        unsafe
        {
            fixed (byte* p = bgra)
            {
                using var pixmap = new SKPixmap(info, (IntPtr)p, width * 4);
                using var data = pixmap.Encode(format, quality) ?? throw new InvalidOperationException($"Could not encode a frame as {format}.");
                return data.ToArray();
            }
        }
    }

    public static byte[] EncodeWebp(byte[] bgra, int width, int height, bool lossless, int quality)
    {
        var info = new SKImageInfo(width, height, SKColorType.Bgra8888, SKAlphaType.Opaque);
        unsafe
        {
            fixed (byte* p = bgra)
            {
                using var pixmap = new SKPixmap(info, (IntPtr)p, width * 4);
                var options = new SKWebpEncoderOptions(lossless ? SKWebpEncoderCompression.Lossless : SKWebpEncoderCompression.Lossy, quality);
                using var data = pixmap.Encode(options) ?? throw new InvalidOperationException("Could not encode a frame as WebP.");
                return data.ToArray();
            }
        }
    }

    static JsonObject Stats(List<double> values)
    {
        if (values.Count == 0)
        {
            return new JsonObject();
        }

        var sorted = values.OrderBy(v => v).ToArray();
        return new JsonObject
        {
            ["median"] = Math.Round(sorted[sorted.Length / 2], 3),
            ["p95"] = Math.Round(sorted[(int)Math.Min(sorted.Length - 1, Math.Floor(sorted.Length * 0.95))], 3),
            ["worst"] = Math.Round(sorted[^1], 3),
            ["worstIndex"] = values.IndexOf(sorted[^1]),
            ["mean"] = Math.Round(values.Average(), 3),
        };
    }

    public static string Vec(Vector3 v) => FormattableString.Invariant($"{v.X:0.###},{v.Y:0.###},{v.Z:0.###}");

    static string Hex((byte R, byte G, byte B) c) => $"{c.R:x2}{c.G:x2}{c.B:x2}";
}

/// <summary>Pixel rectangle (top-left origin) of everything that differs from the background.</summary>
readonly record struct PixelBox(int X0, int Y0, int X1, int Y1, bool IsEmpty)
{
    public static PixelBox Empty => new(0, 0, 0, 0, true);

    public PixelBox Add(byte[] bgra, int width, int height, (byte R, byte G, byte B) background, int threshold = 12)
    {
        int x0 = int.MaxValue, y0 = int.MaxValue, x1 = -1, y1 = -1;
        for (var y = 0; y < height; y++)
        {
            var row = y * width * 4;
            for (var x = 0; x < width; x++)
            {
                var i = row + x * 4;
                if (Math.Abs(bgra[i] - background.B) > threshold || Math.Abs(bgra[i + 1] - background.G) > threshold || Math.Abs(bgra[i + 2] - background.R) > threshold)
                {
                    if (x < x0) x0 = x;
                    if (x > x1) x1 = x;
                    if (y < y0) y0 = y;
                    if (y > y1) y1 = y;
                }
            }
        }

        if (x1 < 0)
        {
            return this;
        }

        return IsEmpty
            ? new PixelBox(x0, y0, x1, y1, false)
            : new PixelBox(Math.Min(X0, x0), Math.Min(Y0, y0), Math.Max(X1, x1), Math.Max(Y1, y1), false);
    }
}

/// <summary>Union of a scene node's bounds over time, ignoring empty and unbounded boxes.</summary>
readonly record struct Bounds(Vector3 Min, Vector3 Max, bool IsEmpty)
{
    public static Bounds Empty => new(Vector3.Zero, Vector3.Zero, true);

    public Vector3 Size => Max - Min;

    public Vector3 Center => (Min + Max) * 0.5f;

    public Bounds Include(Vector3 min, Vector3 max)
        => IsEmpty ? new Bounds(min, max, false) : new Bounds(Vector3.Min(Min, min), Vector3.Max(Max, max), false);

    public Bounds Add(SceneNode node)
    {
        if (node is ParticleSceneNode { HasInfiniteBounds: true })
        {
            return this;
        }

        var box = node.BoundingBox;
        var size = box.Max - box.Min;

        if (!float.IsFinite(size.X) || !float.IsFinite(size.Y) || !float.IsFinite(size.Z) || size.X > 1e6f || (size == Vector3.Zero && box.Min == Vector3.Zero))
        {
            return this;
        }

        return IsEmpty ? new Bounds(box.Min, box.Max, false) : new Bounds(Vector3.Min(Min, box.Min), Vector3.Max(Max, box.Max), false);
    }

    /// <summary>Grows the box by every live particle (position plus radius) of a system and its children,
    /// without the authored per-system padding; screen-space systems are skipped.</summary>
    public static Bounds AddParticles(Bounds bounds, ValveResourceFormat.Particles.ParticleSystemSimulation? simulation)
    {
        if (simulation == null || simulation.IsScreenSpaceEffect)
        {
            return bounds;
        }

        var collection = simulation.Particles;
        if (collection.Count > 0)
        {
            var span = collection.Current;
            for (var i = 0; i < collection.Count; i++)
            {
                var p = span.ReadVector(ValveResourceFormat.Particles.ParticleField.Position, i);
                var radius = span.ReadFloat(ValveResourceFormat.Particles.ParticleField.Radius, i);
                var r = float.IsFinite(radius) ? Math.Clamp(radius, 0f, 1e4f) : 0f;
                if (!float.IsFinite(p.X) || !float.IsFinite(p.Y) || !float.IsFinite(p.Z) || p.Length() > 1e6f)
                {
                    continue;
                }

                var min = p - new Vector3(r);
                var max = p + new Vector3(r);
                bounds = bounds.IsEmpty ? new Bounds(min, max, false) : new Bounds(Vector3.Min(bounds.Min, min), Vector3.Max(bounds.Max, max), false);
            }
        }

        foreach (var child in simulation.Children)
        {
            bounds = AddParticles(bounds, child);
        }

        return bounds;
    }

    public JsonObject ToJson() => IsEmpty
        ? new JsonObject { ["empty"] = true }
        : new JsonObject { ["min"] = Program.Vec(Min), ["max"] = Program.Vec(Max), ["size"] = Program.Vec(Size) };
}

/// <summary>Encodes each frame as JPEG and WebP without sending it anywhere, to measure what a stream would cost.</summary>
sealed class StreamCost(int quality)
{
    readonly List<double> jpegMs = [];
    readonly List<double> webpMs = [];
    readonly List<double> webpLosslessMs = [];
    readonly List<int> jpegBytes = [];
    readonly List<int> webpBytes = [];
    readonly List<int> webpLosslessBytes = [];

    public void Measure(byte[] bgra, int width, int height)
    {
        var sw = Stopwatch.StartNew();
        jpegBytes.Add(Program.Encode(bgra, width, height, SKEncodedImageFormat.Jpeg, quality).Length);
        jpegMs.Add(sw.Elapsed.TotalMilliseconds);

        sw.Restart();
        webpBytes.Add(Program.EncodeWebp(bgra, width, height, lossless: false, quality).Length);
        webpMs.Add(sw.Elapsed.TotalMilliseconds);

        sw.Restart();
        // Lossless at the fastest effort setting a stream would use.
        webpLosslessBytes.Add(Program.EncodeWebp(bgra, width, height, lossless: true, 0).Length);
        webpLosslessMs.Add(sw.Elapsed.TotalMilliseconds);
    }

    static JsonObject Summary(List<double> ms, List<int> bytes, int fps)
    {
        var sortedMs = ms.OrderBy(v => v).ToArray();
        var sortedBytes = bytes.OrderBy(v => v).ToArray();
        return new JsonObject
        {
            ["medianMs"] = Math.Round(sortedMs[sortedMs.Length / 2], 3),
            ["worstMs"] = Math.Round(sortedMs[^1], 3),
            ["medianBytes"] = sortedBytes[sortedBytes.Length / 2],
            ["worstBytes"] = sortedBytes[^1],
            ["meanBytes"] = Math.Round(bytes.Average(), 0),
            ["bytesPerSecondAtFps"] = Math.Round(bytes.Average() * fps, 0),
        };
    }

    public JsonObject ToJson(int fps) => new()
    {
        ["quality"] = quality,
        ["jpeg"] = Summary(jpegMs, jpegBytes, fps),
        ["webp"] = Summary(webpMs, webpBytes, fps),
        ["webpLossless"] = Summary(webpLosslessMs, webpLosslessBytes, fps),
    };
}

sealed class Options
{
    public string Pak { get; private set; } = "";
    public string Effect { get; private set; } = "";
    public string? Overlay { get; private set; }
    public string Out { get; private set; } = "";
    public string? Prefix { get; private set; }
    public string? Classes { get; private set; }
    public string? Keys { get; private set; }
    public int Width { get; private set; } = 640;
    public int Height { get; private set; } = 480;
    public int Fps { get; private set; } = 30;
    public float Seconds { get; private set; } = 3f;
    public float Speed { get; private set; } = 1f;
    public float Preroll { get; private set; } = -1f;
    public (byte R, byte G, byte B) Background { get; private set; } = (0x0b, 0x0c, 0x10);
    public float Exposure { get; private set; }
    public float Gamma { get; private set; } = 2.2f;
    public int Msaa { get; private set; } = 4;
    public string Frame { get; private set; } = "auto";
    public float Yaw { get; private set; } = 225f;
    public float Pitch { get; private set; } = 20f;
    public float? Distance { get; private set; }
    public Vector3? Target { get; private set; }
    public float Pad { get; private set; } = 1.15f;
    public List<(int Index, Vector3 Position)> ControlPoints { get; } = [];
    public List<(int Index, Vector3 Velocity)> ControlPointVelocities { get; } = [];
    public float ControlPointPeriod { get; set; } = 1f;
    public bool Rules { get; private set; }
    public float Radius { get; private set; } = EffectRules.DefaultRadius;
    public bool NoSun { get; private set; }
    public bool ShowModel { get; private set; }
    public string Mode { get; private set; } = "preview";
    public float? Ground { get; private set; }
    public bool Serial { get; private set; }
    public bool NoFrames { get; private set; }
    public bool StreamCost { get; private set; }
    public int Quality { get; private set; } = 80;
    public bool Verbose { get; private set; }
    public Version GLVersion { get; private set; } = GLEnvironment.RequiredVersion;

    public static Options Parse(string[] args, bool needsPak = true)
    {
        var o = new Options();

        for (var i = 0; i < args.Length; i++)
        {
            string Next() => i + 1 < args.Length ? args[++i] : throw new ArgumentException($"{args[i]} needs a value.");
            float NextFloat() => float.Parse(Next(), CultureInfo.InvariantCulture);

            switch (args[i])
            {
                case "--pak": o.Pak = Next(); break;
                case "--effect": o.Effect = Next().Replace('\\', '/'); break;
                case "--overlay": o.Overlay = Next(); break;
                case "--out": o.Out = Next(); break;
                case "--prefix": o.Prefix = Next(); break;
                case "--class": o.Classes = Next(); break;
                case "--key": o.Keys = Next(); break;
                case "--size":
                    var size = Next().Split('x');
                    o.Width = int.Parse(size[0], CultureInfo.InvariantCulture);
                    o.Height = int.Parse(size[1], CultureInfo.InvariantCulture);
                    break;
                case "--fps": o.Fps = int.Parse(Next(), CultureInfo.InvariantCulture); break;
                case "--seconds": o.Seconds = NextFloat(); break;
                case "--speed": o.Speed = NextFloat(); break;
                case "--preroll": o.Preroll = NextFloat(); break;
                case "--background": o.Background = ParseHex(Next()); break;
                case "--exposure": o.Exposure = NextFloat(); break;
                case "--gamma": o.Gamma = NextFloat(); break;
                case "--msaa": o.Msaa = int.Parse(Next(), CultureInfo.InvariantCulture); break;
                case "--frame": o.Frame = Next(); break;
                case "--yaw": o.Yaw = NextFloat(); break;
                case "--pitch": o.Pitch = NextFloat(); break;
                case "--distance": o.Distance = NextFloat(); break;
                case "--target": o.Target = ParseVector(Next()); break;
                case "--pad": o.Pad = NextFloat(); break;
                case "--cp-velocity":
                    var cv = Next().Split('=', 2);
                    o.ControlPointVelocities.Add((int.Parse(cv[0], CultureInfo.InvariantCulture), ParseVector(cv[1])));
                    break;
                case "--cp-period": o.ControlPointPeriod = NextFloat(); break;
                case "--cp":
                    var cp = Next().Split('=', 2);
                    o.ControlPoints.Add((int.Parse(cp[0], CultureInfo.InvariantCulture), ParseVector(cp[1])));
                    break;
                case "--rules": o.Rules = true; break;
                case "--radius": o.Radius = NextFloat(); break;
                case "--no-sun": o.NoSun = true; break;
                case "--show-model": o.ShowModel = true; break;
                case "--mode": o.Mode = Next(); break;
                case "--ground": o.Ground = NextFloat(); break;
                case "--serial": o.Serial = true; break;
                case "--no-frames": o.NoFrames = true; break;
                case "--stream-cost": o.StreamCost = true; break;
                case "--quality": o.Quality = int.Parse(Next(), CultureInfo.InvariantCulture); break;
                case "--verbose": o.Verbose = true; break;
                // Test switch: request a context version, for example one that does not exist.
                case "--gl-version": o.GLVersion = Version.Parse(Next()); break;
                default: throw new ArgumentException($"Unknown option {args[i]}.");
            }
        }

        if (needsPak && o.Pak.Length == 0)
        {
            throw new ArgumentException("--pak is required.");
        }

        if (o.Preroll < 0)
        {
            o.Preroll = o.Seconds;
        }

        return o;
    }

    static (byte, byte, byte) ParseHex(string hex)
    {
        hex = hex.TrimStart('#');
        var value = Convert.ToInt32(hex, 16);
        return ((byte)(value >> 16), (byte)(value >> 8), (byte)value);
    }

    static Vector3 ParseVector(string text)
    {
        var parts = text.Split(',').Select(p => float.Parse(p, CultureInfo.InvariantCulture)).ToArray();
        return new Vector3(parts[0], parts[1], parts[2]);
    }
}
