using System.Numerics;
using OpenTK.Graphics.OpenGL;
using ValveResourceFormat.Renderer;
using ValveResourceFormat.Renderer.Materials;
using ValveResourceFormat.Renderer.SceneNodes;
using ValveResourceFormat.Renderer.World;
using ValveResourceFormat.ResourceTypes;
using ValveResourceFormat.Utils;

namespace Dmb.EffectPlayer;

/// <summary>
/// An invisible floor for projected decals: a square at height <c>z</c> that lands in the scene
/// depth the decals read and in nothing the sprites are tested against.
/// <list type="number">
/// <item>Opaque pass: it writes depth and no colour. Custom nodes draw after the meshes there, so
/// an opaque model particle below the floor has already drawn its colour.</item>
/// <item>The renderer then copies the scene depth (<c>g_tSceneDepth</c>), which the decals project
/// onto, before it draws them.</item>
/// <item>Translucent pass, first of all (it sorts as the farthest node):
/// where the floor is still the nearest surface (the copied depth equals its own) it puts the
/// cleared depth back, so sprites, trails and translucent models that cross z = 0 are not cut by
/// it. Sprites that feather against the scene depth still fade into it softly, as they would on
/// the ground.</item>
/// </list>
/// The background stays flat throughout.
/// </summary>
sealed class DepthFloor : SceneNode
{
    static int program;
    static int viewProjectionLocation;
    static int modeLocation;
    static int clearDepthLocation;
    static int vertexArray;
    static int vertexBuffer;

    readonly float z;
    readonly float half;

    public DepthFloor(Scene scene, float z, float half)
        : base(scene)
    {
        this.z = z;
        this.half = half;
        LocalBoundingBox = new AABB(new Vector3(-half, -half, z - 1f), new Vector3(half, half, z + 1f));
        RenderPasses = CustomRenderPasses.Opaque | CustomRenderPasses.Translucent;
        Name = "dmb_depth_floor";
    }

    /// <summary>Sorted as the farthest translucent node, so its translucent pass runs before any other.</summary>
    public override float GetCameraDistance(Camera camera) => 1e9f;

    static void EnsureProgram()
    {
        if (program != 0)
        {
            return;
        }

        const string vertex = """
            #version 460 core
            layout(location = 0) in vec3 position;
            uniform mat4 viewProjection;
            void main() { gl_Position = viewProjection * vec4(position, 1.0); }
            """;
        // Mode 0 writes the floor's depth. Mode 1 runs after the scene depth was copied: where the
        // copy holds the floor's own depth (the floor is the nearest surface there), it writes the
        // cleared depth back; anywhere else (something nearer was drawn) it leaves the depth alone.
        var fragment = $$"""
            #version 460 core
            layout(location = 0) out vec4 color;
            layout(binding = {{(int)ReservedTextureSlots.SceneDepth}}) uniform sampler2D g_tSceneDepth;
            uniform int mode;
            uniform float clearDepth;
            void main()
            {
                color = vec4(0.0);
                // The shader writes gl_FragDepth, so every path must.
                gl_FragDepth = gl_FragCoord.z;
                if (mode == 0)
                {
                    return;
                }

                float scene = texelFetch(g_tSceneDepth, ivec2(gl_FragCoord.xy), 0).r;
                float tolerance = max(2.0 * fwidth(gl_FragCoord.z), 2e-6 * max(abs(gl_FragCoord.z), 1e-3));
                if (abs(scene - gl_FragCoord.z) > tolerance)
                {
                    discard;
                }

                gl_FragDepth = clearDepth;
            }
            """;

        int Compile(ShaderType type, string source)
        {
            var shader = GL.CreateShader(type);
            GL.ShaderSource(shader, source);
            GL.CompileShader(shader);
            GL.GetShader(shader, ShaderParameter.CompileStatus, out var ok);
            if (ok == 0)
            {
                throw new InvalidOperationException($"The floor shader did not compile: {GL.GetShaderInfoLog(shader)}");
            }

            return shader;
        }

        var vs = Compile(ShaderType.VertexShader, vertex);
        var fs = Compile(ShaderType.FragmentShader, fragment);
        program = GL.CreateProgram();
        GL.AttachShader(program, vs);
        GL.AttachShader(program, fs);
        GL.LinkProgram(program);
        GL.DeleteShader(vs);
        GL.DeleteShader(fs);
        GL.GetProgram(program, GetProgramParameterName.LinkStatus, out var linked);
        if (linked == 0)
        {
            var log = GL.GetProgramInfoLog(program);
            GL.DeleteProgram(program);
            program = 0;
            throw new InvalidOperationException($"The floor shader did not link: {log}");
        }
        viewProjectionLocation = GL.GetUniformLocation(program, "viewProjection");
        modeLocation = GL.GetUniformLocation(program, "mode");
        clearDepthLocation = GL.GetUniformLocation(program, "clearDepth");

        vertexArray = GL.GenVertexArray();
        vertexBuffer = GL.GenBuffer();
    }

    public override void Update(Scene.UpdateContext context)
    {
    }

    public override void Render(Scene.RenderContext context)
    {
        var lift = context.RenderPass == RenderPass.Translucent;
        if (context.ReplacementShader != null || !(lift || context.RenderPass is RenderPass.Opaque or RenderPass.DepthOnly))
        {
            return;
        }

        // The renderer keeps track of what it bound; put everything back as it was.
        var previousProgram = GL.GetInteger(GetPName.CurrentProgram);
        var previousArray = GL.GetInteger(GetPName.VertexArrayBinding);
        var previousBuffer = GL.GetInteger(GetPName.ArrayBufferBinding);
        GL.GetBoolean(GetPName.DepthWritemask, out bool previousDepthWrite);
        var previousDepthFunc = GL.GetInteger(GetPName.DepthFunc);
        var depthTest = GL.IsEnabled(EnableCap.DepthTest);
        var blend = GL.IsEnabled(EnableCap.Blend);
        var cull = GL.IsEnabled(EnableCap.CullFace);

        EnsureProgram();

        float[] vertices =
        [
            -half, -half, z,
            half, -half, z,
            -half, half, z,
            half, half, z,
        ];

        GL.BindVertexArray(vertexArray);
        GL.BindBuffer(BufferTarget.ArrayBuffer, vertexBuffer);
        GL.BufferData(BufferTarget.ArrayBuffer, vertices.Length * sizeof(float), vertices, BufferUsageHint.DynamicDraw);
        GL.EnableVertexAttribArray(0);
        GL.VertexAttribPointer(0, 3, VertexAttribPointerType.Float, false, 3 * sizeof(float), 0);

        GL.UseProgram(program);
        var viewProjection = context.Camera.ViewProjectionMatrix;
        unsafe
        {
            GL.UniformMatrix4(viewProjectionLocation, 1, false, &viewProjection.M11);
        }

        GL.Uniform1(modeLocation, lift ? 1 : 0);
        GL.GetFloat(GetPName.DepthClearValue, out float clearDepth);
        GL.Uniform1(clearDepthLocation, clearDepth);

        GL.ColorMask(false, false, false, false);
        GL.Disable(EnableCap.CullFace);
        GL.Disable(EnableCap.Blend);
        GL.Enable(EnableCap.DepthTest);
        if (lift)
        {
            GL.DepthFunc(DepthFunction.Always);
        }

        GL.DepthMask(true);
        GL.DrawArrays(PrimitiveType.TriangleStrip, 0, 4);
        GL.ColorMask(true, true, true, true);
        GL.DepthMask(previousDepthWrite);
        GL.DepthFunc((DepthFunction)previousDepthFunc);
        if (!depthTest)
        {
            GL.Disable(EnableCap.DepthTest);
        }

        if (blend)
        {
            GL.Enable(EnableCap.Blend);
        }

        if (cull)
        {
            GL.Enable(EnableCap.CullFace);
        }

        GL.BindVertexArray(previousArray);
        GL.BindBuffer(BufferTarget.ArrayBuffer, previousBuffer);
        GL.UseProgram(previousProgram);
    }
}
