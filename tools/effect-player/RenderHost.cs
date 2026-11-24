using System.Numerics;
using Microsoft.Extensions.Logging;
using OpenTK.Graphics.OpenGL;
using OpenTK.Mathematics;
using OpenTK.Windowing.Common;
using OpenTK.Windowing.Desktop;
using ValveResourceFormat.CompiledShader;
using ValveResourceFormat.Renderer;
using ValveResourceFormat.Renderer.Materials;
using Vector3 = System.Numerics.Vector3;
using Vector4 = System.Numerics.Vector4;

namespace Dmb.EffectPlayer;

/// <summary>Raised when no OpenGL context of the required version can be made.</summary>
sealed class OpenGLUnavailableException(string message, Exception? inner = null) : Exception(message, inner);

/// <summary>Presents the hidden window's context to the renderer.</summary>
sealed class WindowSurface(IGLFWGraphicsContext context) : IGraphicsSurface
{
    public void Begin() => context.MakeCurrent();

    public void End() => context.MakeNoneCurrent();
}

/// <summary>
/// One renderer with its own scene, camera and post-processing, drawing into a multisampled HDR
/// scene target and tone mapping into an 8-bit output target that frames are read back from. No
/// grid, no sky and no background material are drawn: the scene target is cleared to a flat colour.
/// Several views can share one <see cref="RenderHost"/> (one context, one set of material and
/// shader caches); each has its own scene, so effects in different views never light, cull or
/// sort against each other.
/// </summary>
sealed class RenderView : IDisposable
{
    public Renderer Renderer { get; }
    public Framebuffer Scene { get; }
    public Framebuffer Output { get; }
    public int Width { get; private set; }
    public int Height { get; private set; }
    readonly int msaa;
    readonly TextRenderer? text;

    public RenderView(RendererContext context, int width, int height, int msaa, TextRenderer? sharedText, bool sun, bool initializeScene = true)
    {
        Width = width;
        Height = height;
        this.msaa = msaa;

        Renderer = new Renderer(context);
        if (sharedText == null)
        {
            text = new TextRenderer(context, Renderer.Camera);
            text.Load();
        }
        else
        {
            text = sharedText;
        }

        Renderer.Postprocess.Load(msaa);

        Scene = Framebuffer.Prepare("EffectScene", width, height, msaa, ImageFormat.RGBA16161616F, ImageFormat.D32);
        Scene.Initialize();

        Output = Framebuffer.Prepare("EffectOutput", width, height, 0, ImageFormat.RGBA8888, null);
        Output.Initialize();

        Renderer.Initialize();
        Renderer.MainFramebuffer = Scene;
        Renderer.LoadRendererResources();

        // The default background material is a gradient; a flat clear colour is drawn instead.
        Renderer.Skybox2D = null;

        Renderer.Camera.SetViewportSize(width, height);

        if (initializeScene)
        {
            if (sun)
            {
                LoadSunLighting();
            }

            Renderer.Scene.Initialize();
        }
    }

    public TextRenderer Text => text!;

    /// <summary>The default sun of viewers without lighting information, without an environment map.</summary>
    public void LoadSunLighting()
    {
        var scene = Renderer.Scene;
        var angles = Renderer.DefaultSunAngles;
        var sunForward = ValveResourceFormat.ResourceTypes.EntityTransformHelper.EulerAnglesToForwardDirection(new Vector3(angles.X, angles.Y, 0));
        scene.LightingInfo.LightingData.SunDirection = new Vector4(-sunForward, scene.LightingInfo.GetSunMinRoughness());
        var color = Renderer.DefaultSunColor;
        scene.LightingInfo.LightingData.SunColor = new Vector4(new Vector3(color.X, color.Y, color.Z) * color.W, 1f);
        scene.LightingInfo.UseSceneBoundsForSunLightFrustum = false;
    }

    public void Resize(int width, int height)
    {
        if (width == Width && height == Height)
        {
            return;
        }

        Width = width;
        Height = height;
        Scene.Resize(width, height, msaa);
        Output.Resize(width, height);
        Renderer.Camera.SetViewportSize(width, height);
    }

    public void Update(float timestep)
    {
        Renderer.Update(new ValveResourceFormat.Renderer.Scene.UpdateContext
        {
            Camera = Renderer.Camera,
            TextRenderer = Text,
            Timestep = timestep,
        });
    }

    /// <summary>Renders the scene and tone maps it into the 8-bit output target.</summary>
    public void Draw()
    {
        Renderer.Render(Scene);
        Renderer.PostprocessRender(Scene, Output, flipY: false);
    }

    /// <summary>
    /// Reads the output target as top-down BGRA rows into <paramref name="destination"/>, a buffer
    /// <paramref name="destinationWidth"/> pixels wide, starting at column <paramref name="x0"/>.
    /// </summary>
    public void ReadBack(byte[] destination, int destinationWidth, int x0, byte[] scratch)
    {
        Output.Bind(FramebufferTarget.ReadFramebuffer);
        GL.PixelStore(PixelStoreParameter.PackAlignment, 1);
        GL.ReadPixels(0, 0, Width, Height, PixelFormat.Bgra, PixelType.UnsignedByte, scratch);

        // OpenGL rows start at the bottom
        var stride = Width * 4;
        var destinationStride = destinationWidth * 4;
        for (var y = 0; y < Height; y++)
        {
            System.Buffer.BlockCopy(scratch, (Height - 1 - y) * stride, destination, y * destinationStride + x0 * 4, stride);
        }
    }

    /// <summary>Reads one output pixel (x, y from the top-left) as RGB.</summary>
    public (byte R, byte G, byte B) ReadPixel(int x, int y)
    {
        var pixel = new byte[4];
        Output.Bind(FramebufferTarget.ReadFramebuffer);
        GL.PixelStore(PixelStoreParameter.PackAlignment, 1);
        GL.ReadPixels(x, Height - 1 - y, 1, 1, PixelFormat.Bgra, PixelType.UnsignedByte, pixel);
        return (pixel[2], pixel[1], pixel[0]);
    }

    public void Dispose()
    {
        Renderer.Dispose();
    }
}

/// <summary>
/// A hidden window with an OpenGL core context, the library's renderer context, and a primary
/// <see cref="RenderView"/>. Further views share the context.
/// </summary>
sealed class RenderHost : IDisposable
{
    public NativeWindow Window { get; }
    public RendererContext Context { get; }
    public RenderView Primary { get; }
    public Renderer Renderer => Primary.Renderer;
    public TextRenderer Text => Primary.Text;
    public Framebuffer Scene => Primary.Scene;
    public Framebuffer Output => Primary.Output;
    public int Width => Primary.Width;
    public int Height => Primary.Height;
    public int Msaa { get; }
    public string GLVendor { get; }
    public string GLRenderer { get; }
    public string GLVersion { get; }

    /// <param name="initializePrimary">Whether the primary view's scene is lit and initialised here;
    /// when false the caller does it after adding its own lighting.</param>
    public RenderHost(EffectLoader loader, ILogger logger, int width, int height, Version glVersion, int msaa, bool initializePrimary = false)
    {
        Msaa = msaa;

        var settings = new NativeWindowSettings
        {
            APIVersion = glVersion,
            Profile = ContextProfile.Core,
            Flags = ContextFlags.ForwardCompatible | ContextFlags.Offscreen,
            WindowBorder = WindowBorder.Hidden,
            StartVisible = false,
            StartFocused = false,
            ClientSize = new Vector2i(64, 64),
            Vsync = VSyncMode.Off,
            Title = "effect-player",
        };

        try
        {
            Window = new NativeWindow(settings);
        }
        catch (OpenTK.Windowing.GraphicsLibraryFramework.GLFWException e)
        {
            // OpenTK appends advice about its error callback to the driver's message; keep the driver's part.
            var reason = e.Message.Trim();
            var advice = reason.IndexOf(" (this is thrown from", StringComparison.Ordinal);
            if (advice > 0)
            {
                reason = reason[..advice];
            }

            throw new OpenGLUnavailableException($"OpenGL {glVersion.Major}.{glVersion.Minor} is not available on this computer ({reason.TrimEnd('.')}).", e);
        }

        Window.Context.MakeCurrent();

        GLVendor = GL.GetString(StringName.Vendor) ?? "";
        GLRenderer = GL.GetString(StringName.Renderer) ?? "";
        GLVersion = GL.GetString(StringName.Version) ?? "";

        Context = new RendererContext(loader, logger);
        Context.Device.CreateContext(new WindowSurface(Window.Context)).Begin();

        try
        {
            GLEnvironment.Initialize(logger);
        }
        catch (NotSupportedException e)
        {
            throw new OpenGLUnavailableException(e.Message, e);
        }

        GLEnvironment.SetDefaultRenderState();
        Context.TextureStreaming.Mode = TextureStreamingMode.Immediate;

        Primary = new RenderView(Context, width, height, msaa, null, sun: true, initializeScene: initializePrimary);
        GL.Viewport(0, 0, width, height);
    }

    /// <summary>A further view on the same context, sharing its caches and text renderer.</summary>
    public RenderView CreateView(int width, int height, bool sun) => new(Context, width, height, Msaa, Primary.Text, sun);

    /// <summary>The default sun of viewers without lighting information, for the primary view.</summary>
    public void LoadSunLighting() => Primary.LoadSunLighting();

    public void Update(float timestep) => Primary.Update(timestep);

    public void Draw() => Primary.Draw();

    public void ReadBack(byte[] destination)
    {
        var scratch = new byte[destination.Length];
        Primary.ReadBack(destination, Primary.Width, 0, scratch);
    }

    public (byte R, byte G, byte B) ReadPixel(int x, int y) => Primary.ReadPixel(x, y);

    public void Dispose()
    {
        Window.MakeCurrent();
        Context.Dispose();
        Primary.Dispose();
        Window.Dispose();
    }
}
