using System.Buffers.Binary;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Dmb.EffectPlayer;

/// <summary>
/// The byte layout of the <c>serve</c> output (see README, "Protocol"). Every message is a 12-byte
/// header followed by its payload:
/// <code>
/// offset 0  4 bytes  magic "DMBE"
/// offset 4  1 byte   kind: 1 = status (UTF-8 JSON), 2 = frame
/// offset 5  3 bytes  zero
/// offset 8  4 bytes  payload length, unsigned, little-endian
/// </code>
/// A frame payload is a 32-byte frame header (<see cref="FrameHeaderSize"/>) followed by one JPEG.
/// </summary>
static class Protocol
{
    public const int Version = 1;
    public const int HeaderSize = 12;
    public const int FrameHeaderSize = 32;
    public const byte KindStatus = 1;
    public const byte KindFrame = 2;
    public static ReadOnlySpan<byte> Magic => "DMBE"u8;

    public static byte[] Header(byte kind, int length)
    {
        var header = new byte[HeaderSize];
        Magic.CopyTo(header);
        header[4] = kind;
        BinaryPrimitives.WriteUInt32LittleEndian(header.AsSpan(8), (uint)length);
        return header;
    }

    [Flags]
    public enum FrameFlags : byte
    {
        None = 0,
        Compare = 1,
        Paused = 2,
        Finished = 4,
    }

    [Flags]
    public enum Edges : byte
    {
        None = 0,
        Left = 1,
        Right = 2,
        Top = 4,
        Bottom = 8,
    }

    /// <summary>Builds a frame payload: the 32-byte frame header and the JPEG.</summary>
    public static byte[] Frame(uint sequence, uint generation, float time, uint wallMs, int width, int height, float coverage, FrameFlags flags, Edges edges, byte[] jpeg)
    {
        var payload = new byte[FrameHeaderSize + jpeg.Length];
        var span = payload.AsSpan();
        BinaryPrimitives.WriteUInt32LittleEndian(span[0..], sequence);
        BinaryPrimitives.WriteUInt32LittleEndian(span[4..], generation);
        BinaryPrimitives.WriteSingleLittleEndian(span[8..], time);
        BinaryPrimitives.WriteUInt32LittleEndian(span[12..], wallMs);
        BinaryPrimitives.WriteUInt16LittleEndian(span[16..], (ushort)width);
        BinaryPrimitives.WriteUInt16LittleEndian(span[18..], (ushort)height);
        BinaryPrimitives.WriteSingleLittleEndian(span[20..], coverage);
        span[24] = (byte)flags;
        span[25] = (byte)edges;
        jpeg.CopyTo(span[FrameHeaderSize..]);
        return payload;
    }
}

/// <summary>
/// Writes protocol messages to stdout on its own thread. Status messages are queued and always
/// written, in order. Frames go through a single slot: a frame that has not been written when the
/// next one arrives is dropped, so a slow reader always gets the newest picture rather than a
/// backlog. A failed write (the reader has gone) raises <see cref="Broken"/>.
/// </summary>
sealed class ProtocolWriter : IDisposable
{
    readonly Stream stream;
    readonly Thread thread;
    readonly object gate = new();
    readonly Queue<byte[]> statuses = new();
    byte[]? frame;
    bool closing;
    bool broken;

    public long FramesSent;
    public long FramesDropped;
    public long BytesSent;

    public event Action? Broken;

    static readonly JsonSerializerOptions Compact = new() { WriteIndented = false };

    public ProtocolWriter(Stream stream)
    {
        this.stream = stream;
        thread = new Thread(Run) { IsBackground = true, Name = "protocol-writer" };
        thread.Start();
    }

    public bool IsBroken => Volatile.Read(ref broken);

    public void Status(JsonObject message)
    {
        var bytes = Encoding.UTF8.GetBytes(message.ToJsonString(Compact));
        lock (gate)
        {
            statuses.Enqueue(bytes);
            Monitor.PulseAll(gate);
        }
    }

    public void Frame(byte[] payload)
    {
        lock (gate)
        {
            if (frame != null)
            {
                FramesDropped++;
            }

            frame = payload;
            Monitor.PulseAll(gate);
        }
    }

    /// <summary>Whether a frame is still waiting to be written.</summary>
    public bool FramePending
    {
        get
        {
            lock (gate)
            {
                return frame != null;
            }
        }
    }

    void Run()
    {
        var batch = new List<(byte Kind, byte[] Payload)>();
        while (true)
        {
            lock (gate)
            {
                while (statuses.Count == 0 && frame == null && !closing)
                {
                    Monitor.Wait(gate);
                }

                if (statuses.Count == 0 && frame == null && closing)
                {
                    return;
                }

                while (statuses.Count > 0)
                {
                    batch.Add((Protocol.KindStatus, statuses.Dequeue()));
                }

                if (frame != null)
                {
                    batch.Add((Protocol.KindFrame, frame));
                    frame = null;
                }
            }

            try
            {
                foreach (var (kind, payload) in batch)
                {
                    stream.Write(Protocol.Header(kind, payload.Length));
                    stream.Write(payload);
                    BytesSent += Protocol.HeaderSize + payload.Length;
                    if (kind == Protocol.KindFrame)
                    {
                        FramesSent++;
                    }
                }

                stream.Flush();
            }
            catch (Exception)
            {
                Volatile.Write(ref broken, true);
                Broken?.Invoke();
                return;
            }

            batch.Clear();
        }
    }

    /// <summary>Writes what is queued (up to <paramref name="wait"/>) and stops the thread.</summary>
    public void Close(TimeSpan wait)
    {
        lock (gate)
        {
            closing = true;
            Monitor.PulseAll(gate);
        }

        thread.Join(wait);
    }

    public void Dispose()
    {
        bool alreadyClosing;
        lock (gate)
        {
            alreadyClosing = closing;
        }

        Close(alreadyClosing ? TimeSpan.Zero : TimeSpan.FromMilliseconds(500));
    }
}

/// <summary>Reads command lines from stdin on its own thread and signals each one and end of file.</summary>
sealed class CommandReader
{
    readonly Thread thread;
    readonly Queue<string> lines = new();
    readonly Lock gate = new();
    readonly AutoResetEvent signal;
    volatile bool ended;

    public CommandReader(Stream input, AutoResetEvent signal)
    {
        this.signal = signal;
        var reader = new StreamReader(input, new UTF8Encoding(false));
        thread = new Thread(() =>
        {
            try
            {
                string? line;
                while ((line = reader.ReadLine()) != null)
                {
                    lock (gate)
                    {
                        lines.Enqueue(line);
                    }

                    signal.Set();
                }
            }
            catch (Exception)
            {
                // A broken pipe is the same as end of file.
            }

            ended = true;
            signal.Set();
        })
        { IsBackground = true, Name = "command-reader" };
        thread.Start();
    }

    /// <summary>Whether stdin has reached end of file (the reader has gone or closed the pipe).</summary>
    public bool Ended => ended;

    public bool TryTake(out string line)
    {
        lock (gate)
        {
            return lines.TryDequeue(out line!);
        }
    }
}
