// Independent binary parser: ValveResourceFormat (Source2Viewer), MIT.
// The normal viewer text formatter deliberately uses F6. This audit transport
// uses round-trip numbers instead; it never repairs or modifies model data.
using System.Globalization;
using System.Text;
using System.Text.Json;
using ValveKeyValue;
using ValveResourceFormat;
using ValveResourceFormat.ResourceTypes;

internal static class Program
{
    private const int MaxBytes = 256 * 1024 * 1024;
    private static readonly CultureInfo Invariant = CultureInfo.InvariantCulture;

    private static int Main(string[] args)
    {
        try
        {
            if (args.Length == 1 && args[0] == "--version")
            {
                Console.WriteLine("Dmb.ClothReader 1.0.2; ValveResourceFormat 20.1.7467; round-trip-v1");
                return 0;
            }
            if (args.Length != 4 || args[0] != "-i" || args[2] != "-b"
                || (args[3] != "PHYS" && args[3] != "DATA"))
                throw new InvalidDataException("Usage: Dmb.ClothReader -i model.vmdl_c -b PHYS|DATA");
            using var input = File.OpenRead(args[1]);
            var bytes = ReadBlock(input, args[3]);
            using var stream = new MemoryStream(bytes, writable: false);
            using var reader = new BinaryReader(stream);
            var block = new BinaryKV3(BlockType.Undefined)
                { Resource = null!, Offset = 0, Size = (uint)bytes.Length };
            block.Read(reader);
            if (!block.Data.Root.IsCollection)
                throw new InvalidDataException("Expected an object at the KV3 root.");
            // Build before emitting: a malformed/non-finite value cannot produce
            // a success-looking partial document. The parent also checks exit code.
            using var output = new StringWriter(Invariant);
            WriteValue(output, block.Data.Root, 0);
            Console.OutputEncoding = new UTF8Encoding(false);
            Console.WriteLine($"--- Data for block \"{args[3]}\" ---");
            Console.Write(output.ToString());
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine("Full-precision cloth reader failed: " + error.Message);
            return 1;
        }
    }

    private static byte[] ReadBlock(FileStream stream, string requested)
    {
        if (stream.Length < 16 || stream.Length > MaxBytes)
            throw new InvalidDataException("Resource size is outside the supported range.");
        using var reader = new BinaryReader(stream, Encoding.UTF8, leaveOpen: true);
        if (reader.ReadUInt32() != stream.Length || reader.ReadUInt16() != 12)
            throw new InvalidDataException("Invalid resource size/header; trailing payloads are unsupported.");
        reader.ReadUInt16(); // Resource-specific version.
        long table = 8L + reader.ReadUInt32();
        var count = reader.ReadUInt32();
        long tableEnd = table + 12L * count;
        if (count == 0 || count > 4096 || table < 16 || tableEnd > stream.Length)
            throw new InvalidDataException("Invalid resource block table.");
        var ranges = new List<(long Start, long End)>();
        var matches = new List<(long Start, uint Size)>();
        for (uint i = 0; i < count; i++)
        {
            long entry = table + 12L * i;
            stream.Position = entry;
            var name = Encoding.ASCII.GetString(reader.ReadBytes(4));
            long start = entry + 4 + reader.ReadUInt32();
            var size = reader.ReadUInt32();
            if (start < tableEnd || start + size > stream.Length)
                throw new InvalidDataException("Resource block is out of bounds.");
            if (size > 0) ranges.Add((start, start + size));
            if (name == requested) matches.Add((start, size));
        }
        ranges.Sort((a, b) => a.Start.CompareTo(b.Start));
        for (int i = 1; i < ranges.Count; i++)
            if (ranges[i - 1].End > ranges[i].Start)
                throw new InvalidDataException("Overlapping resource blocks.");
        if (matches.Count != 1 || matches[0].Size == 0 || matches[0].Size > 128 * 1024 * 1024)
            throw new InvalidDataException("Expected exactly one non-empty bounded " + requested + " block.");
        stream.Position = matches[0].Start;
        return reader.ReadBytes(checked((int)matches[0].Size));
    }

    private static void WriteValue(TextWriter output, KVObject value, int depth)
    {
        if (depth > 128) throw new InvalidDataException("KV3 nesting exceeds the audit limit.");
        // Flags are not used as numeric types in the text audit. The primary
        // typed comparison separately preserves every annotation exactly.
        switch (value.ValueType)
        {
            case KVValueType.Collection:
                output.WriteLine("{");
                var names = new HashSet<string>(StringComparer.Ordinal);
                foreach (var child in value.Children)
                {
                    if (!names.Add(child.Key)) throw new InvalidDataException("Duplicate KV3 field.");
                    WriteString(output, child.Key);
                    output.Write(" = ");
                    WriteValue(output, child.Value, depth + 1);
                }
                output.WriteLine("}");
                break;
            case KVValueType.Array:
                output.WriteLine("[");
                foreach (var child in value.Values) WriteValue(output, child, depth + 1);
                output.WriteLine("]");
                break;
            case KVValueType.BinaryBlob:
                output.Write("#[");
                output.Write(Convert.ToHexString(value.AsBlob()));
                output.WriteLine("]");
                break;
            case KVValueType.Null:
                output.WriteLine("null");
                break;
            case KVValueType.Boolean:
                output.WriteLine((bool)value ? "true" : "false");
                break;
            case KVValueType.FloatingPoint:
                float single = value.ToSingle(Invariant);
                if (!float.IsFinite(single)) throw new InvalidDataException("Non-finite Float32.");
                output.WriteLine(single.ToString("G9", Invariant));
                break;
            case KVValueType.FloatingPoint64:
                double number = value.ToDouble(Invariant);
                if (!double.IsFinite(number)) throw new InvalidDataException("Non-finite Float64.");
                // Explicit precision also preserves power-of-two boundaries
                // across runtimes: "R" shortened -2**-25 by one Float64 ULP.
                output.WriteLine(number.ToString("G17", Invariant));
                break;
            case KVValueType.Int16:
            case KVValueType.Int32:
            case KVValueType.Int64:
            case KVValueType.UInt16:
            case KVValueType.UInt32:
            case KVValueType.UInt64:
                output.WriteLine(value.ToString(Invariant));
                break;
            case KVValueType.String:
                WriteString(output, value.ToString(Invariant));
                output.WriteLine();
                break;
            default:
                throw new InvalidDataException("Unsupported KV3 scalar: " + value.ValueType);
        }
    }

    private static void WriteString(TextWriter output, string value)
    {
        output.Write('"');
        output.Write(JsonEncodedText.Encode(value).ToString());
        output.Write('"');
    }
}
