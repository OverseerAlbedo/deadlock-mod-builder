using System.Numerics;
using System.Text.Json.Nodes;
using OpenTK.Graphics.OpenGL;
using ValveKeyValue;
using ValveResourceFormat.CompiledShader;
using ValveResourceFormat.Particles;
using ValveResourceFormat.Renderer;
using ValveResourceFormat.Renderer.Materials;
using ValveResourceFormat.Renderer.SceneNodes;
using ValveResourceFormat.Serialization.KeyValues;
using ValveResourceFormat.Utils;

namespace Dmb.EffectPlayer;

/// <summary>
/// Applies the material variables of model renderers (<c>C_OP_RenderModels.m_MaterialVars</c>),
/// which the library reads but does not apply. Each entry names a render attribute
/// (<c>$COLORTINT</c>, <c>$SELFILLUM</c>, ...) and the particle field it takes its value from,
/// times a scale; the model's material turns attributes into its parameters through its dynamic
/// expressions (<c>g_vColorTint1 = return $COLORTINT;</c>). The library's model materials are shared
/// between every node that draws them, so each draw of a model node that reads one of the
/// attributes gets its own copy of its material, and the expressions are evaluated per particle
/// after every update and written into that copy. The library still tints each node by the
/// particle's colour as it always does (unless the renderer suppresses the tint).
///
/// Which node draws which particle is read through <see cref="LibraryAccess"/>. An expression is
/// applied only when every attribute it reads is set by the renderer and it compiles
/// (<see cref="MaterialExpression"/>); otherwise the parameter keeps the material's own value, and
/// the renderer is counted as still drawn untinted. An entry without a particle field reads the
/// scratch float (compiled files leave out a field that holds its default; this one is inferred:
/// no entry in the game names the scratch float explicitly, and the effects whose entries leave the
/// field out are the ones that write it, through <c>C_OP_SetFloat</c>).
/// </summary>
sealed class ModelMaterialVars : IDisposable
{
    public sealed record Variable(string Name, ParticleField? Field, float Scale);

    /// <summary>The particle field of an entry that names none (inferred from the game's files, see <see cref="Attach"/>).</summary>
    const ParticleField DefaultField = ParticleField.ScratchFloat;

    sealed class Binding
    {
        public required ParticleSystemSimulation Simulation;
        public required object Renderer;
        public required string Part;
        public required Dictionary<string, Variable> Variables;
        public readonly Dictionary<string, MaterialPlan> Seen = new(StringComparer.OrdinalIgnoreCase);
        public bool Drawn;
        public Vector3 FirstTint;
    }

    enum ParamKind
    {
        Float,
        Int,
        Vector,
    }

    sealed record Applied(string Param, MaterialExpression Expression, ParamKind Kind);

    sealed class MaterialPlan
    {
        public required string Material;
        public List<Applied> Applied { get; } = [];
        public List<string> NotApplied { get; } = [];
        public List<string> Unset { get; } = [];
        public List<string> FromColour { get; } = [];
        public HashSet<string> Reads { get; } = new(StringComparer.OrdinalIgnoreCase);
    }

    sealed class Copy
    {
        public required RenderMaterial Base;
        public required RenderMaterial Material;
        public required MaterialPlan Plan;
    }

    readonly RendererContext context;
    readonly List<Binding> bindings = [];
    readonly Dictionary<(Binding, RenderMaterial), MaterialPlan> plans = [];
    readonly Dictionary<DrawCall, Copy> copies = [];
    readonly List<string> problems = [];

    /// <summary>Model renderers with material variables in the node's tree.</summary>
    public int Renderers => bindings.Count;

    ModelMaterialVars(RendererContext context)
    {
        this.context = context;
    }

    /// <summary>Finds the model renderers of a freshly built particle node that carry material variables.</summary>
    public static ModelMaterialVars Attach(ParticleSceneNode node, RendererContext context)
    {
        var vars = new ModelMaterialVars(context);
        if (!LibraryAccess.CanReadModelNodes)
        {
            vars.problems.Add("The player cannot see which model draws which particle in this version of the rendering library.");
            return vars;
        }

        foreach (var system in LibraryAccess.ModelRenderers(node))
        {
            var entries = new List<KVObject>();
            foreach (var function in system.Simulation.Definition.GetArray("m_Renderers") ?? [])
            {
                if (function.GetStringProperty("_class") == "C_OP_RenderModels" && !Disabled(function))
                {
                    entries.Add(function);
                }
            }

            var part = system.Simulation.Name ?? "?";
            if (entries.Count != system.ModelRenderers.Count)
            {
                if (entries.Any(e => (e.GetArray("m_MaterialVars")?.Count ?? 0) > 0))
                {
                    vars.problems.Add($"The model renderers of {EffectRules.ShortName(part)} could not be matched to their data.");
                }

                continue;
            }

            for (var i = 0; i < entries.Count; i++)
            {
                var variables = new Dictionary<string, Variable>(StringComparer.OrdinalIgnoreCase);
                foreach (var entry in entries[i].GetArray("m_MaterialVars") ?? [])
                {
                    var name = entry.GetStringProperty("m_strVariable") ?? "";
                    if (name.Length == 0)
                    {
                        continue;
                    }

                    // An absent field holds its default, read as the scratch float: no entry in the
                    // game's files names it explicitly, and the effects whose entries leave the
                    // field out are the ones that write the scratch float.
                    ParticleField? field = entry.ContainsKey("m_nVariableField") ? (ParticleField)entry.GetInt32Property("m_nVariableField", -1) : DefaultField;
                    if (field is { } f && (int)f < 0)
                    {
                        field = null;
                    }

                    variables[name.StartsWith('$') ? name : "$" + name] = new Variable(name, field, entry.ContainsKey("m_flScale") ? entry.GetFloatProperty("m_flScale") : 1f);
                }

                if (variables.Count > 0)
                {
                    vars.bindings.Add(new Binding { Simulation = system.Simulation, Renderer = system.ModelRenderers[i], Part = part, Variables = variables });
                }
            }
        }

        return vars;
    }

    static bool Disabled(KVObject function)
    {
        try
        {
            return function.ContainsKey("m_bDisableOperator") && function.GetParticleBooleanProperty("m_bDisableOperator", false);
        }
        catch (Exception)
        {
            return false;
        }
    }

    /// <summary>The particle fields that hold a vector (the library's own list is internal).</summary>
    static bool IsVectorField(ParticleField field) => field is ParticleField.Position or ParticleField.PositionPrevious
        or ParticleField.Color or ParticleField.HitboxOffsetPosition or ParticleField.ScratchVector or ParticleField.Normal
        or ParticleField.GlowRgb or ParticleField.ScratchVector2 or ParticleField.BoxMins or ParticleField.BoxMaxs
        or ParticleField.BoxAngles;

    /// <summary>Evaluates the variables for every drawn model particle and writes them into the node's materials.</summary>
    public void Apply()
    {
        foreach (var binding in bindings)
        {
            var particles = binding.Simulation.Particles;
            if (particles.Count == 0)
            {
                continue;
            }

            var nodes = LibraryAccess.NodesByParticle(binding.Renderer, particles);
            if (nodes.Length == 0)
            {
                continue;
            }

            var span = particles.Current;
            var time = span.Time;
            for (var i = 0; i < nodes.Length; i++)
            {
                if (nodes[i] is not { Visible: true } node)
                {
                    continue;
                }

                if (!binding.Drawn)
                {
                    binding.Drawn = true;
                    binding.FirstTint = node.Tint;
                }

                var index = i;
                ExpressionValue Value(string attribute)
                {
                    if (!binding.Variables.TryGetValue(attribute, out var variable))
                    {
                        return ExpressionValue.Scalar(0f);
                    }

                    if (variable.Field is not { } field)
                    {
                        return ExpressionValue.Scalar(variable.Scale);
                    }

                    return IsVectorField(field)
                        ? new ExpressionValue(new Vector4(span.ReadVector(field, index) * variable.Scale, 0f), 3)
                        : ExpressionValue.Scalar(span.ReadFloat(field, index) * variable.Scale);
                }

                foreach (var mesh in node.RenderableMeshes)
                {
                    foreach (var draw in mesh.DrawCalls)
                    {
                        if (CopyFor(binding, draw) is not { } copy)
                        {
                            continue;
                        }

                        foreach (var applied in copy.Plan.Applied)
                        {
                            Write(copy, applied, applied.Expression.Evaluate(Value, time));
                        }
                    }
                }
            }
        }
    }

    /// <summary>The draw's own material copy, made (or made again after the library put a shared material back) when needed.</summary>
    Copy? CopyFor(Binding binding, DrawCall draw)
    {
        var current = draw.Material;
        if (copies.TryGetValue(draw, out var existing))
        {
            if (ReferenceEquals(existing.Material, current))
            {
                return existing;
            }

            // A skin or mesh group change put a shared material back.
            Delete(existing);
            copies.Remove(draw);
        }

        var plan = PlanFor(binding, current);
        if (plan.Applied.Count == 0)
        {
            return null;
        }

        RenderMaterial material;
        try
        {
            material = new RenderMaterial(current.Material, context, new Dictionary<string, byte>(current.Shader.Parameters));
        }
        catch (Exception e)
        {
            AddProblem($"A copy of {current.Material.Name} could not be made ({e.Message}).");
            return null;
        }

        foreach (var (name, value) in current.IntParams)
        {
            material.IntParams[name] = value;
        }

        foreach (var (name, value) in current.FloatParams)
        {
            material.FloatParams[name] = value;
        }

        foreach (var (name, value) in current.VectorParams)
        {
            material.VectorParams[name] = value;
        }

        foreach (var (name, value) in current.Matrices)
        {
            material.Matrices[name] = value;
        }

        foreach (var (name, value) in current.Textures)
        {
            material.Textures[name] = value;
        }

        var copy = new Copy { Base = current, Material = material, Plan = plan };
        copies[draw] = copy;
        draw.SetNewMaterial(material);
        return copy;
    }

    MaterialPlan PlanFor(Binding binding, RenderMaterial material)
    {
        if (plans.TryGetValue((binding, material), out var plan))
        {
            binding.Seen.TryAdd(plan.Material, plan);
            return plan;
        }

        var data = material.Material;
        plan = new MaterialPlan { Material = data.Name };
        var expressions = data.DynamicExpressions;
        foreach (var (param, text) in expressions)
        {
            var isTexture = data.DynamicTextureParams.ContainsKey(param);
            var compiled = MaterialExpression.Compile(text, out var reason);
            if (compiled == null)
            {
                plan.NotApplied.Add($"{param} = {text.Replace("\\n", " ", StringComparison.Ordinal)}: {reason}");
                continue;
            }

            plan.Reads.UnionWith(compiled.Attributes);
            var unset = compiled.Attributes.Where(a => !binding.Variables.ContainsKey(a)).ToList();
            if (compiled.Attributes.Count == 0 && !compiled.ReadsTime)
            {
                // A constant: the library already holds the material's own value.
                continue;
            }

            if (unset.Count > 0)
            {
                // The game evaluates these with attributes the renderer does not set; what they read
                // then is not known here, so the material keeps its own value.
                plan.Unset.Add($"{param} = {compiled.Text} ({string.Join(", ", unset)} not set by the renderer)");
                continue;
            }

            if (isTexture)
            {
                // What a texture parameter given a number shows in the game is not known here (a
                // texture of that colour would lose the texture's shape), so the material keeps its
                // own texture.
                if (material.Shader.UniformNames.Contains(param))
                {
                    plan.NotApplied.Add($"{param} = {compiled.Text}: a texture parameter fed from a render attribute; the material's own texture is drawn");
                }

                continue;
            }

            ParamKind kind;
            if (data.VectorParams.ContainsKey(param) || param.StartsWith("g_v", StringComparison.Ordinal))
            {
                kind = ParamKind.Vector;
            }
            else if (data.IntParams.ContainsKey(param) || param.StartsWith("g_n", StringComparison.Ordinal) || param.StartsWith("g_b", StringComparison.Ordinal))
            {
                kind = ParamKind.Int;
            }
            else
            {
                kind = ParamKind.Float;
            }

            plan.Applied.Add(new Applied(param, compiled, kind));
            if (compiled.Attributes.Any(a => binding.Variables[a].Field == ParticleField.Color))
            {
                plan.FromColour.Add(param);
            }
        }

        plans[(binding, material)] = plan;
        binding.Seen.TryAdd(plan.Material, plan);
        return plan;
    }

    void Write(Copy copy, Applied applied, ExpressionValue value)
    {
        var material = copy.Material;
        switch (applied.Kind)
        {
            case ParamKind.Float:
                material.FloatParams[applied.Param] = value.X;
                break;
            case ParamKind.Int:
                material.IntParams[applied.Param] = (long)MathF.Round(value.X);
                break;
            case ParamKind.Vector:
                material.VectorParams[applied.Param] = value.Widened;
                break;
        }
    }

    void AddProblem(string text)
    {
        if (!problems.Contains(text))
        {
            problems.Add(text);
        }
    }

    /// <summary>
    /// What was applied: per renderer part, the parameters fed from the particle, the ones fed from
    /// its colour, the ones that stay at the material's value because the player cannot evaluate
    /// them, and the ones that read an attribute the renderer does not set (the game evaluates those
    /// too, with whatever an unset attribute holds). A renderer counts as untinted when the player
    /// left one of its expressions unapplied, or could not reach its models at all.
    /// </summary>
    public (int Untinted, List<string> UntintedParts, JsonObject Report) Summary()
    {
        var parts = new JsonArray();
        var untinted = 0;
        var untintedParts = new List<string>();
        var applied = 0;
        var fromColour = 0;
        foreach (var binding in bindings)
        {
            var appliedParams = binding.Seen.Values.SelectMany(p => p.Applied.Select(a => a.Param)).Distinct().ToList();
            var colourParams = binding.Seen.Values.SelectMany(p => p.FromColour).Distinct().ToList();
            var notApplied = binding.Seen.Values.SelectMany(p => p.NotApplied).Distinct().ToList();
            var unset = binding.Seen.Values.SelectMany(p => p.Unset).Distinct().ToList();
            if (notApplied.Count > 0 || (problems.Count > 0 && binding.Seen.Count == 0))
            {
                untinted++;
                if (!untintedParts.Contains(binding.Part))
                {
                    untintedParts.Add(binding.Part);
                }
            }

            if (appliedParams.Count > 0)
            {
                applied++;
            }

            if (colourParams.Count > 0)
            {
                fromColour++;
            }

            parts.Add((JsonNode)new JsonObject
            {
                ["part"] = binding.Part,
                ["variables"] = EffectRules.ToArray(binding.Variables.Values.Select(v => v.Field is { } f ? $"{v.Name} from {f}{(v.Scale != 1f ? FormattableString.Invariant($" x {v.Scale:0.###}") : "")}" : FormattableString.Invariant($"{v.Name} = {v.Scale:0.###}"))),
                ["materials"] = EffectRules.ToArray(binding.Seen.Keys),
                ["applied"] = EffectRules.ToArray(appliedParams),
                ["fromColour"] = EffectRules.ToArray(colourParams),
                ["notApplied"] = EffectRules.ToArray(notApplied),
                ["unset"] = EffectRules.ToArray(unset),
                // Variables the renderer sets that no material it was seen drawing reads (the game
                // sets them too, to no effect).
                ["ignored"] = binding.Drawn
                    ? EffectRules.ToArray(binding.Variables.Keys.Where(v => !binding.Seen.Values.Any(p => p.Reads.Contains(v))))
                    : new JsonArray(),
                ["seen"] = binding.Drawn,
                // The tint the library gave the first model it drew (the particle's colour), as sRGB hex.
                ["firstTint"] = binding.Drawn ? Hex(binding.FirstTint) : null,
            });
        }

        var report = new JsonObject
        {
            ["renderers"] = bindings.Count,
            ["applied"] = applied,
            ["fromColour"] = fromColour,
            ["parts"] = parts,
            ["problems"] = EffectRules.ToArray(problems),
        };
        return (untinted, untintedParts, report);
    }

    static string Hex(Vector3 c)
    {
        static int B(float x) => (int)MathF.Round(Math.Clamp(x, 0f, 1f) * 255f);
        return $"{B(c.X):x2}{B(c.Y):x2}{B(c.Z):x2}";
    }

    static void Delete(Copy copy) => copy.Material.Delete();

    /// <summary>Deletes the material copies. Call before the particle node is deleted.</summary>
    public void Dispose()
    {
        foreach (var (draw, copy) in copies)
        {
            if (ReferenceEquals(draw.Material, copy.Material))
            {
                draw.Material = copy.Base;
            }

            Delete(copy);
        }

        copies.Clear();
        plans.Clear();
    }
}
