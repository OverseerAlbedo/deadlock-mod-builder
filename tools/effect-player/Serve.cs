using System.Diagnostics;
using System.Globalization;
using System.Numerics;
using System.Runtime.InteropServices;
using System.Text.Json.Nodes;
using Microsoft.Extensions.Logging;
using Microsoft.Win32.SafeHandles;
using SkiaSharp;
using ValvePak;
using ValveResourceFormat.Renderer.SceneNodes;
using ValveResourceFormat.ResourceTypes;

namespace Dmb.EffectPlayer;

/// <summary>Settings for <c>serve</c>, from its command line.</summary>
sealed class ServeOptions
{
    public string Pak { get; private set; } = "";
    public int Width { get; private set; } = 640;
    public int Height { get; private set; } = 480;
    public int Fps { get; private set; } = 30;
    public int Quality { get; private set; } = 80;
    public int Msaa { get; private set; } = 4;
    public (byte R, byte G, byte B) Background { get; private set; } = (0x0b, 0x0c, 0x10);
    public float Exposure { get; private set; }
    public bool Verbose { get; private set; }
    public Version GLVersion { get; private set; } = ValveResourceFormat.Renderer.GLEnvironment.RequiredVersion;

    public static ServeOptions Parse(string[] args)
    {
        var o = new ServeOptions();
        for (var i = 0; i < args.Length; i++)
        {
            string Next() => i + 1 < args.Length ? args[++i] : throw new ArgumentException($"{args[i]} needs a value.");
            switch (args[i])
            {
                case "--pak": o.Pak = Next(); break;
                case "--size":
                    var size = Next().Split('x');
                    o.Width = int.Parse(size[0], CultureInfo.InvariantCulture);
                    o.Height = int.Parse(size[1], CultureInfo.InvariantCulture);
                    break;
                case "--fps": o.Fps = int.Parse(Next(), CultureInfo.InvariantCulture); break;
                case "--quality": o.Quality = int.Parse(Next(), CultureInfo.InvariantCulture); break;
                case "--msaa": o.Msaa = int.Parse(Next(), CultureInfo.InvariantCulture); break;
                case "--background": o.Background = Server.ParseColor(Next()); break;
                case "--exposure": o.Exposure = float.Parse(Next(), CultureInfo.InvariantCulture); break;
                case "--verbose": o.Verbose = true; break;
                // Test switch: request a context version, for example one that does not exist.
                case "--gl-version": o.GLVersion = Version.Parse(Next()); break;
                default: throw new ArgumentException($"Unknown option {args[i]}.");
            }
        }

        if (o.Pak.Length == 0)
        {
            throw new ArgumentException("--pak is required.");
        }

        if (o.Fps < 1 || o.Fps > Server.MaxFps)
        {
            throw new ArgumentException($"--fps must be between 1 and {Server.MaxFps}.");
        }

        return o;
    }
}

/// <summary>Raised by a command handler with a sentence for the reply.</summary>
sealed class CommandException(string message) : Exception(message);

/// <summary>
/// The long-lived player: one hidden OpenGL context, commands on stdin, frames and status on stdout.
/// See README, "serve".
/// </summary>
sealed class Server : IDisposable
{
    const float DefaultYaw = 225f;
    const float DefaultPitch = 20f;
    const float OrbitDegreesPerPixel = 0.4f;
    const float ZoomPerStep = 0.85f;
    const float PrerollSeconds = 2.5f;
    const int MinSize = 64;
    const int MaxSize = 2048;
    public const int MaxFps = 240;
    const int MaxCatchUpSteps = 8;

    readonly ServeOptions options;
    readonly ProtocolWriter writer;
    readonly CollectingLogger logger;
    readonly Stopwatch clock = Stopwatch.StartNew();
    readonly AutoResetEvent wake = new(false);
    CommandReader? reader;
    EffectLoader? loader;
    RenderHost? host;
    RenderView? gameView;
    readonly Dictionary<(uint Background, float Exposure), Vector3> calibrations = [];

    // Settings
    int width;
    int height;
    int fps;
    int quality;
    bool compare;
    bool loop = true;
    bool paused;
    float speed = 1f;
    (byte R, byte G, byte B) background;
    (byte R, byte G, byte B) achievedBackground;
    float exposure;

    // Camera
    Vector3 target;
    float distance = 346f;
    float yaw = DefaultYaw;
    float pitch = DefaultPitch;
    bool autoView = true;

    // Where a moving effect's control point 0 is now; the camera keeps this offset (rule 5).
    Vector3 follow;

    // The loaded effect
    Loaded? effect;
    uint generation;
    uint sequence;
    float playTime;
    bool finished;
    bool dirty;
    bool quit;
    double readyAt;
    byte[] pixels = [];
    byte[] scratch = [];

    // Where the time of a played frame goes, since the last stats command (see Stats).
    long catchUpSteps;
    long scheduleResets;
    readonly Stopwatch stageClock = new();
    double simulateMs;
    double drawMs;
    double readBackMs;
    double coverageMs;
    double encodeMs;
    long stagedFrames;
    long simulatedSteps;

    sealed class Loaded
    {
        public required string Path;
        public required float Radius;
        public required bool Rules;
        public string? OverlayPath;
        public Package? Overlay;
        public ValveResourceFormat.Resource? YoursResource;
        public ParticleSceneNode? Yours;
        public ValveResourceFormat.Resource? GameResource;
        public ParticleSceneNode? Game;
        public EffectRules.Plan Plan = new();
        public List<string> Applied = [];
        public JsonObject? Notes;
        public Bounds Bounds = Bounds.Empty;
        public SortedSet<int> MovedControlPoints = [];
        public bool Seeded;
        public bool Floor = true;
    }

    public Server(ServeOptions options, ProtocolWriter writer, CollectingLogger logger)
    {
        this.options = options;
        this.writer = writer;
        this.logger = logger;
        width = options.Width;
        height = options.Height;
        fps = options.Fps;
        quality = options.Quality;
        background = options.Background;
        exposure = options.Exposure;
        writer.Broken += () =>
        {
            quit = true;
            wake.Set();
        };
    }

    public static int Run(string[] args)
    {
        // Only the protocol goes to stdout: anything the libraries print goes to stderr.
        var stdout = Console.OpenStandardOutput();
        Console.SetOut(Console.Error);

        ServeOptions options;
        try
        {
            options = ServeOptions.Parse(args);
        }
        catch (Exception e) when (e is ArgumentException or FormatException or OverflowException)
        {
            Console.Error.WriteLine(e.Message);
            return ExitCodes.Usage;
        }

        using var writer = new ProtocolWriter(stdout);
        var logger = new CollectingLogger(echo: true, options.Verbose ? LogLevel.Debug : LogLevel.Warning);
        using var server = new Server(options, writer, logger);

        try
        {
            server.Start();
        }
        catch (OpenGLUnavailableException e)
        {
            Console.Error.WriteLine(e.Message);
            writer.Status(new JsonObject { ["type"] = "error", ["fatal"] = true, ["code"] = "opengl", ["message"] = e.Message });
            writer.Close(TimeSpan.FromSeconds(1));
            return ExitCodes.OpenGLUnavailable;
        }
        catch (Exception e)
        {
            var message = $"The effect player could not start: {e.Message}";
            Console.Error.WriteLine(message);
            writer.Status(new JsonObject { ["type"] = "error", ["fatal"] = true, ["code"] = "start", ["message"] = message });
            writer.Close(TimeSpan.FromSeconds(1));
            return ExitCodes.Failure;
        }

        int code;
        try
        {
            code = server.Loop();
        }
        catch (Exception e)
        {
            var message = $"The effect player stopped: {e.GetType().Name}: {e.Message}";
            Console.Error.WriteLine(message);
            writer.Status(new JsonObject { ["type"] = "error", ["fatal"] = true, ["code"] = "internal", ["message"] = message });
            code = ExitCodes.Failure;
        }

        // A reader that has gone (stdin closed) may also have stopped reading stdout: wait briefly.
        writer.Close(TimeSpan.FromMilliseconds(code == ExitCodes.Ok ? 200 : 500));

        // Tearing the context down can wait on the driver; the process is ending anyway.
        var watchdog = new Thread(() =>
        {
            Thread.Sleep(1000);
            Environment.Exit(code);
        })
        { IsBackground = true };
        watchdog.Start();
        return code;
    }

    void Start()
    {
        if (!File.Exists(options.Pak))
        {
            throw new FileNotFoundException($"The game archive {options.Pak} was not found.");
        }

        loader = new EffectLoader(null, options.Pak, logger);
        width = Math.Clamp(width, MinSize, MaxSize);
        height = Math.Clamp(height, MinSize, MaxSize);
        host = new RenderHost(loader, logger, width, height, options.GLVersion, options.Msaa, initializePrimary: true);
        host.Renderer.Postprocess.ExposureCompensation = exposure;
        Calibrate();
        Allocate();
        readyAt = clock.Elapsed.TotalMilliseconds;

        writer.Status(new JsonObject
        {
            ["type"] = "ready",
            ["protocol"] = Protocol.Version,
            ["version"] = typeof(Program).Assembly.GetName().Version?.ToString(3),
            ["library"] = typeof(ValveResourceFormat.Renderer.Renderer).Assembly.GetName().Version?.ToString(),
            ["gl"] = new JsonObject { ["vendor"] = host.GLVendor, ["renderer"] = host.GLRenderer, ["version"] = host.GLVersion },
            ["startMs"] = Math.Round(readyAt, 1),
            ["settings"] = Settings(),
        });

        reader = new CommandReader(Console.OpenStandardInput(), wake);
    }

    JsonObject Settings() => new()
    {
        ["width"] = width,
        ["height"] = height,
        ["fps"] = fps,
        ["quality"] = quality,
        ["background"] = Hex(background),
        ["brightness"] = exposure,
        ["speed"] = speed,
        ["loop"] = loop,
        ["paused"] = paused,
        ["compare"] = compare,
    };

    int Loop()
    {
        using var timer = new HighResolutionTimer();
        var period = 1.0 / fps;
        var nextTick = clock.Elapsed.TotalSeconds;
        var wasPlaying = false;

        while (!quit)
        {
            // Every queued command is handled before the next frame (orbit arrives at mouse rate).
            var redraw = Redraw.None;
            while (reader!.TryTake(out var line))
            {
                var r = Handle(line);
                redraw = r > redraw ? r : redraw;
                if (quit)
                {
                    break;
                }
            }

            if (quit || reader.Ended)
            {
                break;
            }

            period = 1.0 / fps;
            var now = clock.Elapsed.TotalSeconds;
            var playing = effect != null && !paused && !finished;

            // Playing again (resume, loop on, a new effect): the schedule starts from now, so the
            // time spent paused or finished is not caught up.
            if (playing && !wasPlaying)
            {
                nextTick = now;
            }

            wasPlaying = playing;

            // A new version or layout is shown at once; a view change waits for the next tick while playing.
            if (playing && redraw == Redraw.Now)
            {
                nextTick = now;
            }

            var renderNow = !playing && redraw != Redraw.None;

            if (playing && now >= nextTick)
            {
                // Whole periods missed (a frame that took longer than one period) are simulated
                // without drawing, a few at most, so the effect keeps its real speed when frames
                // cannot keep up with the rate.
                var behind = Math.Min((int)Math.Floor((now - nextTick) / period), MaxCatchUpSteps);
                for (var k = 0; k < behind && !finished; k++)
                {
                    Step((float)period * speed);
                    catchUpSteps++;
                }

                nextTick += period * behind;
                if (!finished)
                {
                    Step((float)period * speed);
                }

                RenderFrame();
                nextTick += period;

                // Never fall behind for good: if a step ran late, start the schedule again from now.
                if (now - nextTick > 2 * period)
                {
                    nextTick = now + period;
                    scheduleResets++;
                }
            }
            else if (effect != null && !playing && (renderNow || dirty))
            {
                UpdateViews(0f);

                RenderFrame();
                if (playing)
                {
                    nextTick = clock.Elapsed.TotalSeconds + period;
                }
            }

            dirty = false;

            if (effect != null && !paused && !finished)
            {
                var wait = nextTick - clock.Elapsed.TotalSeconds;
                if (wait > 0)
                {
                    timer.Wait(wait, wake);
                }
            }
            else
            {
                wake.WaitOne();
            }
        }

        return ExitCodes.Ok;
    }

    /// <summary>
    /// Steps every view by the same time. The two versions' renderers keep one clock, so functions
    /// that read the time see the same value in both.
    /// </summary>
    void UpdateViews(float dt)
    {
        if (compare && gameView != null)
        {
            gameView.Renderer.Uptime = host!.Primary.Renderer.Uptime;
        }

        foreach (var view in Views())
        {
            view.Update(dt);
        }

        // Model particles take their material variables after each update, before any draw.
        foreach (var node in Nodes())
        {
            if (materialVars.TryGetValue(node, out var vars))
            {
                vars.Apply();
            }
        }
    }

    readonly Dictionary<ParticleSceneNode, ModelMaterialVars> materialVars = [];

    IEnumerable<RenderView> Views()
    {
        yield return host!.Primary;
        if (compare && gameView != null)
        {
            yield return gameView;
        }
    }

    /// <summary>Handles one command line. Returns whether a frame should be drawn right away.</summary>
    Redraw Handle(string line)
    {
        if (string.IsNullOrWhiteSpace(line))
        {
            return Redraw.None;
        }

        JsonNode? id = null;
        var command = "";
        var started = clock.Elapsed.TotalMilliseconds;
        try
        {
            if (JsonNode.Parse(line) is not JsonObject message)
            {
                throw new CommandException("A command must be one JSON object on one line.");
            }

            id = message["id"]?.DeepClone();
            command = message["cmd"]?.GetValue<string>() ?? throw new CommandException("The command has no \"cmd\".");
            var details = new JsonObject();
            var draw = Execute(command, message, details);
            var reply = new JsonObject
            {
                ["type"] = "reply",
                ["id"] = id,
                ["command"] = command,
                ["ok"] = true,
                ["generation"] = generation,
                ["elapsedMs"] = Math.Round(clock.Elapsed.TotalMilliseconds - started, 1),
            };

            foreach (var (key, value) in details.ToList())
            {
                details.Remove(key);
                reply[key] = value;
            }

            writer.Status(reply);
            return draw;
        }
        catch (Exception e)
        {
            var sentence = e switch
            {
                CommandException => e.Message,
                System.Text.Json.JsonException or FormatException => $"The command could not be read: {e.Message}",
                _ => $"The command failed: {e.GetType().Name}: {e.Message}",
            };

            if (e is not CommandException)
            {
                Console.Error.WriteLine($"[Error] {command}: {e}".Replace('\r', ' ').Replace('\n', ' '));
            }

            writer.Status(new JsonObject
            {
                ["type"] = "reply",
                ["id"] = id,
                ["command"] = command,
                ["ok"] = false,
                ["error"] = sentence,
                ["generation"] = generation,
                ["elapsedMs"] = Math.Round(clock.Elapsed.TotalMilliseconds - started, 1),
            });
            return Redraw.None;
        }
    }

    enum Redraw
    {
        None,
        View,
        Now,
    }

    static float Number(JsonObject message, string key)
    {
        var node = message[key] ?? throw new CommandException($"The command needs \"{key}\".");
        try
        {
            return node.GetValue<float>();
        }
        catch (Exception)
        {
            throw new CommandException($"\"{key}\" must be a number.");
        }
    }

    static bool Flag(JsonObject message, string key, bool fallback)
    {
        var node = message[key];
        if (node == null)
        {
            return fallback;
        }

        try
        {
            return node.GetValue<bool>();
        }
        catch (Exception)
        {
            throw new CommandException($"\"{key}\" must be true or false.");
        }
    }

    static string? Text(JsonObject message, string key)
    {
        var node = message[key];
        if (node == null)
        {
            return null;
        }

        try
        {
            return node.GetValue<string>();
        }
        catch (Exception)
        {
            throw new CommandException($"\"{key}\" must be text.");
        }
    }

    Redraw Execute(string command, JsonObject message, JsonObject reply)
    {
        switch (command)
        {
            case "load":
                Load(message, reply);
                return Redraw.Now;

            case "overlay":
                if (effect == null)
                {
                    throw new CommandException("No effect is loaded.");
                }

                ReplaceOverlay(Text(message, "overlay"), reply);
                return Redraw.Now;

            case "compare":
                SetCompare(Flag(message, "on", true));
                if (effect != null)
                {
                    reply["loaded"] = LoadedStatus();
                }

                return Redraw.Now;

            case "pause":
                paused = true;
                return Redraw.View;

            case "resume":
                paused = false;
                return Redraw.None;

            case "restart":
                RestartAll();
                finished = false;
                generation++;
                return Redraw.Now;

            case "loop":
                loop = Flag(message, "on", true);
                if (loop && finished)
                {
                    RestartAll();
                    finished = false;
                }

                return Redraw.None;

            case "speed":
                var value = Number(message, "value");
                if (!float.IsFinite(value) || value <= 0f || value > 8f)
                {
                    throw new CommandException("The speed must be above 0 and at most 8.");
                }

                speed = value;
                return Redraw.None;

            case "size":
                var w = (int)Number(message, "width");
                var h = (int)Number(message, "height");
                if (w < MinSize || h < MinSize || w > MaxSize || h > MaxSize)
                {
                    throw new CommandException($"The size must be between {MinSize} and {MaxSize} pixels each way.");
                }

                width = w;
                height = h;
                Layout();
                Allocate();
                Reframe();
                return Redraw.Now;

            case "background":
                background = ParseColor(Text(message, "color") ?? throw new CommandException("The command needs \"color\"."));
                Calibrate();
                reply["achieved"] = Hex(achievedBackground);
                return Redraw.View;

            case "brightness":
                var stops = Number(message, "stops");
                if (!float.IsFinite(stops) || stops < -4f || stops > 6f)
                {
                    throw new CommandException("The brightness must be between -4 and +6 stops.");
                }

                exposure = stops;
                Calibrate();
                reply["achieved"] = Hex(achievedBackground);
                return Redraw.View;

            case "fit":
                if (effect != null)
                {
                    Fit(resetAngles: false);
                    reply["camera"] = CameraJson();
                }

                return Redraw.Now;

            case "resetView":
                if (effect != null)
                {
                    Fit(resetAngles: true);
                    reply["camera"] = CameraJson();
                }

                return Redraw.Now;

            case "orbit":
                var dx = message["dx"] != null ? Number(message, "dx") : 0f;
                var dy = message["dy"] != null ? Number(message, "dy") : 0f;
                yaw = (yaw - dx * OrbitDegreesPerPixel) % 360f;
                pitch = Math.Clamp(pitch + dy * OrbitDegreesPerPixel, -89f, 89f);
                autoView = false;
                ApplyCamera();
                return Redraw.View;

            case "zoom":
                var steps = Number(message, "steps");
                distance = Math.Clamp(distance * MathF.Pow(ZoomPerStep, steps), 1f, 100000f);
                autoView = false;
                ApplyCamera();
                return Redraw.View;

            case "fps":
                var rate = (int)Number(message, "value");
                if (rate < 1 || rate > MaxFps)
                {
                    throw new CommandException($"The frame rate must be between 1 and {MaxFps}.");
                }

                fps = rate;
                return Redraw.None;

            case "quality":
                var q = (int)Number(message, "value");
                if (q < 10 || q > 100)
                {
                    throw new CommandException("The JPEG quality must be between 10 and 100.");
                }

                quality = q;
                return Redraw.View;

            case "stats":
                if (Flag(message, "collect", false))
                {
                    // Measure what is still referenced rather than garbage waiting to be collected.
                    GC.Collect();
                    GC.WaitForPendingFinalizers();
                    GC.Collect();
                }

                reply["stats"] = Stats();
                return Redraw.None;

            case "quit":
                quit = true;
                return Redraw.None;

            default:
                throw new CommandException($"Unknown command \"{command}\".");
        }
    }

    JsonObject Stats()
    {
        using var process = Process.GetCurrentProcess();
        return new JsonObject
        {
            ["framesSent"] = writer.FramesSent,
            ["framesDropped"] = writer.FramesDropped,
            ["bytesSent"] = writer.BytesSent,
            ["generation"] = generation,
            ["workingSetMB"] = Math.Round(process.WorkingSet64 / 1048576.0, 1),
            ["privateMB"] = Math.Round(process.PrivateMemorySize64 / 1048576.0, 1),
            ["managedMB"] = Math.Round(GC.GetTotalMemory(false) / 1048576.0, 1),
            ["cpuSeconds"] = Math.Round(process.TotalProcessorTime.TotalSeconds, 3),
            ["uptimeSeconds"] = Math.Round(clock.Elapsed.TotalSeconds, 3),
            ["settings"] = Settings(),
            ["stages"] = Stages(),
        };
    }

    /// <summary>
    /// Mean milliseconds per drawn frame of each stage since the previous stats command: simulating
    /// (per step), drawing (submitting the views), reading the pixels back (which waits for the
    /// GPU to finish the frame), the coverage scan and the JPEG encode; and how many undrawn steps
    /// kept the effect at real speed, and how often the schedule was started again.
    /// </summary>
    JsonObject Stages()
    {
        double Mean(double total, long count) => count == 0 ? 0 : Math.Round(total / count, 3);
        var result = new JsonObject
        {
            ["frames"] = stagedFrames,
            ["steps"] = simulatedSteps,
            ["simulateMs"] = Mean(simulateMs, simulatedSteps),
            ["drawMs"] = Mean(drawMs, stagedFrames),
            ["readBackMs"] = Mean(readBackMs, stagedFrames),
            ["coverageMs"] = Mean(coverageMs, stagedFrames),
            ["encodeMs"] = Mean(encodeMs, stagedFrames),
            ["catchUpSteps"] = catchUpSteps,
            ["scheduleResets"] = scheduleResets,
        };
        simulateMs = drawMs = readBackMs = coverageMs = encodeMs = 0;
        stagedFrames = simulatedSteps = catchUpSteps = scheduleResets = 0;
        return result;
    }

    // ----- Loading -----

    void Load(JsonObject message, JsonObject reply)
    {
        var path = (Text(message, "effect") ?? throw new CommandException("The command needs \"effect\".")).Replace('\\', '/').Trim();
        if (!path.EndsWith(".vpcf_c", StringComparison.OrdinalIgnoreCase))
        {
            path = path.EndsWith(".vpcf", StringComparison.OrdinalIgnoreCase) ? path + "_c" : path;
        }

        var overlayPath = Text(message, "overlay");
        var radius = message["radius"] != null ? Number(message, "radius") : EffectRules.DefaultRadius;
        if (!float.IsFinite(radius) || radius <= 0f || radius > 10000f)
        {
            throw new CommandException("The radius must be above 0 and at most 10000.");
        }

        var rules = Flag(message, "rules", true);
        compare = Flag(message, "compare", compare);
        loop = Flag(message, "loop", loop);
        if (message["speed"] != null)
        {
            var s = Number(message, "speed");
            if (!float.IsFinite(s) || s <= 0f || s > 8f)
            {
                throw new CommandException("The speed must be above 0 and at most 8.");
            }

            speed = s;
        }

        paused = Flag(message, "paused", false);
        var keepView = Flag(message, "keepView", false);

        // Open the new overlay first: if it cannot be read, nothing changes.
        var overlay = OpenOverlay(overlayPath);

        var timer = Stopwatch.StartNew();
        Unload();

        var mark = logger.Mark;
        var next = new Loaded { Path = path, Radius = radius, Rules = rules, OverlayPath = overlay?.FileName, Overlay = overlay, Floor = Flag(message, "floor", true) };
        loader!.SwapOverlay(overlay)?.Dispose();
        loader.ResetResolved();

        try
        {
            next.YoursResource = loader.LoadFile(path);
            if (next.YoursResource?.DataBlock is not ParticleSystem)
            {
                next.YoursResource?.Dispose();
                next.YoursResource = null;
                throw new CommandException(loader.FindFile(path, logNotFound: false).PackageEntry == null && loader.FindFile(path, logNotFound: false).PathOnDisk == null
                    ? $"The effect {path} was not found in the game files or the overlay."
                    : $"{path} is not a particle effect.");
            }

            next.Plan = EffectRules.Analyse(loader, path, radius, rulesOn: rules);
        }
        catch (Exception)
        {
            next.YoursResource?.Dispose();
            loader.SwapOverlay(null);
            overlay?.Dispose();
            throw;
        }

        double loadMs;
        double fitMs;
        try
        {
            effect = next;
            Layout();
            Allocate();
            BuildNodes(buildGame: compare);
            loadMs = timer.Elapsed.TotalMilliseconds;

            if (keepView)
            {
                ApplyCamera();
                Preroll(collectBounds: true);
            }
            else
            {
                Fit(resetAngles: true);
            }

            fitMs = timer.Elapsed.TotalMilliseconds - loadMs;
            RestartAll();
            finished = false;
            generation++;

            effect.Notes = Notes(mark);
        }
        catch (Exception)
        {
            // A failed load leaves nothing loaded, never half an effect.
            Unload();
            generation++;
            throw;
        }

        reply["loaded"] = LoadedStatus();
        reply["timing"] = new JsonObject { ["loadMs"] = Math.Round(loadMs, 1), ["fitMs"] = Math.Round(fitMs, 1) };
    }

    static Package? OpenOverlay(string? path)
    {
        if (string.IsNullOrEmpty(path))
        {
            return null;
        }

        if (!File.Exists(path))
        {
            throw new CommandException($"The overlay {path} was not found.");
        }

        // The whole file is read into memory, so the caller may overwrite or delete it as soon as the
        // reply arrives. A VPK whose files live in separate numbered chunks is read from disk instead
        // and stays open until it is replaced.
        var package = new Package();
        try
        {
            var bytes = File.ReadAllBytes(path);
            package.SetFileName(path);
            package.Read(new MemoryStream(bytes, writable: false));
            var chunked = package.Entries?.Values.Any(list => list.Any(entry => entry.ArchiveIndex != 0x7FFF)) == true;
            if (chunked)
            {
                package.Dispose();
                package = new Package();
                package.Read(path);
            }
        }
        catch (Exception e)
        {
            package.Dispose();
            throw new CommandException($"The overlay {path} could not be read as a VPK ({e.Message}).");
        }

        return package;
    }

    void ReplaceOverlay(string? overlayPath, JsonObject reply)
    {
        var current = effect!;
        var overlay = OpenOverlay(overlayPath);
        var timer = Stopwatch.StartNew();
        var mark = logger.Mark;

        // The new version is built next to the old one, which goes only once the new one stands;
        // if anything fails the old version, its package and its file list stay as they were. The
        // game's version is not touched.
        var primary = host!.Primary;
        var previous = loader!.SwapOverlay(overlay);
        var resolvedBefore = loader.Resolved.ToArray();
        var appliedBefore = current.Applied;
        loader.Resolved.Clear();
        ValveResourceFormat.Resource? resource = null;
        ParticleSceneNode? node = null;
        try
        {
            resource = loader.LoadFile(current.Path);
            if (resource?.DataBlock is not ParticleSystem system)
            {
                throw new CommandException($"The effect {current.Path} could not be read with the new overlay.");
            }

            node = BuildNode(primary, system);
        }
        catch (Exception)
        {
            if (node != null)
            {
                RemoveNode(primary, node);
            }

            resource?.Dispose();
            loader.SwapOverlay(previous);
            overlay?.Dispose();
            loader.Resolved.Clear();
            foreach (var (file, source) in resolvedBefore)
            {
                loader.Resolved[file] = source;
            }

            current.Applied = appliedBefore;
            throw;
        }

        RemoveNode(primary, current.Yours);
        current.YoursResource?.Dispose();
        previous?.Dispose();
        current.Yours = node;
        current.YoursResource = resource;
        current.Overlay = overlay;
        current.OverlayPath = overlay?.FileName;
        if (compare && current.Game != null)
        {
            // A new game version too, so both are equally new and play the same draws.
            RebuildGame();
        }

        RestartAll();
        finished = false;
        generation++;

        var notes = Notes(mark);
        // Control point findings need a pre-roll; the tree's structure does not change with colours.
        if (current.Notes?["undrivenControlPoints"] is JsonNode undriven)
        {
            notes["undrivenControlPoints"] = undriven.DeepClone();
        }

        // The new version has not drawn a model yet; what its material variables do was found on the
        // previous one, whose models and materials an edit of colours does not change.
        var seenNow = (notes["modelMaterialVars"]?["parts"] as JsonArray)?.Any(p => p?["seen"]?.GetValue<bool>() == true) == true;
        if (!seenNow && current.Notes?["modelMaterialVars"] is JsonNode previousVars && current.Notes["untintedModels"] is JsonNode previousUntinted)
        {
            notes["modelMaterialVars"] = previousVars.DeepClone();
            notes["untintedModels"] = previousUntinted.DeepClone();
        }

        current.Notes = notes;
        reply["loaded"] = LoadedStatus();
        reply["timing"] = new JsonObject { ["replaceMs"] = Math.Round(timer.Elapsed.TotalMilliseconds, 1) };
    }

    void SetCompare(bool on)
    {
        if (on == compare && (!on || effect?.Game != null))
        {
            return;
        }

        compare = on;
        Layout();
        Allocate();
        if (effect != null)
        {
            if (on)
            {
                // Both versions start as new systems, so they have run the same steps.
                RebuildYours();
                RebuildGame();
            }

            Reframe();
            RestartAll();
            finished = false;
            generation++;
        }
    }

    /// <summary>
    /// After the frame's shape changed (compare, size): fits again while the view is the automatic
    /// one, so a half-width picture does not cut what a full-width one showed; a view the user has
    /// turned or zoomed is kept.
    /// </summary>
    void Reframe()
    {
        if (autoView && effect != null)
        {
            Fit(resetAngles: false);
        }
    }

    /// <summary>
    /// Builds the user's version again from its loaded file. A system keeps some state over its
    /// restarts, so two versions only play the same draws when both are equally new.
    /// </summary>
    void RebuildYours()
    {
        if (effect?.YoursResource?.DataBlock is not ParticleSystem system)
        {
            return;
        }

        var primary = host!.Primary;
        var node = BuildNode(primary, system);
        RemoveNode(primary, effect.Yours);
        effect.Yours = node;
    }

    /// <summary>Builds the game's version again (or for the first time), with the overlay hidden.</summary>
    void RebuildGame()
    {
        if (effect == null)
        {
            return;
        }

        if (gameView != null)
        {
            RemoveNode(gameView, effect.Game);
        }

        effect.Game = null;
        effect.GameResource?.Dispose();
        effect.GameResource = null;
        BuildGame();
    }

    void Unload()
    {
        if (effect == null)
        {
            return;
        }

        RemoveNode(host!.Primary, effect.Yours);
        if (gameView != null)
        {
            RemoveNode(gameView, effect.Game);
        }

        effect.YoursResource?.Dispose();
        effect.GameResource?.Dispose();
        loader!.SwapOverlay(null);
        effect.Overlay?.Dispose();
        effect = null;
    }

    void RemoveNode(RenderView view, ParticleSceneNode? node)
    {
        if (node == null)
        {
            return;
        }

        if (materialVars.Remove(node, out var vars))
        {
            vars.Dispose();
        }

        var scene = view.Renderer.Scene;
        if (node.PreviewModel is { } model)
        {
            scene.Remove(model, true);
            model.Delete();
        }

        scene.Remove(node, true);
        node.Delete();
    }

    void BuildNodes(bool buildGame)
    {
        var system = (ParticleSystem)effect!.YoursResource!.DataBlock!;
        effect.Yours = BuildNode(host!.Primary, system);
        if (buildGame)
        {
            BuildGame();
        }
    }

    void BuildGame()
    {
        gameView ??= host!.CreateView(Math.Max(MinSize / 2, width / 2), height, sun: true);
        Layout();
        gameView.Renderer.Postprocess.ExposureCompensation = exposure;
        gameView.Scene.ClearColor = host!.Scene.ClearColor;
        ApplyCamera();

        using (loader!.GameOnly())
        {
            effect!.GameResource = loader.LoadFile(effect.Path);
            if (effect.GameResource?.DataBlock is not ParticleSystem system)
            {
                effect.GameResource?.Dispose();
                effect.GameResource = null;
                return;
            }

            effect.Game = BuildNode(gameView, system);
        }
    }

    ParticleSceneNode BuildNode(RenderView view, ParticleSystem system)
    {
        // Simulated on this thread: the steps are a fraction of a millisecond, and the two versions
        // of an effect then run exactly the same sequence of operations.
        view.Renderer.ParallelSimulation = false;
        var node = new ParticleSceneNode(view.Renderer.Scene, system, null, preview: true)
        {
            Transform = Matrix4x4.Identity,
            // The helper restarts the versions together, so neither loops by itself.
            Loop = false,
        };

        view.Renderer.Scene.Add(node, true);
        if (node.PreviewModel != null)
        {
            // It still animates and carries its attachments; only its drawing is off.
            node.PreviewModel.Visible = false;
        }

        // Rule 4: the library's ground plane (the particle viewer's "Collide With Floor"), and for
        // projected decals an invisible floor that writes depth for them to land on.
        view.Renderer.EntitySystem.PhysicsWorld.GroundPlane = effect!.Plan.Ground != null || effect.Plan.DecalRenderers > 0 ? 0f : null;
        SetFloor(view, effect.Plan.DecalRenderers > 0 && effect.Floor);

        var applied = EffectRules.ApplyControlPoints(effect.Plan, node);
        if (ReferenceEquals(view, host!.Primary))
        {
            effect.Applied = applied;
        }

        effect.Seeded = PinSeed(node, effect.Path);
        materialVars[node] = ModelMaterialVars.Attach(node, host.Context);
        node.Restart();
        return node;
    }

    readonly Dictionary<RenderView, DepthFloor> floors = [];

    void SetFloor(RenderView view, bool on)
    {
        if (on && !floors.ContainsKey(view))
        {
            // It sorts as the farthest node, so it draws first in the translucent pass, where it
            // takes itself out of the depth the sprites test against (see DepthFloor).
            var floor = new DepthFloor(view.Renderer.Scene, 0f, 20000f);
            view.Renderer.Scene.Add(floor, true);
            floors[view] = floor;
        }
        else if (!on && floors.Remove(view, out var floor))
        {
            view.Renderer.Scene.Remove(floor, true);
        }

        // Decals read the depth the renderer resolves after the opaque pass.
        view.Renderer.ForceResolveSceneDepth = on;
    }

    /// <summary>
    /// Fixes the seed every replay starts from (the library's own <c>PinRandomSeed</c>, which is not
    /// public; see <see cref="LibraryAccess"/>), the same for both versions of an effect: each restart
    /// then plays out the same, and the game's version and the edited one draw the same random
    /// numbers. Also puts the control points that functions wrote back to how they were set at each replay.
    /// </summary>
    static bool PinSeed(ParticleSceneNode node, string path)
    {
        var simulation = Program.SimulationOf(node);
        var seed = unchecked((int)ValveResourceFormat.Utils.StringToken.Get(path.ToLowerInvariant()));
        return simulation != null && LibraryAccess.PinRandomSeed(simulation, seed);
    }

    // ----- Playback -----

    void RestartAll()
    {
        if (effect == null)
        {
            return;
        }

        playTime = 0f;
        foreach (var node in Nodes())
        {
            node.Restart();
            EffectRules.ApplyControlPoints(effect.Plan, node);
        }

        PlaceMoving();
    }

    IEnumerable<ParticleSceneNode> Nodes()
    {
        if (effect?.Yours != null)
        {
            yield return effect.Yours;
        }

        if (compare && effect?.Game != null)
        {
            yield return effect.Game;
        }
    }

    /// <summary>
    /// Rule 5: carries control point 0 (and a preview model holding it) along +X. The camera
    /// follows it, so the moving thing stays in place in the picture and its trail streams behind.
    /// </summary>
    void PlaceMoving()
    {
        if (effect?.Plan.Moving is not { } moving)
        {
            if (follow != Vector3.Zero)
            {
                follow = Vector3.Zero;
                ApplyCamera();
            }

            return;
        }

        var offset = new Vector3(-moving.Speed * moving.Period * 0.5f + moving.Speed * Math.Min(playTime, moving.Period), 0, 0);
        follow = offset;
        ApplyCamera();
        var transform = Matrix4x4.CreateTranslation(offset);
        foreach (var node in Nodes())
        {
            node.Transform = transform;
            if (node.PreviewModel != null)
            {
                node.PreviewModel.Transform = transform;
            }
        }
    }

    void Step(float dt)
    {
        if (effect == null)
        {
            return;
        }

        playTime += dt;
        if (effect.Plan.Moving is { } moving && playTime >= moving.Period)
        {
            if (loop)
            {
                RestartAll();
            }
        }

        PlaceMoving();

        var started = Stopwatch.GetTimestamp();
        UpdateViews(dt);
        simulateMs += Stopwatch.GetElapsedTime(started).TotalMilliseconds;
        simulatedSteps++;

        var allFinished = Nodes().All(n => n.IsFinished);
        if (allFinished)
        {
            if (loop)
            {
                RestartAll();
            }
            else
            {
                finished = true;
                writer.Status(new JsonObject { ["type"] = "finished", ["generation"] = generation });
            }
        }
    }

    // ----- Drawing -----

    void Layout()
    {
        var primary = host!.Primary;
        if (compare)
        {
            var left = width / 2;
            gameView?.Resize(left, height);
            primary.Resize(width - left, height);
        }
        else
        {
            primary.Resize(width, height);
        }

        ApplyCamera();
    }

    void Allocate()
    {
        if (pixels.Length != width * height * 4)
        {
            pixels = new byte[width * height * 4];
            scratch = new byte[width * height * 4];
        }
    }

    void RenderFrame()
    {
        if (effect == null || writer.IsBroken)
        {
            return;
        }

        var primary = host!.Primary;
        var x0 = 0;
        var t0 = Stopwatch.GetTimestamp();
        double read = 0;
        if (compare && gameView != null && effect.Game != null)
        {
            gameView.Draw();
            var r0 = Stopwatch.GetTimestamp();
            gameView.ReadBack(pixels, width, 0, scratch);
            read += Stopwatch.GetElapsedTime(r0).TotalMilliseconds;
            x0 = gameView.Width;
        }
        else if (compare)
        {
            // The game has no file of that name (the effect exists only in the overlay): the left half stays empty.
            Clear(pixels, width, 0, width / 2, height, achievedBackground);
            x0 = width / 2;
        }

        primary.Draw();
        var r1 = Stopwatch.GetTimestamp();
        primary.ReadBack(pixels, width, x0, scratch);
        read += Stopwatch.GetElapsedTime(r1).TotalMilliseconds;
        drawMs += Stopwatch.GetElapsedTime(t0).TotalMilliseconds - read;
        readBackMs += read;

        var c0 = Stopwatch.GetTimestamp();
        var (coverage, edges) = Coverage(pixels, width, x0, primary.Width, height, achievedBackground);
        var e0 = Stopwatch.GetTimestamp();
        coverageMs += Stopwatch.GetElapsedTime(c0, e0).TotalMilliseconds;
        var jpeg = Program.Encode(pixels, width, height, SKEncodedImageFormat.Jpeg, quality);
        encodeMs += Stopwatch.GetElapsedTime(e0).TotalMilliseconds;
        stagedFrames++;
        var flags = (compare ? Protocol.FrameFlags.Compare : 0) | (paused ? Protocol.FrameFlags.Paused : 0) | (finished ? Protocol.FrameFlags.Finished : 0);
        var wallMs = (uint)Math.Max(0, clock.Elapsed.TotalMilliseconds - readyAt);
        writer.Frame(Protocol.Frame(sequence++, generation, playTime, wallMs, width, height, (float)coverage, flags, edges, jpeg));
    }

    static void Clear(byte[] bgra, int stride, int x0, int w, int h, (byte R, byte G, byte B) color)
    {
        for (var y = 0; y < h; y++)
        {
            for (var x = x0; x < x0 + w; x++)
            {
                var i = (y * stride + x) * 4;
                bgra[i] = color.B;
                bgra[i + 1] = color.G;
                bgra[i + 2] = color.R;
                bgra[i + 3] = 255;
            }
        }
    }

    /// <summary>The share of pixels in a region that differ from the background, and which edges they touch.</summary>
    public static (double Coverage, Protocol.Edges Edges) Coverage(byte[] bgra, int stride, int x0, int w, int h, (byte R, byte G, byte B) background, int threshold = 12)
    {
        var changed = 0;
        var edges = Protocol.Edges.None;
        for (var y = 0; y < h; y++)
        {
            var row = (y * stride + x0) * 4;
            for (var x = 0; x < w; x++)
            {
                var i = row + x * 4;
                if (Math.Abs(bgra[i] - background.B) > threshold || Math.Abs(bgra[i + 1] - background.G) > threshold || Math.Abs(bgra[i + 2] - background.R) > threshold)
                {
                    changed++;
                    if (x == 0) edges |= Protocol.Edges.Left;
                    if (x == w - 1) edges |= Protocol.Edges.Right;
                    if (y == 0) edges |= Protocol.Edges.Top;
                    if (y == h - 1) edges |= Protocol.Edges.Bottom;
                }
            }
        }

        return ((double)changed / (w * h), edges);
    }

    // ----- Background and brightness -----

    void Calibrate()
    {
        var primary = host!.Primary;
        primary.Renderer.Postprocess.ExposureCompensation = exposure;
        if (gameView != null)
        {
            gameView.Renderer.Postprocess.ExposureCompensation = exposure;
        }

        // The centre pixel is read back: hide what the view holds while searching.
        var hidden = Nodes().Where(n => n.Visible).ToList();
        foreach (var node in hidden)
        {
            node.Visible = false;
        }

        try
        {
            var key = ((uint)(background.R << 16 | background.G << 8 | background.B), exposure);
            if (!calibrations.TryGetValue(key, out var linear))
            {
                linear = SearchClearColor(primary, background);
                calibrations[key] = linear;
            }

            var clear = new OpenTK.Mathematics.Color4(linear.X, linear.Y, linear.Z, 1f);
            primary.Scene.ClearColor = clear;
            if (gameView != null)
            {
                gameView.Scene.ClearColor = clear;
            }

            primary.Update(0f);
            primary.Draw();
            achievedBackground = primary.ReadPixel(primary.Width / 2, primary.Height / 2);
        }
        finally
        {
            foreach (var node in hidden)
            {
                node.Visible = true;
            }
        }

        dirty = true;
    }

    /// <summary>
    /// The clear colour passes through exposure, tone mapping and gamma like the effect does. Each
    /// channel is searched (in log space) for the linear value whose output byte is the requested one.
    /// </summary>
    static Vector3 SearchClearColor(RenderView view, (byte R, byte G, byte B) target)
    {
        (byte, byte, byte) RenderWith(Vector3 linear)
        {
            view.Scene.ClearColor = new OpenTK.Mathematics.Color4(linear.X, linear.Y, linear.Z, 1f);
            view.Update(0f);
            view.Draw();
            return view.ReadPixel(view.Width / 2, view.Height / 2);
        }

        var lo = new Vector3(-14f);
        var hi = new Vector3(6f);
        var want = new Vector3(target.R, target.G, target.B);

        for (var i = 0; i < 22; i++)
        {
            var mid = (lo + hi) * 0.5f;
            var (r, g, b) = RenderWith(new Vector3(MathF.Pow(2, mid.X), MathF.Pow(2, mid.Y), MathF.Pow(2, mid.Z)));
            var got = new Vector3(r, g, b);
            lo = new Vector3(got.X < want.X ? mid.X : lo.X, got.Y < want.Y ? mid.Y : lo.Y, got.Z < want.Z ? mid.Z : lo.Z);
            hi = new Vector3(got.X < want.X ? hi.X : mid.X, got.Y < want.Y ? hi.Y : mid.Y, got.Z < want.Z ? hi.Z : mid.Z);
        }

        return new Vector3(
            target.R == 0 ? 0 : MathF.Pow(2, hi.X),
            target.G == 0 ? 0 : MathF.Pow(2, hi.Y),
            target.B == 0 ? 0 : MathF.Pow(2, hi.Z));
    }

    // ----- Camera -----

    void ApplyCamera()
    {
        foreach (var view in new[] { host!.Primary, gameView })
        {
            if (view == null)
            {
                continue;
            }

            var camera = view.Renderer.Camera;
            camera.Yaw = yaw * MathF.PI / 180f;
            camera.Pitch = pitch * MathF.PI / 180f;
            camera.Roll = 0f;
            camera.RecalculateDirectionVectors();
            camera.SetLocation(target + follow - camera.Forward * distance);
            camera.NearPlane = Math.Clamp(distance * 0.01f, 0.05f, 4f);
            camera.SetViewportSize(view.Width, view.Height);
            camera.CreateProjectionMatrix();
            camera.RecalculateMatrices();
        }

        dirty = true;
    }

    JsonObject CameraJson() => new()
    {
        ["target"] = Program.Vec(target),
        ["distance"] = Math.Round(distance, 2),
        ["yaw"] = Math.Round(yaw, 2),
        ["pitch"] = Math.Round(pitch, 2),
    };

    /// <summary>
    /// Plays the effect from the start for <see cref="PrerollSeconds"/> and collects, per step, where
    /// its particles are (position and radius) and which control points move away from the origin.
    /// </summary>
    List<List<Vector4>> Preroll(bool collectBounds)
    {
        var frames = new List<List<Vector4>>();
        RestartAll();
        finished = false;
        var dt = 1f / 30f;
        var steps = (int)MathF.Ceiling(PrerollSeconds * 30f);
        var bounds = Bounds.Empty;
        var reads = effect!.Plan.ControlPointReads.Keys.Where(cp => cp > 0).ToArray();
        var moved = new SortedSet<int>();

        for (var i = 0; i < steps; i++)
        {
            StepQuiet(dt);
            var points = new List<Vector4>();
            var models = new List<Vector4>();
            foreach (var node in Nodes())
            {
                Framing.CollectPoints(Program.SimulationOf(node), points);
                Framing.CollectModelBounds(node, models);
            }

            var particleCount = points.Count;
            points.AddRange(models);

            // A moving effect is framed in the frame that travels with it.
            if (follow != Vector3.Zero)
            {
                for (var k = 0; k < points.Count; k++)
                {
                    points[k] -= new Vector4(follow, 0f);
                }
            }

            // The reported bounds stay those of the particles; the model corners only frame.
            foreach (var p in points.Take(particleCount))
            {
                var r = p.W;
                bounds = bounds.Include(new Vector3(p.X, p.Y, p.Z) - new Vector3(r), new Vector3(p.X, p.Y, p.Z) + new Vector3(r));
            }

            if (effect.Yours != null)
            {
                foreach (var cp in reads)
                {
                    if (effect.Yours.GetControlPoint(cp).Position != Vector3.Zero)
                    {
                        moved.Add(cp);
                    }
                }
            }

            frames.Add(points);
        }

        if (collectBounds)
        {
            effect.Bounds = bounds;
            effect.MovedControlPoints = moved;
        }

        return frames;
    }

    /// <summary>A step for pre-rolls: no status messages, loops when looping is off too.</summary>
    void StepQuiet(float dt)
    {
        playTime += dt;
        if (effect!.Plan.Moving is { } moving && playTime >= moving.Period)
        {
            RestartAll();
        }

        PlaceMoving();
        UpdateViews(dt);

        if (Nodes().All(n => n.IsFinished))
        {
            RestartAll();
        }
    }

    /// <summary>
    /// Frames the effect: a box fit from the particles' positions and radii over a pre-roll (so long
    /// beams and paths are framed by their end points, not by pixels that may already be off the
    /// frame), then a check of the drawn pixels that backs off while the picture touches an edge and
    /// moves in when it fills little of the frame.
    /// </summary>
    void Fit(bool resetAngles)
    {
        if (resetAngles)
        {
            yaw = DefaultYaw;
            pitch = DefaultPitch;
        }

        var frames = Preroll(collectBounds: true);
        var primary = host!.Primary;
        var aspect = (float)primary.Width / primary.Height;
        var fov = primary.Renderer.Camera.GetFOV();

        var fill = 0.8f;
        var screenSpace = effect!.Yours?.HasInfiniteBounds == true;
        if (screenSpace || frames.All(f => f.Count == 0))
        {
            target = Vector3.Zero;
            distance = 346f;
        }
        else
        {
            (target, distance) = Framing.FitPoints(frames, yaw, pitch, fov, aspect, fill);
        }

        ApplyCamera();

        // Pixel check over further play-throughs: a search between the nearest distance known to
        // touch an edge and the farthest known to fit. Drawn size does not shrink in proportion
        // to distance (near parts grow faster), so a single proportional step can overshoot.
        float? touching = null;
        (float Distance, Vector3 Target)? fits = null;
        var lastTouched = false;
        for (var pass = 0; pass < 6 && !screenSpace; pass++)
        {
            var box = PixelPass();
            if (box.IsEmpty)
            {
                // Sprites with m_flStartFadeSize fade out once they cover that share of the screen:
                // when nothing showed, frame the effect smaller, below the smallest such share.
                if (fill == 0.8f && effect.Plan.SpriteFadeStart is { } fade && fade * 0.8f < fill && frames.Any(f => f.Count > 0))
                {
                    fill = Math.Clamp(fade * 0.8f, 0.08f, 0.8f);
                    (target, distance) = Framing.FitPoints(frames, yaw, pitch, fov, aspect, fill);
                    ApplyCamera();
                    continue;
                }

                break;
            }

            var w = primary.Width;
            var h = primary.Height;
            var touches = box.X0 <= 1 || box.Y0 <= 1 || box.X1 >= w - 2 || box.Y1 >= h - 2;
            var filled = MathF.Max((box.X1 - box.X0 + 1) / (float)w, (box.Y1 - box.Y0 + 1) / (float)h);
            if (options.Verbose)
            {
                Console.Error.WriteLine(FormattableString.Invariant($"[Debug] fit pass {pass}: box {box.X0},{box.Y0}-{box.X1},{box.Y1} of {w}x{h}, touches {touches}, filled {filled:0.00}, distance {distance:0.#}"));
            }

            lastTouched = touches;
            if (touches)
            {
                // What is drawn reaches past the frame (a model, or a sprite larger than its radius).
                touching = distance;
                if (fits is { } known)
                {
                    target = known.Target;
                    distance = MathF.Sqrt(distance * known.Distance);
                }
                else
                {
                    distance *= distance < 100f ? 2.5f : 1.6f;
                }

                ApplyCamera();
                continue;
            }

            // Fits: centre on what was drawn, then move in if it fills little of the frame.
            var centred = target;
            var proposed = distance;
            Framing.FitToPixels(ref centred, ref proposed, primary.Renderer.Camera, box, w, h, fill);
            fits = (distance, Framing.Recentre(target, distance, primary.Renderer.Camera, box, w, h));
            if (filled >= fill * 0.7f)
            {
                target = fits.Value.Target;
                ApplyCamera();
                break;
            }

            target = centred;
            distance = touching is { } near ? MathF.Max(proposed, MathF.Sqrt(near * distance)) : proposed;
            ApplyCamera();
        }

        if (lastTouched && fits is { } safe)
        {
            (distance, target) = safe;
            ApplyCamera();
        }

        autoView = true;
        RestartAll();
    }

    PixelBox PixelPass()
    {
        RestartAll();
        var primary = host!.Primary;
        var dt = 1f / 30f;
        var steps = (int)MathF.Ceiling(PrerollSeconds * 30f);
        var box = PixelBox.Empty;
        var buffer = new byte[primary.Width * primary.Height * 4];
        for (var i = 0; i < steps; i++)
        {
            StepQuiet(dt);
            if (i % 3 == 0)
            {
                primary.Draw();
                primary.ReadBack(buffer, primary.Width, 0, scratch.Length >= buffer.Length ? scratch : new byte[buffer.Length]);
                box = box.Add(buffer, primary.Width, primary.Height, achievedBackground);
                if (compare && gameView != null && effect!.Game != null)
                {
                    gameView.Draw();
                    var gameBuffer = new byte[gameView.Width * gameView.Height * 4];
                    gameView.ReadBack(gameBuffer, gameView.Width, 0, new byte[gameBuffer.Length]);
                    box = box.Add(gameBuffer, gameView.Width, gameView.Height, achievedBackground);
                }
            }
        }

        return box;
    }

    // ----- Status -----

    JsonObject Notes(long mark)
    {
        var current = effect!;
        var plan = current.Plan;
        var notes = new JsonObject();

        // Model renderers with material variables: what the player applied, and what it could not.
        var untintedCount = plan.UntintedModelRenderers;
        IEnumerable<string> untintedParts = plan.UntintedModels;
        if (current.Yours != null && materialVars.TryGetValue(current.Yours, out var vars))
        {
            var (count, parts, report) = vars.Summary();
            untintedCount = count + Math.Max(0, plan.UntintedModelRenderers - vars.Renderers);
            untintedParts = vars.Renderers < plan.UntintedModelRenderers ? parts.Union(plan.UntintedModels, StringComparer.OrdinalIgnoreCase) : parts;
            notes["modelMaterialVars"] = report;
        }

        notes["untintedModels"] = new JsonObject
        {
            ["renderers"] = untintedCount,
            ["parts"] = EffectRules.ToArray(untintedParts),
            ["note"] = untintedCount == 0 ? null : "Model particles in these parts are drawn without some of the values their material variables give them in the game.",
        };

        notes["decals"] = new JsonObject
        {
            ["renderers"] = plan.DecalRenderers,
            ["parts"] = EffectRules.ToArray(plan.Decals),
            ["surface"] = plan.DecalRenderers == 0 ? null : current.Floor ? "floor" : "none",
            ["note"] = plan.DecalRenderers == 0 ? null : current.Floor
                ? "These parts project decals onto surfaces; the preview gives them a flat, invisible floor at the effect's base, so on walls or slopes in the game they look different."
                : "These parts project decals onto surfaces; with the floor switched off they have nothing to land on and do not show.",
        };

        var undriven = new JsonArray();
        foreach (var (cp, reads) in plan.ControlPointReads)
        {
            if (cp == 0)
            {
                continue;
            }

            var filled = current.Applied.Where(a => a.Contains($"control point {cp} ", StringComparison.Ordinal)).ToList();
            if ((current.MovedControlPoints.Contains(cp) || plan.WrittenControlPoints.Contains(cp)) && filled.Count == 0)
            {
                continue;
            }

            undriven.Add(new JsonObject
            {
                ["cp"] = cp,
                ["readBy"] = EffectRules.ToArray(reads.Take(8)),
                ["reads"] = reads.Count,
                ["filled"] = filled.Count == 0 ? null : EffectRules.ToArray(filled),
            });
        }

        notes["undrivenControlPoints"] = undriven;

        var skipped = new SortedSet<string>(StringComparer.Ordinal);
        foreach (var node in Nodes())
        {
            foreach (var name in Program.SkippedRenderers(node))
            {
                skipped.Add(name?.ToString() ?? "?");
            }
        }

        var renderers = new JsonObject();
        foreach (var (name, count) in plan.Renderers)
        {
            renderers[name] = count;
        }

        notes["renderers"] = renderers;
        var screenSpace = current.Yours?.HasInfiniteBounds == true;
        notes["screenSpace"] = screenSpace;

        // Plain sentences for what the preview cannot show of this effect.
        var cannot = new JsonArray();
        if (plan.Renderers.Count == 0)
        {
            cannot.Add("This effect draws nothing itself; it only starts or steers other effects in the game.");
        }
        else if (plan.Renderers.Keys.All(EffectRules.LightRenderers.Contains))
        {
            cannot.Add("This effect only gives off light, and the preview has no surfaces for it to light.");
        }
        else if (!current.Floor && plan.Renderers.Keys.All(k => k == "C_OP_RenderProjected" || EffectRules.LightRenderers.Contains(k)))
        {
            cannot.Add("This effect is drawn as decals projected onto surfaces, and with the floor switched off the preview has none for them to land on.");
        }

        if (plan.NeedsModel != null && current.Yours?.PreviewModel == null)
        {
            cannot.Add($"Parts of this effect are placed on a character model ({plan.NeedsModel}), and the preview has no model to place them on.");
        }

        if (screenSpace)
        {
            cannot.Add("This is a screen effect: in the game it covers the player's view, so it fills the preview frame.");
        }

        notes["cannotShow"] = cannot;
        notes["skippedRenderers"] = EffectRules.ToArray(skipped);
        notes["unsupportedFunctions"] = EffectRules.ToArray(plan.Unsupported.Distinct());

        var warnings = new JsonArray();
        foreach (var (level, text) in logger.Since(mark))
        {
            if (level >= LogLevel.Warning)
            {
                warnings.Add($"{level}: {text}");
            }
        }

        notes["warnings"] = warnings;
        return notes;
    }

    JsonObject LoadedStatus()
    {
        var current = effect!;
        var overlayFiles = new JsonArray();
        var missing = new JsonArray();
        var gameFiles = 0;
        foreach (var (file, source) in loader!.Resolved.OrderBy(p => p.Key, StringComparer.Ordinal))
        {
            if (source.EndsWith("(overlay)", StringComparison.Ordinal))
            {
                overlayFiles.Add(file);
            }
            else if (source == "NOT FOUND")
            {
                missing.Add(file);
            }
            else
            {
                gameFiles++;
            }
        }

        var plan = current.Plan;
        var rules = new JsonArray();
        foreach (var applied in current.Applied)
        {
            rules.Add(applied);
        }

        if (plan.Ground != null)
        {
            rules.Add($"ground: a ground plane at z = 0 for {plan.Ground}");
        }

        if (plan.Moving is { } moving)
        {
            rules.Add(FormattableString.Invariant($"moving: control point 0 moves along +X at {moving.Speed:0} units per second, starting over every {moving.Period:0.##} s, for {moving.Reason}"));
        }

        return new JsonObject
        {
            ["effect"] = current.Path,
            ["overlay"] = current.OverlayPath,
            ["compare"] = compare,
            ["bounds"] = current.Bounds.ToJson(),
            ["camera"] = CameraJson(),
            ["files"] = new JsonObject
            {
                ["fromOverlay"] = overlayFiles,
                ["fromGame"] = gameFiles,
                ["missing"] = missing,
            },
            ["rules"] = rules,
            ["notes"] = current.Notes?.DeepClone(),
            ["seeded"] = current.Seeded,
            ["compareNote"] = current.Seeded ? CompareNote : CompareNoteUnseeded,
            ["gameVersion"] = compare ? (current.Game != null ? "drawn on the left" : "not in the game files; the left half is empty") : null,
        };
    }

    const string CompareNote = "The game's version (left) and yours (right) share the camera and the playback clock and restart together from the same random seed, so they match particle for particle; only an edit that changes how many particles a part makes can make them drift apart.";

    const string CompareNoteUnseeded = "The game's version (left) and yours (right) share the camera and the playback clock and restart together, but their random numbers are not tied: they play in step in time, not particle for particle.";

    public static (byte R, byte G, byte B) ParseColor(string text)
    {
        var hex = text.Trim().TrimStart('#');
        if (hex.Length != 6 || !int.TryParse(hex, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var value))
        {
            throw new CommandException("A colour is six hexadecimal digits, such as 0b0c10.");
        }

        return ((byte)(value >> 16), (byte)(value >> 8), (byte)value);
    }

    static string Hex((byte R, byte G, byte B) c) => $"{c.R:x2}{c.G:x2}{c.B:x2}";

    public void Dispose()
    {
        try
        {
            Unload();
            gameView?.Dispose();
            host?.Dispose();
        }
        catch (Exception)
        {
            // Teardown failures do not matter at exit.
        }

        loader?.Dispose();
        wake.Dispose();
    }
}

/// <summary>A waitable timer with sub-millisecond resolution, waited on together with a wake-up event.</summary>
sealed class HighResolutionTimer : IDisposable
{
    const uint CreateWaitableTimerHighResolution = 0x2;
    const uint TimerAllAccess = 0x1F0003;

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr CreateWaitableTimerExW(IntPtr attributes, string? name, uint flags, uint access);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool SetWaitableTimer(SafeWaitHandle timer, ref long dueTime, int period, IntPtr completion, IntPtr argument, [MarshalAs(UnmanagedType.Bool)] bool resume);

    sealed class TimerWaitHandle : WaitHandle
    {
        public TimerWaitHandle(IntPtr handle) => SafeWaitHandle = new SafeWaitHandle(handle, ownsHandle: true);
    }

    readonly TimerWaitHandle? handle;

    public HighResolutionTimer()
    {
        var raw = CreateWaitableTimerExW(IntPtr.Zero, null, CreateWaitableTimerHighResolution, TimerAllAccess);
        if (raw != IntPtr.Zero)
        {
            handle = new TimerWaitHandle(raw);
        }
    }

    /// <summary>Waits <paramref name="seconds"/> or until <paramref name="wake"/> is set.</summary>
    public void Wait(double seconds, WaitHandle wake)
    {
        if (handle == null)
        {
            wake.WaitOne(TimeSpan.FromSeconds(seconds));
            return;
        }

        var due = -(long)(seconds * 1e7);
        if (due >= 0)
        {
            return;
        }

        SetWaitableTimer(handle.SafeWaitHandle, ref due, 0, IntPtr.Zero, IntPtr.Zero, false);
        WaitHandle.WaitAny([wake, handle]);
    }

    public void Dispose() => handle?.Dispose();
}
