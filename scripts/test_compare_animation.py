#!/usr/bin/env python3
"""Unit tests for scripts/compare_animation.py (t0 alignment, +/-window matching, filmstrip, video)."""
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
import compare_animation as ca  # noqa: E402


def make_bar_frame(width, height, bar_x):
    img = Image.new("RGB", (width, height), (255, 255, 255))
    draw = ImageDraw.Draw(img)
    draw.rectangle([bar_x, 40, bar_x + 40, 100], fill=(0, 122, 255))
    return img


class CompareAnimationTests(unittest.TestCase):
    def test_t0_auto_alignment_skips_input_latency_frames(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            # 5 resting frames (bar at x=20), then motion starts at frame 5.
            paths = []
            for i in range(12):
                x = 20 if i < 5 else 20 + (i - 4) * 15
                p = d / f"frame_{i:04d}.png"
                make_bar_frame(240, 160, x).save(p)
                paths.append(p)
            self.assertEqual(ca.find_t0_index(paths, motion_threshold_pct=0.15), 5)

    def test_window_matching_absorbs_1_frame_capture_jitter(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            nat_dir = tmp / "nat"
            os_dir = tmp / "os"
            nat_dir.mkdir()
            os_dir.mkdir()
            fps = 60.0
            dt = 1000.0 / fps
            # Native has 4 static lead-in frames + motion shifted by +1 frame jitter relative to OneSim.
            for i in range(24):
                x = 20 if i < 5 else 20 + min(120, (i - 5) * 10)
                make_bar_frame(240, 160, x).save(nat_dir / f"frame_{i:04d}.png")
            os_frames = []
            for i in range(20):
                x = 20 + min(120, i * 10)
                fname = f"frame_{i:04d}.png"
                make_bar_frame(240, 160, x).save(os_dir / fname)
                os_frames.append({"index": i, "file": fname, "t_ms": round(i * dt, 3)})
            (os_dir / "manifest.json").write_text(json.dumps({"fps": fps, "t0_index": 0, "frames": os_frames}))

            out_dir = tmp / "out"
            summary = ca.compare_sequences(nat_dir, os_dir, out_dir, timestamps=[50, 120, 200], window=2, fps=fps, max_diff=1.0)
            self.assertTrue(summary["all_pass"], summary)
            self.assertTrue((out_dir / "filmstrip.png").exists())
            self.assertTrue((out_dir / "report.json").exists())
            # Filmstrip has 3 rows stacked vertically.
            with Image.open(out_dir / "filmstrip.png") as fs:
                self.assertGreater(fs.height, 100)

    def test_compare_catches_snapped_animation(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            nat_dir = tmp / "nat"
            os_dir = tmp / "os"
            nat_dir.mkdir()
            os_dir.mkdir()
            fps = 60.0
            dt = 1000.0 / fps
            # Native animates smoothly; OneSim snaps immediately to x=140.
            for i in range(20):
                make_bar_frame(240, 160, 20 + min(120, i * 8)).save(nat_dir / f"frame_{i:04d}.png")
                make_bar_frame(240, 160, 140).save(os_dir / f"frame_{i:04d}.png")
            (os_dir / "manifest.json").write_text(json.dumps({
                "fps": fps, "t0_index": 0,
                "frames": [{"index": i, "file": f"frame_{i:04d}.png", "t_ms": round(i * dt, 3)} for i in range(20)],
            }))
            summary = ca.compare_sequences(nat_dir, os_dir, tmp / "out", timestamps=[50, 120], window=1, fps=fps, max_diff=2.0)
            self.assertFalse(summary["all_pass"], "snapped animation must fail mid-flight comparison")

    def test_compare_rejects_timestamps_beyond_captured_interval(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            nat_dir = tmp / "nat"
            os_dir = tmp / "os"
            nat_dir.mkdir()
            os_dir.mkdir()
            fps = 60.0
            dt = 1000.0 / fps
            # Both recordings only cover 0..100ms (7 frames).
            meta = []
            for i in range(7):
                fname = f"frame_{i:04d}.png"
                make_bar_frame(240, 160, 20 + i * 10).save(nat_dir / fname)
                make_bar_frame(240, 160, 20 + i * 10).save(os_dir / fname)
                meta.append({"index": i, "file": fname, "t_ms": round(i * dt, 3)})
            (nat_dir / "manifest.json").write_text(json.dumps({"fps": fps, "t0_index": 0, "frames": meta}))
            (os_dir / "manifest.json").write_text(json.dumps({"fps": fps, "t0_index": 0, "frames": meta}))

            summary = ca.compare_sequences(nat_dir, os_dir, tmp / "out", timestamps=[50, 350, 600], window=2, fps=fps, max_diff=2.0)
            self.assertFalse(summary["all_pass"], "timestamps beyond captured duration must fail")
            self.assertTrue(summary["samples"][0]["pass"])
            self.assertFalse(summary["samples"][0]["missing"])
            self.assertTrue(summary["samples"][1]["missing"])
            self.assertFalse(summary["samples"][1]["pass"])
            self.assertTrue(summary["samples"][2]["missing"])
            self.assertFalse(summary["samples"][2]["pass"])

    @unittest.skipUnless(shutil.which("ffmpeg"), "ffmpeg not installed")
    def test_compare_isolates_same_stem_video_extractions(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            for sub, x_fn in [("native", lambda i: 20 + i * 10), ("onesim", lambda _i: 160)]:
                d = tmp / sub
                d.mkdir()
                for i in range(12):
                    make_bar_frame(240, 160, x_fn(i)).save(d / f"frame_{i:04d}.png")
                subprocess.run(
                    ["ffmpeg", "-y", "-loglevel", "error", "-framerate", "30",
                     "-i", str(d / "frame_%04d.png"), "-c:v", "libx264", "-pix_fmt", "yuv420p",
                     str(d / "recording.mp4")],
                    check=True,
                )
            # Both inputs share the exact same filename stem `recording.mp4` but differ visually.
            summary = ca.compare_sequences(
                tmp / "native/recording.mp4",
                tmp / "onesim/recording.mp4",
                tmp / "out",
                timestamps=[50, 150],
                window=1,
                fps=30.0,
                max_diff=2.0,
            )
            self.assertFalse(summary["all_pass"], "same-stem videos must not overwrite each other's extracted frames")

    @unittest.skipUnless(shutil.which("ffmpeg"), "ffmpeg not installed")
    def test_stitch_side_by_side_video(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            nat_dir = tmp / "nat"
            os_dir = tmp / "os"
            nat_dir.mkdir()
            os_dir.mkdir()
            fps = 30.0
            dt = 1000.0 / fps
            frames_meta = []
            for i in range(10):
                fname = f"frame_{i:04d}.png"
                make_bar_frame(240, 160, 20 + i * 10).save(nat_dir / fname)
                make_bar_frame(240, 160, 20 + i * 10).save(os_dir / fname)
                frames_meta.append({"index": i, "file": fname, "t_ms": round(i * dt, 3)})
            (nat_dir / "manifest.json").write_text(json.dumps({"fps": fps, "t0_index": 0, "frames": frames_meta}))
            (os_dir / "manifest.json").write_text(json.dumps({"fps": fps, "t0_index": 0, "frames": frames_meta}))

            out_mp4 = tmp / "side_by_side.mp4"
            ca.stitch_side_by_side_video(nat_dir, os_dir, out_mp4, duration_ms=250.0, fps=fps, include_diff=True, scale_div=1)
            self.assertTrue(out_mp4.exists() and out_mp4.stat().st_size > 1000)
            if shutil.which("ffprobe"):
                probe = subprocess.run(
                    ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name,width,height", "-of", "json", str(out_mp4)],
                    capture_output=True, text=True, check=True,
                )
                info = json.loads(probe.stdout)["streams"][0]
                self.assertEqual(info["codec_name"], "h264")
                self.assertEqual(info["width"], 240 * 3)


if __name__ == "__main__":
    unittest.main(verbosity=1)
