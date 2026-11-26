using System.Text.Json.Nodes;
using ValveKeyValue;
using ValveResourceFormat;
using ValveResourceFormat.IO;
using ValveResourceFormat.ResourceTypes;
using ValveResourceFormat.Serialization.KeyValues;

namespace Dmb.EffectPlayer;

/// <summary>
/// Lists what an effect reads from control points: number inputs of type
/// PF_TYPE_CONTROL_POINT_COMPONENT, vector inputs that take a control point value, and integer
/// fields naming a control point. Children are followed through the loader.
/// </summary>
static class ControlPointScan
{
    static readonly string[] FunctionLists =
    [
        "m_PreEmissionOperators", "m_Emitters", "m_Initializers", "m_Operators",
        "m_ForceGenerators", "m_Constraints", "m_Renderers",
    ];

    public sealed record Use(string File, string Function, string Field, string Kind, int ControlPoint, int Component, string Detail);

    public static List<Use> Scan(IFileLoader loader, string effect, HashSet<string>? seen = null, int depth = 0, bool followChildren = true)
    {
        seen ??= new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var uses = new List<Use>();

        if (!seen.Add(effect) || depth > 8)
        {
            return uses;
        }

        using var resource = loader.LoadFile(effect);

        if (resource?.DataBlock is not ParticleSystem system)
        {
            return uses;
        }

        var data = system.GetUpgradedData();

        foreach (var listName in FunctionLists)
        {
            var list = data.GetArray(listName);
            if (list == null)
            {
                continue;
            }

            for (var i = 0; i < list.Count; i++)
            {
                var function = list[i];
                var className = function.GetStringProperty("_class") ?? "?";
                Walk(function, $"{listName}[{i}] {className}", "", effect, uses);
            }
        }

        foreach (var child in followChildren ? system.GetChildParticleNames(enabledOnly: true) : [])
        {
            var childPath = child.EndsWith("_c", StringComparison.Ordinal) ? child : child + "_c";
            uses.AddRange(Scan(loader, childPath, seen, depth + 1));
        }

        return uses;
    }

    static void Walk(KVObject node, string function, string path, string file, List<Use> uses)
    {
        if (node.IsArray)
        {
            for (var i = 0; i < node.Count; i++)
            {
                var element = node[i];
                if (element.IsCollection || element.IsArray)
                {
                    Walk(element, function, $"{path}[{i}]", file, uses);
                }
            }

            return;
        }

        if (!node.IsCollection)
        {
            return;
        }

        if (node.ContainsKey("m_nType"))
        {
            var type = node.GetStringProperty("m_nType") ?? "";

            if (type == "PF_TYPE_CONTROL_POINT_COMPONENT")
            {
                uses.Add(new Use(file, function, path, "number-from-cp-component",
                    node.GetInt32Property("m_nControlPoint", -1), node.GetInt32Property("m_nVectorComponent", -1),
                    MappingText(node)));
            }
            else if (type.StartsWith("PF_TYPE_CONTROL_POINT", StringComparison.Ordinal))
            {
                uses.Add(new Use(file, function, path, type, node.GetInt32Property("m_nControlPoint", -1), node.GetInt32Property("m_nVectorComponent", -1), ""));
            }
            else if (type.StartsWith("PVEC_TYPE_CP", StringComparison.Ordinal))
            {
                uses.Add(new Use(file, function, path, type, node.GetInt32Property("m_nControlPoint", -1), -1, ""));
            }
        }

        foreach (var key in node.Keys)
        {
            var value = node[key];

            if (value.IsCollection || value.IsArray)
            {
                Walk(value, function, path.Length == 0 ? key : $"{path}.{key}", file, uses);
                continue;
            }

            if (IsControlPointIndexField(key) && TryInt(value, out var index) && index > 0)
            {
                uses.Add(new Use(file, function, path.Length == 0 ? key : $"{path}.{key}", "cp-index-field", index, -1, ""));
            }
        }
    }

    static bool IsControlPointIndexField(string key)
    {
        if (key is "m_nControlPoint" or "m_nVectorComponent")
        {
            return false;
        }

        return key.Contains("ControlPoint", StringComparison.Ordinal)
            || key.StartsWith("m_nCP", StringComparison.Ordinal)
            || key.Contains("CPOutput", StringComparison.Ordinal)
            || key.Contains("CPInput", StringComparison.Ordinal);
    }

    static bool TryInt(KVObject value, out int result)
    {
        try
        {
            result = value.ToInt32(System.Globalization.CultureInfo.InvariantCulture);
            return true;
        }
        catch
        {
            result = 0;
            return false;
        }
    }

    /// <summary>The mapping of a number input: its map type and the scalar settings that apply to it.</summary>
    static string MappingText(KVObject node)
    {
        var mapType = node.GetStringProperty("m_nMapType") ?? "";
        var keys = mapType switch
        {
            "PF_MAP_TYPE_MULT" => new[] { "m_flMultFactor" },
            "PF_MAP_TYPE_REMAP" or "PF_MAP_TYPE_REMAP_BIASED" => ["m_flInput0", "m_flInput1", "m_flOutput0", "m_flOutput1"],
            _ => [],
        };

        var parts = keys.Where(node.ContainsKey).Select(k => $"{k[4..]}={FloatText(node, k)}");
        return $"{mapType} {string.Join(' ', parts)}".Trim();
    }

    static string FloatText(KVObject node, string key)
        => node.ContainsKey(key) ? node.GetFloatProperty(key).ToString("0.###", System.Globalization.CultureInfo.InvariantCulture) : "-";

    public static JsonArray ToJson(IEnumerable<Use> uses)
    {
        var array = new JsonArray();
        foreach (var use in uses)
        {
            array.Add((JsonNode)new JsonObject
            {
                ["file"] = use.File,
                ["function"] = use.Function,
                ["field"] = use.Field,
                ["kind"] = use.Kind,
                ["cp"] = use.ControlPoint,
                ["component"] = use.Component,
                ["detail"] = use.Detail,
            });
        }

        return array;
    }

    /// <summary>The control point configurations an effect carries (the editor's preview set-ups and the game's).</summary>
    public static JsonArray Configurations(ParticleSystem system)
    {
        var result = new JsonArray();
        var configurations = system.GetUpgradedData().GetArray("m_controlPointConfigurations");
        if (configurations == null)
        {
            return result;
        }

        foreach (var configuration in configurations)
        {
            var drivers = new JsonArray();
            foreach (var driver in configuration.GetArray("m_drivers") ?? [])
            {
                drivers.Add((JsonNode)new JsonObject
                {
                    ["cp"] = driver.GetInt32Property("m_iControlPoint"),
                    ["attachType"] = driver.GetStringProperty("m_iAttachType") ?? "",
                    ["attachment"] = driver.GetStringProperty("m_attachmentName") ?? "",
                    ["offset"] = VectorText(driver, "m_vecOffset"),
                    ["entity"] = driver.GetStringProperty("m_entityName") ?? "",
                });
            }

            result.Add((JsonNode)new JsonObject
            {
                ["name"] = configuration.GetStringProperty("m_name") ?? "",
                ["drivers"] = drivers,
                ["previewModel"] = configuration.ContainsKey("m_previewState") ? configuration["m_previewState"].GetStringProperty("m_previewModel") ?? "" : "",
            });
        }

        return result;
    }

    static string VectorText(KVObject node, string key)
    {
        if (!node.ContainsKey(key))
        {
            return "";
        }

        try
        {
            var v = node.GetSubCollection(key)!.ToVector3();
            return FormattableString.Invariant($"{v.X:0.##},{v.Y:0.##},{v.Z:0.##}");
        }
        catch
        {
            return node[key].ToString() ?? "";
        }
    }
}
