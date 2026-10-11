import { useEffect, useRef, useState } from "react";
import { ExtraIcon } from "./Icons";
import { duration } from "./models";
import { previewError } from "./media";
let context: AudioContext | null = null;
let playEpoch = 0;
function audioContext() {
  return (context ??= new AudioContext());
}
const stopEvent = "dmb-stop-extra-audio";

export function AudioPlayer({
  bytes,
  label,
  volume = 100,
  disabled = false,
}: {
  bytes: ArrayBuffer;
  label: string;
  volume?: number;
  disabled?: boolean;
}) {
  const [buffer, setBuffer] = useState<AudioBuffer | null>(null),
    [peaks, setPeaks] = useState<number[]>([]),
    [position, setPosition] = useState(0),
    [playing, setPlaying] = useState(false),
    [error, setError] = useState("");
  const source = useRef<AudioBufferSourceNode | null>(null),
    gain = useRef<GainNode | null>(null),
    clip = useRef<WaveShaperNode | null>(null);
  const offset = useRef(0),
    start = useRef(0),
    active = useRef(false),
    raf = useRef(0),
    volumeRef = useRef(volume);
  volumeRef.current = volume;
  const generation = useRef(0),
    disabledRef = useRef(disabled);
  disabledRef.current = disabled;
  function stop(reset = false) {
    if (active.current && context)
      offset.current += context.currentTime - start.current;
    active.current = false;
    cancelAnimationFrame(raf.current);
    if (source.current) {
      source.current.onended = null;
      source.current.stop();
      source.current.disconnect();
      source.current = null;
    }
    gain.current?.disconnect();
    clip.current?.disconnect();
    gain.current = null;
    clip.current = null;
    if (reset) offset.current = 0;
    setPlaying(false);
    setPosition(offset.current);
  }
  const stopRef = useRef(stop);
  stopRef.current = stop;
  useEffect(() => {
    let disposed = false;
    generation.current++;
    setBuffer(null);
    setError("");
    setPeaks([]);
    offset.current = 0;
    setPosition(0);
    try {
      void audioContext()
        .decodeAudioData(bytes.slice(0))
        .then((decoded) => {
          if (disposed) return;
          setBuffer(decoded);
          const bins = 84,
            values = new Array<number>(bins).fill(0);
          for (let c = 0; c < decoded.numberOfChannels; c++) {
            const samples = decoded.getChannelData(c);
            for (let i = 0; i < bins; i++) {
              const from = Math.floor((i * samples.length) / bins),
                to = Math.floor(((i + 1) * samples.length) / bins);
              for (let n = from; n < to; n++)
                values[i] = Math.max(values[i], Math.abs(samples[n]));
            }
          }
          setPeaks(values);
        })
        .catch((e) => {
          if (!disposed) setError(previewError(e));
        });
    } catch (e) {
      setError(previewError(e));
    }
    return () => {
      disposed = true;
      generation.current++;
      stopRef.current(true);
    };
  }, [bytes]);
  useEffect(() => {
    const onStop = () => stopRef.current();
    window.addEventListener(stopEvent, onStop);
    return () => window.removeEventListener(stopEvent, onStop);
  }, []);
  useEffect(() => {
    if (gain.current && context)
      gain.current.gain.setTargetAtTime(
        volume / 100,
        context.currentTime,
        0.008,
      );
  }, [volume]);
  useEffect(() => {
    if (disabled) stopRef.current();
  }, [disabled]);
  async function play() {
    if (!buffer || disabled) return;
    if (active.current) {
      stop();
      return;
    }
    const epoch = ++playEpoch,
      version = generation.current;
    window.dispatchEvent(new Event(stopEvent));
    try {
      const ctx = audioContext();
      await ctx.resume();
      if (
        epoch !== playEpoch ||
        version !== generation.current ||
        disabledRef.current
      )
        return;
      if (offset.current >= buffer.duration) offset.current = 0;
      const node = ctx.createBufferSource(),
        level = ctx.createGain(),
        limiter = ctx.createWaveShaper();
      node.buffer = buffer;
      level.gain.value = volumeRef.current / 100;
      limiter.curve = new Float32Array([-1, 1]);
      node.connect(level).connect(limiter).connect(ctx.destination);
      source.current = node;
      gain.current = level;
      clip.current = limiter;
      start.current = ctx.currentTime;
      active.current = true;
      setPlaying(true);
      node.onended = () => {
        active.current = false;
        offset.current = buffer.duration;
        setPosition(buffer.duration);
        setPlaying(false);
        cancelAnimationFrame(raf.current);
        node.disconnect();
        level.disconnect();
        limiter.disconnect();
        source.current = null;
      };
      node.start(0, offset.current);
      const frame = () => {
        if (active.current) {
          setPosition(
            Math.min(
              buffer.duration,
              offset.current + ctx.currentTime - start.current,
            ),
          );
          raf.current = requestAnimationFrame(frame);
        }
      };
      frame();
    } catch (e) {
      stop();
      setError(previewError(e));
    }
  }
  function seek(seconds: number) {
    const resume = active.current;
    stop();
    offset.current = seconds;
    setPosition(seconds);
    if (resume) void play();
  }
  const progress = buffer ? position / buffer.duration : 0;
  return (
    <div className="extra-player">
      <div className="extra-waveform" aria-hidden="true">
        <svg viewBox="0 0 420 78" preserveAspectRatio="none">
          {peaks.map((peak, i) => {
            const h = Math.max(2, Math.min(1, (peak * volume) / 100) * 68);
            return (
              <rect
                key={i}
                x={i * 5 + 1}
                y={(78 - h) / 2}
                width="3"
                height={h}
                rx="1.5"
                className={i / peaks.length <= progress ? "played" : ""}
              />
            );
          })}
        </svg>
        {!buffer && !error && <span>Preparing waveform…</span>}
      </div>
      <input
        className="extra-seek"
        aria-label={`Seek ${label}`}
        type="range"
        min="0"
        max={buffer?.duration ?? 1}
        step="0.01"
        value={Math.min(position, buffer?.duration ?? 1)}
        disabled={disabled || !buffer}
        onChange={(e) => seek(Number(e.target.value))}
      />
      <div className="extra-player-controls">
        <button
          className="extra-play"
          aria-label={`${playing ? "Pause" : "Play"} ${label}`}
          disabled={disabled || !buffer}
          onClick={() => void play()}
        >
          <ExtraIcon name={playing ? "pause" : "play"} size={18} />
        </button>
        <span>
          {playing
            ? "Playing"
            : position > 0 && buffer && position < buffer.duration
              ? "Paused"
              : "Listen"}
        </span>
        <time>
          {duration(position)}{" "}
          <span>/ {buffer ? duration(buffer.duration) : "—"}</span>
        </time>
      </div>
      {error && (
        <p className="extra-error" role="status">
          {error}
        </p>
      )}
      {peaks.some((peak) => (peak * volume) / 100 > 1) && (
        <p className="extra-processing extra-caution">
          Peaks will clip at this volume. Lower it if you hear distortion.
        </p>
      )}
    </div>
  );
}
