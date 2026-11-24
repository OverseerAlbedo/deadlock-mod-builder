using System.Numerics;
using ValveResourceFormat.Particles;
using ValveResourceFormat.Renderer;

namespace Dmb.EffectPlayer;

/// <summary>Camera framing from particle positions and from drawn pixels.</summary>
static class Framing
{
    /// <summary>Adds every live particle of a system and its children as (position, radius); screen-space systems are skipped.</summary>
    public static void CollectPoints(ParticleSystemSimulation? simulation, List<Vector4> points)
    {
        if (simulation == null || simulation.IsScreenSpaceEffect)
        {
            return;
        }

        var collection = simulation.Particles;
        if (collection.Count > 0)
        {
            var span = collection.Current;
            for (var i = 0; i < collection.Count; i++)
            {
                var p = span.ReadVector(ParticleField.Position, i);
                var radius = span.ReadFloat(ParticleField.Radius, i);
                if (!float.IsFinite(p.X) || !float.IsFinite(p.Y) || !float.IsFinite(p.Z) || p.Length() > 1e6f)
                {
                    continue;
                }

                var r = float.IsFinite(radius) ? Math.Clamp(radius, 0f, 1e4f) : 0f;
                points.Add(new Vector4(p, r));
            }
        }

        foreach (var child in simulation.Children)
        {
            CollectPoints(child, points);
        }
    }

    /// <summary>
    /// Adds the corners of the bounds of every animated model a model renderer of the node's tree
    /// draws now (as points of radius 0), so such model particles are framed by their size, not only
    /// by the particles' positions and radii. Nothing is added when the library's members cannot be read.
    /// </summary>
    public static void CollectModelBounds(ValveResourceFormat.Renderer.SceneNodes.ParticleSceneNode node, List<Vector4> points)
    {
        foreach (var system in LibraryAccess.ModelRenderers(node))
        {
            if (system.Simulation.IsScreenSpaceEffect || system.Simulation.Particles.Count == 0)
            {
                continue;
            }

            foreach (var renderer in system.ModelRenderers)
            {
                foreach (var model in LibraryAccess.NodesByParticle(renderer, system.Simulation.Particles))
                {
                    // Only models posed by a sequence: their parts move within the model's bounds over
                    // the sequence, further than the pre-roll's pixels may show. A model that is not
                    // posed shows its whole shape in the pixels, and its bounds are often much larger
                    // than what it draws.
                    if (model is not { Visible: true } || model.AnimationController.ActiveAnimation == null)
                    {
                        continue;
                    }

                    var box = model.BoundingBox;
                    var size = box.Max - box.Min;
                    if (!float.IsFinite(size.X) || !float.IsFinite(size.Y) || !float.IsFinite(size.Z) || size.Length() > 1e5f || size.Length() <= 0f)
                    {
                        continue;
                    }

                    for (var c = 0; c < 8; c++)
                    {
                        points.Add(new Vector4(
                            (c & 1) == 0 ? box.Min.X : box.Max.X,
                            (c & 2) == 0 ? box.Min.Y : box.Max.Y,
                            (c & 4) == 0 ? box.Min.Z : box.Max.Z,
                            0f));
                    }
                }
            }
        }
    }

    /// <summary>
    /// The camera target and distance that keep the particles of each pre-roll step inside
    /// <paramref name="fill"/> of the frame from the given angles. The target is the middle of the
    /// particles' extent (2nd to 98th percentile along each camera axis, so a stray particle does not
    /// move it); the distance is the 90th percentile over the steps of what each step needs, so a few
    /// steps with stray particles do not push the camera away.
    /// </summary>
    public static (Vector3 Target, float Distance) FitPoints(List<List<Vector4>> frames, float yawDegrees, float pitchDegrees, float fovVertical, float aspect, float fill)
    {
        var (forward, up, right) = Camera.GetDirectionVectors(pitchDegrees * MathF.PI / 180f, yawDegrees * MathF.PI / 180f, 0f);

        var xs = new List<float>();
        var ys = new List<float>();
        var zs = new List<float>();
        foreach (var frame in frames)
        {
            foreach (var p in frame)
            {
                var v = new Vector3(p.X, p.Y, p.Z);
                xs.Add(Vector3.Dot(v, right));
                ys.Add(Vector3.Dot(v, up));
                zs.Add(Vector3.Dot(v, forward));
            }
        }

        if (xs.Count == 0)
        {
            return (Vector3.Zero, 346f);
        }

        static float Middle(List<float> values)
        {
            values.Sort();
            var lo = values[(int)(values.Count * 0.02)];
            var hi = values[Math.Min(values.Count - 1, (int)(values.Count * 0.98))];
            return (lo + hi) * 0.5f;
        }

        var target = right * Middle(xs) + up * Middle(ys) + forward * Middle(zs);

        var tanV = MathF.Tan(fovVertical * 0.5f);
        var tanH = tanV * aspect;
        var needs = new List<float>();
        foreach (var frame in frames)
        {
            if (frame.Count == 0)
            {
                continue;
            }

            var need = 0f;
            foreach (var p in frame)
            {
                var d = new Vector3(p.X, p.Y, p.Z) - target;
                var x = MathF.Abs(Vector3.Dot(d, right)) + p.W;
                var y = MathF.Abs(Vector3.Dot(d, up)) + p.W;
                var z = Vector3.Dot(d, forward);
                need = MathF.Max(need, MathF.Max(x / (tanH * fill), y / (tanV * fill)) - z);
            }

            needs.Add(need);
        }

        needs.Sort();
        var distance = needs.Count == 0 ? 346f : needs[Math.Min(needs.Count - 1, (int)(needs.Count * 0.9))];
        return (target, Math.Clamp(distance, 20f, 100000f));
    }

    /// <summary>The target moved so the drawn pixels are centred, at the same distance.</summary>
    public static Vector3 Recentre(Vector3 target, float distance, Camera camera, PixelBox box, int width, int height)
    {
        var d = distance;
        var t = target;
        FitToPixels(ref t, ref d, camera, box, width, height, 1f);
        return t;
    }

    /// <summary>
    /// Moves and zooms so the pixels the effect covered fill <paramref name="fill"/> of the frame,
    /// centred. The shift is made in the plane through the camera target, so it is exact for content
    /// at the target's depth and close for the rest.
    /// </summary>
    public static void FitToPixels(ref Vector3 target, ref float distance, Camera camera, PixelBox box, int width, int height, float fill)
    {
        if (box.IsEmpty)
        {
            return;
        }

        var tanV = MathF.Tan(camera.GetFOV() * 0.5f);
        var tanH = tanV * width / height;

        var x0 = box.X0 / (float)width * 2f - 1f;
        var x1 = (box.X1 + 1) / (float)width * 2f - 1f;
        var y0 = 1f - (box.Y1 + 1) / (float)height * 2f;
        var y1 = 1f - box.Y0 / (float)height * 2f;
        var cx = (x0 + x1) * 0.5f;
        var cy = (y0 + y1) * 0.5f;
        var half = MathF.Max((x1 - x0) * 0.5f, (y1 - y0) * 0.5f);

        target += camera.Right * (cx * tanH * distance) + camera.Up * (cy * tanV * distance);
        distance *= Math.Clamp(half / fill, 0.05f, 4f);
    }
}
