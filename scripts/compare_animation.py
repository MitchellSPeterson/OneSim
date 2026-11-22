#!/usr/bin/env python3
"""Multi-frame animation capture, t0 auto-alignment, filmstrip diff, and synced side-by-side video tool.

Implements Layers 2 & 3 of OneSim's animation testing harness (issue #127):
  1. `record-onesim`  Drive `onesim serve` with deterministic `tick` + `capture_screenshot` at 60fps.
  2. `extract-native` Extract 60fps frames from an iOS Simulator / Android Emulator recording (`agent-device record`)
                      and auto-align t0 as the first frame whose pixel diff vs. rest crosses a threshold.
  3. `compare`        Compare OneSim frames against native frames at sampled timestamps (e.g. 50,120,220,350,600ms)
                      with a +/-N frame matching window, producing a tabular report and a filmstrip PNG.
  4. `video`          Stitch t0-aligned native vs. OneSim frames side-by-side with a `t = XXX ms` overlay
                      (and optional diff heatmap panel) into an H.264 MP4 via ffmpeg.
"""
import argparse
import base64
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_TIMESTAMPS = [50, 120, 220, 350, 600]
DIFF_LUMA_THRESHOLD = 24  # same ~10% luminance threshold as scripts/compare_ios.py


def pixel_diff_stats(img_a: Image.Image, img_b: Image.Image):
    """Return `(pct_over_threshold, mean_abs_diff, diff_luma_image)` matching scripts/compare_ios.py."""
    a = img_a.convert("RGB")
    b = img_b.convert("RGB")
    if b.size != a.size:
        b = b.resize(a.size)
    diff = ImageChops.difference(a, b).convert("L")
    hist = diff.histogram()
    total = a.size[0] * a.size[1]
    pct = sum(hist[DIFF_LUMA_THRESHOLD:]) / total * 100.0
    mean = sum(i * n for i, n in enumerate(hist)) / total
    return pct, mean, diff


def diff_heatmap(diff_luma: Image.Image) -> Image.Image:
    return Image.merge("RGB", (diff_luma.point(lambda v: min(255, v * 3)), diff_luma.point(lambda _: 0), diff_luma.point(lambda _: 0)))


def find_t0_index(frame_paths, motion_threshold_pct=0.15, rest_path=None):
    """Return the index of the first frame whose pixel diff vs. resting frame exceeds `motion_threshold_pct`.

    Falls back to 0 if no frame crosses the threshold.
    """
    if not frame_paths:
        return 0
    rest = Image.open(rest_path or frame_paths[0]).convert("RGB")
    start = 0 if rest_path is not None else 1
    for i in range(start, len(frame_paths)):
        pct, _, _ = pixel_diff_stats(rest, Image.open(frame_paths[i]))
        if pct >= motion_threshold_pct:
            return i
    return 0


def load_frame_sequence(path: Path, fps: float = 60.0, motion_threshold: float = 0.15, tmp_dir: Path | None = None):
    """Load a frame directory (with or without manifest.json) or extract an .mp4/.mov video via ffmpeg.

    Returns `(frames, t0_idx, fps)` where `frames` is a list of `{"path": Path, "t_ms": float}`.
    """
    if path.is_file():
        if tmp_dir is None:
            raise ValueError("tmp_dir required when loading a video file")
        extract_dir = tmp_dir / path.stem
        extract_native(path, extract_dir, fps=fps, motion_threshold=motion_threshold)
        path = extract_dir

    manifest_path = path / "manifest.json"
    if manifest_path.exists():
        m = json.loads(manifest_path.read_text())
        seq_fps = float(m.get("fps", fps))
        t0_idx = int(m.get("t0_index", 0))
        frames = [{"path": path / f["file"], "t_ms": float(f["t_ms"])} for f in m["frames"]]
        return frames, t0_idx, seq_fps

    pngs = sorted(path.glob("*.png"))
    if not pngs:
        raise FileNotFoundError(f"no PNG frames found in {path}")
    t0_idx = find_t0_index(pngs, motion_threshold_pct=motion_threshold)
    dt = 1000.0 / fps
    frames = [{"path": p, "t_ms": (i - t0_idx) * dt} for i, p in enumerate(pngs)]
    return frames, t0_idx, fps


def frame_at_time(frames, t_ms: float) -> int:
    """Index of the frame whose `t_ms` is closest to `t_ms`."""
    return min(range(len(frames)), key=lambda i: abs(frames[i]["t_ms"] - t_ms))


def match_frame_in_window(onesim_img: Image.Image, native_frames, target_t_ms: float, window: int = 2):
    """Match `onesim_img` against native frames within `[center - window, center + window]`.

    Returns `(best_idx, delta_frames, best_pct, best_mean, best_native_img, best_diff_luma)`.
    """
    center_idx = frame_at_time(native_frames, target_t_ms)
    lo = max(0, center_idx - window)
    hi = min(len(native_frames) - 1, center_idx + window)
    best = None
    for idx in range(lo, hi + 1):
        nat_img = Image.open(native_frames[idx]["path"]).convert("RGB")
        pct, mean, diff = pixel_diff_stats(nat_img, onesim_img)
        if best is None or pct < best[2]:
            best = (idx, idx - center_idx, pct, mean, nat_img, diff)
    assert best is not None
    return best


def extract_native(video_path: Path, out_dir: Path, fps: float = 60.0, motion_threshold: float = 0.15):
    """Extract frames from a native simulator recording via ffmpeg and auto-align t0."""
    if not shutil.which("ffmpeg"):
        raise RuntimeError("ffmpeg is required to extract native recordings (install via Homebrew: brew install ffmpeg)")
    out_dir.mkdir(parents=True, exist_ok=True)
    for old in out_dir.glob("frame_*.png"):
        old.unlink()
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-i", str(video_path), "-vf", f"fps={fps}", str(out_dir / "frame_%04d.png")],
        check=True,
    )
    pngs = sorted(out_dir.glob("frame_*.png"))
    if not pngs:
        raise RuntimeError(f"ffmpeg extracted 0 frames from {video_path}")
    t0_idx = find_t0_index(pngs, motion_threshold_pct=motion_threshold)
    dt = 1000.0 / fps
    manifest = {
        "source": str(video_path),
        "fps": fps,
        "t0_index": t0_idx,
        "motion_threshold_pct": motion_threshold,
        "frames": [{"index": i, "file": p.name, "t_ms": round((i - t0_idx) * dt, 3)} for i, p in enumerate(pngs)],
    }
    (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=2))
    return manifest


def parse_action(spec: str):
    """Parse an action string: `click:LABEL`, `click:X,Y`, `drag:X,Y,DX,DY`, `wait:MS`."""
    kind, _, arg = spec.partition(":")
    if not arg:
        raise ValueError(f"expected KIND:ARG (e.g. click:Beta), got {spec!r}")
    return kind.strip().lower(), arg.strip()


def run_action(cli_call, sid: str, kind: str, arg: str):
    def resolve(target: str):
        parts = [p.strip() for p in target.split(",")]
        if len(parts) == 2:
            try:
                return float(parts[0]), float(parts[1])
            except ValueError:
                pass
        root = cli_call("inspect_ui_tree", {"session_id": sid})["root"]
        stack = [root]
        while stack:
            n = stack.pop()
            if n.get("label") == target or n.get("text") == target:
                f = n["frame"]
                return f["x"] + f["width"] / 2.0, f["y"] + f["height"] / 2.0
            stack.extend(reversed(n.get("children", [])))
        raise RuntimeError(f"target {target!r} not found in UI tree")

    if kind == "click":
        x, y = resolve(arg)
        cli_call("send_pointer_event", {"session_id": sid, "event": {"kind": "click", "x": x, "y": y}})
    elif kind == "drag":
        parts = [float(p.strip()) for p in arg.split(",")]
        if len(parts) != 4:
            raise ValueError(f"drag wants X,Y,DX,DY, got {arg!r}")
        x, y, dx, dy = parts
        cli_call("send_pointer_event", {"session_id": sid, "event": {"kind": "drag", "x": x, "y": y, "dx": dx, "dy": dy}})
    elif kind == "wait":
        cli_call("wait", {"session_id": sid, "ms": float(arg)})
    else:
        raise ValueError(f"unknown action kind {kind!r}")


def record_onesim(app_path: Path, out_dir: Path, setup_actions=None, triggers=None, fps: float = 60.0,
                  duration_ms: float = 700.0, device: str | None = None, ios: str | None = None,
                  appearance: str = "light", onesim_bin: Path | None = None):
    """Drive `onesim serve` and record a 60fps deterministic frame sequence."""
    onesim_bin = onesim_bin or (ROOT / "target/debug/onesim")
    out_dir.mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen(
        [str(onesim_bin), "serve"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    seq = [0]

    def call(method, params=None):
        seq[0] += 1
        req = {"id": seq[0], "method": method}
        if params is not None:
            req["params"] = params
        assert proc.stdin and proc.stdout
        proc.stdin.write(json.dumps(req) + "\n")
        proc.stdin.flush()
        line = proc.stdout.readline()
        if not line:
            raise RuntimeError(f"onesim serve exited on {method}: {proc.stderr.read() if proc.stderr else ''}")
        resp = json.loads(line)
        if not resp.get("ok"):
            raise RuntimeError(f"{method} failed: {resp.get('error')}")
        return resp["result"]

    try:
        call("hello", {"protocol_version": 1})
        cparams = {"framework": "swiftui", "app_path": str(app_path), "appearance": appearance}
        if device:
            cparams["device"] = device
        if ios:
            cparams["os_version"] = ios
        sid = call("create_session", cparams)["session_id"]
        call("start_session", {"session_id": sid})

        for spec in (setup_actions or []):
            k, a = parse_action(spec)
            run_action(call, sid, k, a)

        # Capture resting frame before trigger.
        rest_file = out_dir / "rest.png"
        call("capture_screenshot", {"session_id": sid, "path": str(rest_file)})

        # Parse timed triggers: either "T_MS:kind:arg" or "kind:arg" (implies T_MS=0).
        timed = []
        for t in (triggers or []):
            head, sep, rest = t.partition(":")
            try:
                at_ms = float(head)
                k, a = parse_action(rest)
            except ValueError:
                at_ms = 0.0
                k, a = parse_action(t)
            timed.append((at_ms, k, a))
        timed.sort(key=lambda x: x[0])

        dt_ms = 1000.0 / fps
        n_frames = int(duration_ms / dt_ms) + 1
        frames = []
        trig_idx = 0
        t_cur = 0.0
        for i in range(n_frames):
            if i > 0:
                call("tick", {"session_id": sid, "dt_ms": dt_ms})
                t_cur = i * dt_ms
            while trig_idx < len(timed) and timed[trig_idx][0] <= t_cur + 1e-6:
                _, k, a = timed[trig_idx]
                run_action(call, sid, k, a)
                trig_idx += 1
            fname = f"frame_{i:04d}.png"
            call("capture_screenshot", {"session_id": sid, "path": str(out_dir / fname)})
            frames.append({"index": i, "file": fname, "t_ms": round(t_cur, 3)})

        manifest = {"fps": fps, "duration_ms": duration_ms, "t0_index": 0, "rest": "rest.png", "frames": frames}
        (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=2))
        call("stop_session", {"session_id": sid})
        call("shutdown")
        return manifest
    finally:
        proc.wait(timeout=5)


def build_filmstrip(rows, out_path: Path, native_label="iOS Simulator", scale_div=3):
    """Build a multi-row filmstrip PNG: each row is [Native | OneSim | Diff] at one sampled timestamp."""
    if not rows:
        raise ValueError("no rows to render into filmstrip")
    w, h = rows[0]["native"].size
    header_h = 56
    row_h = h + header_h
    canvas = Image.new("RGB", (w * 3, row_h * len(rows)), "white")
    draw = ImageDraw.Draw(canvas)
    for idx, r in enumerate(rows):
        y0 = idx * row_h
        heat = diff_heatmap(r["diff"])
        t_ms = r["t_ms"]
        df = r["delta_frames"]
        pct = r["pct"]
        panels = [
            (r["native"], f"{native_label}  t={t_ms:.0f}ms ({df:+d}f)"),
            (r["onesim"], f"OneSim  t={t_ms:.0f}ms"),
            (heat, f"diff {pct:.2f}%"),
        ]
        for col, (img, label) in enumerate(panels):
            canvas.paste(img, (col * w, y0 + header_h))
            draw.text((col * w + 18, y0 + 12), label, fill="black", font_size=30)
    if scale_div > 1:
        canvas = canvas.resize((max(1, canvas.width // scale_div), max(1, canvas.height // scale_div)))
    out_path.parent.mkdir(parents=True, exist_ok=True)
    canvas.save(out_path)


def compare_sequences(native_path: Path, onesim_path: Path, out_dir: Path, timestamps=None, window: int = 2,
                      fps: float = 60.0, motion_threshold: float = 0.15, max_diff: float = 2.0,
                      native_label: str = "iOS Simulator"):
    """Compare native vs. OneSim sequences at `timestamps` (ms) with +/-`window` frame matching."""
    timestamps = timestamps or DEFAULT_TIMESTAMPS
    out_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="onesim-anim-cmp-") as tmp:
        nat_frames, nat_t0, _ = load_frame_sequence(native_path, fps=fps, motion_threshold=motion_threshold, tmp_dir=Path(tmp))
        os_frames, os_t0, _ = load_frame_sequence(onesim_path, fps=fps, motion_threshold=motion_threshold, tmp_dir=Path(tmp))
        results = []
        filmstrip_rows = []
        for t_ms in timestamps:
            oi = frame_at_time(os_frames, t_ms)
            os_img = Image.open(os_frames[oi]["path"]).convert("RGB")
            ni, df, pct, mean, nat_img, diff = match_frame_in_window(os_img, nat_frames, t_ms, window=window)
            passed = pct < max_diff
            results.append({
                "t_ms": t_ms,
                "onesim_index": oi,
                "native_index": ni,
                "delta_frames": df,
                "diff_pct": round(pct, 3),
                "mean_abs_diff": round(mean, 2),
                "pass": passed,
            })
            filmstrip_rows.append({"t_ms": t_ms, "delta_frames": df, "pct": pct, "native": nat_img, "onesim": os_img, "diff": diff})
        filmstrip_path = out_dir / "filmstrip.png"
        build_filmstrip(filmstrip_rows, filmstrip_path, native_label=native_label)
        summary = {
            "native_t0_index": nat_t0,
            "onesim_t0_index": os_t0,
            "window_frames": window,
            "max_diff_pct": max_diff,
            "filmstrip": str(filmstrip_path),
            "samples": results,
            "all_pass": all(r["pass"] for r in results),
        }
        (out_dir / "report.json").write_text(json.dumps(summary, indent=2))
        return summary


def stitch_side_by_side_video(native_path: Path, onesim_path: Path, out_mp4: Path, duration_ms: float = 700.0,
                              fps: float = 60.0, motion_threshold: float = 0.15, include_diff: bool = False,
                              native_label: str = "iOS Simulator", scale_div: int = 2):
    """Stitch t0-aligned native vs. OneSim frames side-by-side with `t = XXX ms` overlay into an MP4."""
    if not shutil.which("ffmpeg"):
        raise RuntimeError("ffmpeg is required to encode comparison videos")
    with tempfile.TemporaryDirectory(prefix="onesim-anim-vid-") as tmp:
        tmp = Path(tmp)
        nat_frames, _, _ = load_frame_sequence(native_path, fps=fps, motion_threshold=motion_threshold, tmp_dir=tmp / "nat")
        os_frames, _, _ = load_frame_sequence(onesim_path, fps=fps, motion_threshold=motion_threshold, tmp_dir=tmp / "os")

        stitched_dir = tmp / "stitched"
        stitched_dir.mkdir()
        dt = 1000.0 / fps
        n_frames = int(duration_ms / dt) + 1
        cols = 3 if include_diff else 2

        for i in range(n_frames):
            t_ms = i * dt
            ni = frame_at_time(nat_frames, t_ms)
            oi = frame_at_time(os_frames, t_ms)
            nat_img = Image.open(nat_frames[ni]["path"]).convert("RGB")
            os_img = Image.open(os_frames[oi]["path"]).convert("RGB")
            if os_img.size != nat_img.size:
                os_img = os_img.resize(nat_img.size)
            w, h = nat_img.size
            header_h = 56
            frame = Image.new("RGB", (w * cols, h + header_h), (24, 24, 28))
            draw = ImageDraw.Draw(frame)
            frame.paste(nat_img, (0, header_h))
            frame.paste(os_img, (w, header_h))
            draw.text((18, 12), f"{native_label}   t = {t_ms:5.0f} ms", fill="white", font_size=28)
            draw.text((w + 18, 12), f"OneSim   t = {t_ms:5.0f} ms", fill="white", font_size=28)
            if include_diff:
                pct, _, diff = pixel_diff_stats(nat_img, os_img)
                frame.paste(diff_heatmap(diff), (w * 2, header_h))
                draw.text((w * 2 + 18, 12), f"Diff {pct:.2f}%", fill="white", font_size=28)
            if scale_div > 1:
                tw = max(2, (frame.width // scale_div) & ~1)
                th = max(2, (frame.height // scale_div) & ~1)
                frame = frame.resize((tw, th))
            else:
                tw = frame.width & ~1
                th = frame.height & ~1
                if (tw, th) != frame.size:
                    frame = frame.crop((0, 0, tw, th))
            frame.save(stitched_dir / f"frame_{i:04d}.png")

        out_mp4.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [
                "ffmpeg", "-y", "-loglevel", "error",
                "-framerate", str(fps),
                "-i", str(stitched_dir / "frame_%04d.png"),
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18",
                str(out_mp4),
            ],
            check=True,
        )
    return out_mp4


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_rec = sub.add_parser("record-onesim", help="Record a 60fps deterministic OneSim frame sequence")
    p_rec.add_argument("app", type=Path, help="SwiftUI app path or prebuilt executable")
    p_rec.add_argument("out_dir", type=Path)
    p_rec.add_argument("--setup", action="append", default=[], help="Setup action before rest frame (e.g. click:Controls)")
    p_rec.add_argument("--trigger", action="append", default=[], help="Timed trigger (e.g. click:Beta or 120:click:Alpha)")
    p_rec.add_argument("--fps", type=float, default=60.0)
    p_rec.add_argument("--duration-ms", type=float, default=700.0)
    p_rec.add_argument("--device", default=None)
    p_rec.add_argument("--ios", default=None)
    p_rec.add_argument("--appearance", choices=["light", "dark"], default="light")

    p_ext = sub.add_parser("extract-native", help="Extract 60fps frames from a native simulator recording and auto-align t0")
    p_ext.add_argument("video", type=Path)
    p_ext.add_argument("out_dir", type=Path)
    p_ext.add_argument("--fps", type=float, default=60.0)
    p_ext.add_argument("--motion-threshold", type=float, default=0.15)

    p_cmp = sub.add_parser("compare", help="Compare native vs. OneSim frames at sampled timestamps and build filmstrip")
    p_cmp.add_argument("native", type=Path, help="Native .mp4 recording or extracted frames dir")
    p_cmp.add_argument("onesim", type=Path, help="OneSim frames dir")
    p_cmp.add_argument("out_dir", type=Path)
    p_cmp.add_argument("--timestamps", default=",".join(map(str, DEFAULT_TIMESTAMPS)))
    p_cmp.add_argument("--window", type=int, default=2, help="Matching window in +/- frames (default 2)")
    p_cmp.add_argument("--fps", type=float, default=60.0)
    p_cmp.add_argument("--motion-threshold", type=float, default=0.15)
    p_cmp.add_argument("--max-diff", type=float, default=2.0)
    p_cmp.add_argument("--native-label", default="iOS Simulator")

    p_vid = sub.add_parser("video", help="Stitch t0-aligned native vs. OneSim side-by-side MP4")
    p_vid.add_argument("native", type=Path)
    p_vid.add_argument("onesim", type=Path)
    p_vid.add_argument("out_mp4", type=Path)
    p_vid.add_argument("--duration-ms", type=float, default=700.0)
    p_vid.add_argument("--fps", type=float, default=60.0)
    p_vid.add_argument("--motion-threshold", type=float, default=0.15)
    p_vid.add_argument("--diff", action="store_true", help="Include third diff-heatmap panel")
    p_vid.add_argument("--native-label", default="iOS Simulator")
    p_vid.add_argument("--scale-div", type=int, default=2)

    a = ap.parse_args(argv)
    if a.cmd == "record-onesim":
        m = record_onesim(a.app, a.out_dir, setup_actions=a.setup, triggers=a.trigger, fps=a.fps,
                          duration_ms=a.duration_ms, device=a.device, ios=a.ios, appearance=a.appearance)
        print(f"recorded {len(m['frames'])} OneSim frames to {a.out_dir} ({a.fps:.0f} fps, {a.duration_ms:.0f} ms)")
        return 0
    if a.cmd == "extract-native":
        m = extract_native(a.video, a.out_dir, fps=a.fps, motion_threshold=a.motion_threshold)
        print(f"extracted {len(m['frames'])} frames to {a.out_dir} (auto-aligned t0 at frame {m['t0_index']})")
        return 0
    if a.cmd == "compare":
        ts = [float(x.strip()) for x in a.timestamps.split(",") if x.strip()]
        s = compare_sequences(a.native, a.onesim, a.out_dir, timestamps=ts, window=a.window, fps=a.fps,
                              motion_threshold=a.motion_threshold, max_diff=a.max_diff, native_label=a.native_label)
        print(f"{'t_ms':>7}{'delta_f':>9}{'pixels >10% off':>17}{'mean abs diff':>15}  result (< {a.max_diff}%)")
        for r in s["samples"]:
            print(f"{r['t_ms']:>7.0f}{r['delta_frames']:>+9d}{r['diff_pct']:>16.2f}%{r['mean_abs_diff']:>15.1f}  {'pass' if r['pass'] else 'FAIL'}")
        print(f"filmstrip: {s['filmstrip']}")
        return 0 if s["all_pass"] else 1
    if a.cmd == "video":
        out = stitch_side_by_side_video(a.native, a.onesim, a.out_mp4, duration_ms=a.duration_ms, fps=a.fps,
                                        motion_threshold=a.motion_threshold, include_diff=a.diff,
                                        native_label=a.native_label, scale_div=a.scale_div)
        print(f"wrote {out}")
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
