using System.Globalization;
using System.Numerics;

namespace Dmb.EffectPlayer;

/// <summary>A value of a material expression: a scalar or a vector of up to four components.</summary>
readonly record struct ExpressionValue(Vector4 V, int N)
{
    public static ExpressionValue Scalar(float x) => new(new Vector4(x), 1);

    public float X => V.X;

    public float this[int i] => N == 1 ? V.X : i switch { 0 => V.X, 1 => V.Y, 2 => V.Z, _ => V.W };

    /// <summary>The value widened to four components, a scalar repeated as shader code would.</summary>
    public Vector4 Widened => N == 1 ? new Vector4(V.X) : V;
}

/// <summary>
/// A material's dynamic expression, as the library writes it out from the compiled byte code
/// (<c>Material.DynamicExpressions</c>), compiled into a small tree that can be evaluated with the
/// values a particle gives its render attributes (<c>$NAME</c>). It understands the shapes the
/// game's particle model materials use: numbers, render attributes, <c>time</c>, locals assigned
/// before the <c>return</c>, arithmetic, comparisons, the conditional operator, swizzles and a set of
/// functions (<c>float2</c> to <c>float4</c>, <c>sin</c>, <c>cos</c>, <c>frac</c>, <c>clamp</c>,
/// <c>saturate</c>, <c>lerp</c>, <c>RemapVal</c> and the like). Anything else makes <see cref="Compile"/>
/// fail with a reason, and the material keeps its own value for that parameter.
/// </summary>
sealed class MaterialExpression
{
    abstract record Node;
    sealed record Number(float Value) : Node;
    sealed record Attribute(string Name) : Node;
    sealed record Local(string Name) : Node;
    sealed record Time : Node;
    sealed record Unary(char Op, Node Operand) : Node;
    sealed record Binary(string Op, Node Left, Node Right) : Node;
    sealed record Conditional(Node Test, Node Then, Node Else) : Node;
    sealed record Call(string Function, Node[] Arguments) : Node;
    sealed record Swizzle(Node Operand, int[] Components) : Node;

    readonly List<(string Local, Node Value)> assignments;
    readonly Node result;

    /// <summary>The render attributes the expression reads, as written (with the <c>$</c>).</summary>
    public IReadOnlySet<string> Attributes { get; }

    /// <summary>Whether it reads the time.</summary>
    public bool ReadsTime { get; }

    /// <summary>The expression as text.</summary>
    public string Text { get; }

    MaterialExpression(string text, List<(string, Node)> assignments, Node result, HashSet<string> attributes, bool time)
    {
        Text = text;
        this.assignments = assignments;
        this.result = result;
        Attributes = attributes;
        ReadsTime = time;
    }

    static readonly HashSet<string> Functions = new(StringComparer.Ordinal)
    {
        "float", "float2", "float3", "float4", "sin", "cos", "tan", "frac", "floor", "ceil", "abs", "sqrt",
        "pow", "min", "max", "clamp", "saturate", "lerp", "RemapVal", "RemapValClamped", "step", "smoothstep",
        "dot", "length", "normalize", "exp", "log", "sign", "fmod", "exists",
    };

    /// <summary>Compiles the text, or returns null with the reason it cannot be evaluated.</summary>
    public static MaterialExpression? Compile(string text, out string? reason)
    {
        try
        {
            var parser = new Parser(text.Replace("\\n", "\n", StringComparison.Ordinal));
            var (assignments, value) = parser.Program();
            reason = null;
            return new MaterialExpression(text, assignments, value, parser.Attributes, parser.ReadsTime);
        }
        catch (FormatException e)
        {
            reason = e.Message;
            return null;
        }
    }

    /// <summary>Evaluates the expression. <paramref name="attribute"/> gives each render attribute's value.</summary>
    public ExpressionValue Evaluate(Func<string, ExpressionValue> attribute, float time)
    {
        var locals = new Dictionary<string, ExpressionValue>(StringComparer.Ordinal);
        foreach (var (name, value) in assignments)
        {
            locals[name] = Eval(value, attribute, time, locals);
        }

        return Eval(result, attribute, time, locals);
    }

    static ExpressionValue Eval(Node node, Func<string, ExpressionValue> attribute, float time, Dictionary<string, ExpressionValue> locals)
    {
        ExpressionValue E(Node n) => Eval(n, attribute, time, locals);
        switch (node)
        {
            case Number n:
                return ExpressionValue.Scalar(n.Value);
            case Attribute a:
                return attribute(a.Name);
            case Local l:
                return locals.TryGetValue(l.Name, out var v) ? v : ExpressionValue.Scalar(0f);
            case Time:
                return ExpressionValue.Scalar(time);
            case Unary u:
                var operand = E(u.Operand);
                return u.Op == '-' ? Map(operand, x => -x) : Map(operand, x => x == 0f ? 1f : 0f);
            case Binary b:
                var left = E(b.Left);
                var right = E(b.Right);
                return b.Op switch
                {
                    "+" => Zip(left, right, (x, y) => x + y),
                    "-" => Zip(left, right, (x, y) => x - y),
                    "*" => Zip(left, right, (x, y) => x * y),
                    "/" => Zip(left, right, (x, y) => y == 0f ? 0f : x / y),
                    "<" => Zip(left, right, (x, y) => x < y ? 1f : 0f),
                    ">" => Zip(left, right, (x, y) => x > y ? 1f : 0f),
                    "<=" => Zip(left, right, (x, y) => x <= y ? 1f : 0f),
                    ">=" => Zip(left, right, (x, y) => x >= y ? 1f : 0f),
                    "==" => Zip(left, right, (x, y) => x == y ? 1f : 0f),
                    "!=" => Zip(left, right, (x, y) => x != y ? 1f : 0f),
                    "&&" => Zip(left, right, (x, y) => x != 0f && y != 0f ? 1f : 0f),
                    "||" => Zip(left, right, (x, y) => x != 0f || y != 0f ? 1f : 0f),
                    _ => throw new InvalidOperationException(b.Op),
                };
            case Conditional c:
                return E(c.Test).X != 0f ? E(c.Then) : E(c.Else);
            case Swizzle s:
                var value = E(s.Operand);
                var picked = new float[4];
                for (var i = 0; i < s.Components.Length; i++)
                {
                    picked[i] = value[s.Components[i]];
                }

                return new ExpressionValue(new Vector4(picked[0], picked[1], picked[2], picked[3]), s.Components.Length);
            case Call call:
                return CallFunction(call.Function, call.Arguments.Select(E).ToArray());
            default:
                throw new InvalidOperationException(node.GetType().Name);
        }
    }

    static ExpressionValue Map(ExpressionValue a, Func<float, float> f)
        => new(new Vector4(f(a.V.X), f(a.V.Y), f(a.V.Z), f(a.V.W)), a.N);

    static ExpressionValue Zip(ExpressionValue a, ExpressionValue b, Func<float, float, float> f)
    {
        var n = Math.Max(a.N, b.N);
        var x = a.Widened;
        var y = b.Widened;
        return new(new Vector4(f(x.X, y.X), f(x.Y, y.Y), f(x.Z, y.Z), f(x.W, y.W)), n);
    }

    static ExpressionValue Zip3(ExpressionValue a, ExpressionValue b, ExpressionValue c, Func<float, float, float, float> f)
    {
        var n = Math.Max(a.N, Math.Max(b.N, c.N));
        var x = a.Widened;
        var y = b.Widened;
        var z = c.Widened;
        return new(new Vector4(f(x.X, y.X, z.X), f(x.Y, y.Y, z.Y), f(x.Z, y.Z, z.Z), f(x.W, y.W, z.W)), n);
    }

    static ExpressionValue CallFunction(string name, ExpressionValue[] a)
    {
        static float Frac(float x) => x - MathF.Floor(x);
        static float Remap(float v, float a, float b, float c, float d) => b == a ? c : c + (v - a) * (d - c) / (b - a);

        switch (name)
        {
            case "float" or "float2" or "float3" or "float4":
                var want = name == "float" ? 1 : name[^1] - '0';
                var parts = new List<float>();
                foreach (var arg in a)
                {
                    for (var i = 0; i < arg.N; i++)
                    {
                        parts.Add(arg[i]);
                    }
                }

                if (parts.Count == 1 && want > 1)
                {
                    parts = Enumerable.Repeat(parts[0], want).ToList();
                }

                while (parts.Count < 4)
                {
                    parts.Add(0f);
                }

                return new ExpressionValue(new Vector4(parts[0], parts[1], parts[2], parts[3]), want);
            case "sin": return Map(a[0], MathF.Sin);
            case "cos": return Map(a[0], MathF.Cos);
            case "tan": return Map(a[0], MathF.Tan);
            case "frac": return Map(a[0], Frac);
            case "floor": return Map(a[0], MathF.Floor);
            case "ceil": return Map(a[0], MathF.Ceiling);
            case "abs": return Map(a[0], MathF.Abs);
            case "sqrt": return Map(a[0], x => MathF.Sqrt(MathF.Max(x, 0f)));
            case "exp": return Map(a[0], MathF.Exp);
            case "log": return Map(a[0], x => x > 0f ? MathF.Log(x) : 0f);
            case "sign": return Map(a[0], x => MathF.Sign(x));
            case "saturate": return Map(a[0], x => Math.Clamp(x, 0f, 1f));
            case "normalize":
                var len = MathF.Sqrt(Enumerable.Range(0, a[0].N).Sum(i => a[0][i] * a[0][i]));
                return Map(a[0], x => len > 0f ? x / len : 0f);
            case "length": return ExpressionValue.Scalar(MathF.Sqrt(Enumerable.Range(0, a[0].N).Sum(i => a[0][i] * a[0][i])));
            case "dot": return ExpressionValue.Scalar(Enumerable.Range(0, Math.Max(a[0].N, a[1].N)).Sum(i => a[0][i] * a[1][i]));
            case "pow": return Zip(a[0], a[1], (x, y) => MathF.Pow(MathF.Max(x, 0f), y));
            case "min": return Zip(a[0], a[1], MathF.Min);
            case "max": return Zip(a[0], a[1], MathF.Max);
            case "fmod": return Zip(a[0], a[1], (x, y) => y == 0f ? 0f : x % y);
            case "step": return Zip(a[0], a[1], (edge, x) => x >= edge ? 1f : 0f);
            case "clamp": return Zip3(a[0], a[1], a[2], (x, lo, hi) => MathF.Min(MathF.Max(x, lo), hi));
            case "lerp": return Zip3(a[0], a[1], a[2], (x, y, t) => x + (y - x) * t);
            case "smoothstep":
                return Zip3(a[0], a[1], a[2], (lo, hi, x) =>
                {
                    var t = hi == lo ? 0f : Math.Clamp((x - lo) / (hi - lo), 0f, 1f);
                    return t * t * (3f - 2f * t);
                });
            case "RemapVal":
                return ExpressionValue.Scalar(Remap(a[0].X, a[1].X, a[2].X, a[3].X, a[4].X));
            case "RemapValClamped":
                var t2 = a[2].X == a[1].X ? 0f : Math.Clamp((a[0].X - a[1].X) / (a[2].X - a[1].X), 0f, 1f);
                return ExpressionValue.Scalar(a[3].X + (a[4].X - a[3].X) * t2);
            case "exists":
                // Only compiled when the attribute it names is set, so it is always there.
                return ExpressionValue.Scalar(1f);
            default:
                throw new InvalidOperationException(name);
        }
    }

    sealed class Parser(string text)
    {
        int pos;
        readonly HashSet<string> locals = new(StringComparer.Ordinal);
        public HashSet<string> Attributes { get; } = new(StringComparer.OrdinalIgnoreCase);
        public bool ReadsTime { get; private set; }

        public (List<(string, Node)> Assignments, Node Result) Program()
        {
            var assignments = new List<(string, Node)>();
            while (true)
            {
                Skip();
                if (pos >= text.Length)
                {
                    throw new FormatException("it has no return statement");
                }

                var start = pos;
                var word = Identifier();
                if (word == "return")
                {
                    var value = Expression();
                    Skip();
                    if (pos < text.Length && text[pos] == ';')
                    {
                        pos++;
                    }

                    Skip();
                    if (pos < text.Length)
                    {
                        throw new FormatException("it has statements after the return");
                    }

                    return (assignments, value);
                }

                Skip();
                if (word == null || pos >= text.Length || text[pos] != '=' || (pos + 1 < text.Length && text[pos + 1] == '='))
                {
                    pos = start;
                    throw new FormatException($"it has a statement that is not understood at \"{Excerpt()}\"");
                }

                pos++;
                var assigned = Expression();
                Expect(';');
                locals.Add(word);
                assignments.Add((word, assigned));
            }
        }

        string Excerpt() => text[pos..Math.Min(text.Length, pos + 24)];

        void Skip()
        {
            while (pos < text.Length && char.IsWhiteSpace(text[pos]))
            {
                pos++;
            }
        }

        void Expect(char c)
        {
            Skip();
            if (pos >= text.Length || text[pos] != c)
            {
                throw new FormatException($"'{c}' was expected at \"{Excerpt()}\"");
            }

            pos++;
        }

        bool Accept(string op)
        {
            Skip();
            if (string.CompareOrdinal(text, pos, op, 0, op.Length) == 0)
            {
                // Do not take "<" from "<=", nor "=" alone.
                if (op.Length == 1 && pos + 1 < text.Length && "<>=!".Contains(op[0], StringComparison.Ordinal) && text[pos + 1] == '=')
                {
                    return false;
                }

                pos += op.Length;
                return true;
            }

            return false;
        }

        string? Identifier()
        {
            Skip();
            var start = pos;
            while (pos < text.Length && (char.IsLetterOrDigit(text[pos]) || text[pos] == '_' || (pos == start && text[pos] == '$')))
            {
                pos++;
            }

            return pos > start ? text[start..pos] : null;
        }

        Node Expression()
        {
            var test = Or();
            if (Accept("?"))
            {
                var then = Expression();
                Expect(':');
                var otherwise = Expression();
                return new Conditional(test, then, otherwise);
            }

            return test;
        }

        Node Or()
        {
            var left = And();
            while (Accept("||"))
            {
                left = new Binary("||", left, And());
            }

            return left;
        }

        Node And()
        {
            var left = Comparison();
            while (Accept("&&"))
            {
                left = new Binary("&&", left, Comparison());
            }

            return left;
        }

        Node Comparison()
        {
            var left = Additive();
            while (true)
            {
                var op = new[] { "<=", ">=", "==", "!=", "<", ">" }.FirstOrDefault(Accept);
                if (op == null)
                {
                    return left;
                }

                left = new Binary(op, left, Additive());
            }
        }

        Node Additive()
        {
            var left = Multiplicative();
            while (true)
            {
                if (Accept("+"))
                {
                    left = new Binary("+", left, Multiplicative());
                }
                else if (Accept("-"))
                {
                    left = new Binary("-", left, Multiplicative());
                }
                else
                {
                    return left;
                }
            }
        }

        Node Multiplicative()
        {
            var left = UnaryExpression();
            while (true)
            {
                if (Accept("*"))
                {
                    left = new Binary("*", left, UnaryExpression());
                }
                else if (Accept("/"))
                {
                    left = new Binary("/", left, UnaryExpression());
                }
                else
                {
                    return left;
                }
            }
        }

        Node UnaryExpression()
        {
            if (Accept("-"))
            {
                return new Unary('-', UnaryExpression());
            }

            if (Accept("!"))
            {
                return new Unary('!', UnaryExpression());
            }

            return Postfix(Primary());
        }

        Node Postfix(Node node)
        {
            while (true)
            {
                Skip();
                if (pos < text.Length && text[pos] == '.' && pos + 1 < text.Length && char.IsLetter(text[pos + 1]))
                {
                    pos++;
                    var letters = Identifier()!;
                    var components = letters.Select(c => c switch { 'x' or 'r' => 0, 'y' or 'g' => 1, 'z' or 'b' => 2, 'w' or 'a' => 3, _ => -1 }).ToArray();
                    if (components.Length is 0 or > 4 || components.Any(c => c < 0))
                    {
                        throw new FormatException($"the swizzle .{letters} is not understood");
                    }

                    node = new Swizzle(node, components);
                    continue;
                }

                return node;
            }
        }

        Node Primary()
        {
            Skip();
            if (pos >= text.Length)
            {
                throw new FormatException("it ends early");
            }

            var c = text[pos];
            if (c == '(')
            {
                pos++;
                var inner = Expression();
                Expect(')');
                return inner;
            }

            if (char.IsDigit(c) || c == '.')
            {
                var start = pos;
                while (pos < text.Length && (char.IsDigit(text[pos]) || text[pos] == '.' || text[pos] == 'e' || text[pos] == 'E'
                    || ((text[pos] == '-' || text[pos] == '+') && (text[pos - 1] == 'e' || text[pos - 1] == 'E'))))
                {
                    pos++;
                }

                if (pos < text.Length && text[pos] == 'f')
                {
                    pos++;
                }

                return new Number(float.Parse(text[start..pos].TrimEnd('f'), NumberStyles.Float, CultureInfo.InvariantCulture));
            }

            var name = Identifier() ?? throw new FormatException($"\"{Excerpt()}\" is not understood");
            if (name.StartsWith('$'))
            {
                Attributes.Add(name);
                return new Attribute(name);
            }

            Skip();
            var isCall = pos < text.Length && text[pos] == '(';
            if (name == "time")
            {
                if (isCall)
                {
                    Expect('(');
                    Expect(')');
                }

                ReadsTime = true;
                return new Time();
            }

            if (isCall)
            {
                if (!Functions.Contains(name))
                {
                    throw new FormatException($"the function {name} is not supported");
                }

                pos++;
                var arguments = new List<Node>();
                Skip();
                if (pos < text.Length && text[pos] == ')')
                {
                    pos++;
                }
                else
                {
                    do
                    {
                        arguments.Add(Expression());
                    }
                    while (Accept(","));

                    Expect(')');
                }

                return new Call(name, [.. arguments]);
            }

            if (locals.Contains(name))
            {
                return new Local(name);
            }

            throw new FormatException($"it reads {name}, which is not a render attribute");
        }
    }
}
