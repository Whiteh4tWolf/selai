"""Create the looping README preview from the approved MP4 recording."""

from pathlib import Path
import subprocess

import imageio_ffmpeg


def main():
    media = Path(__file__).resolve().parents[2] / "docs" / "media"
    source = media / "selai-ssh-ai-demo.mp4"
    target = media / "selai-ssh-ai-demo.gif"
    # Stable dithering and changed rectangles keep the mostly static UI compact.
    filters = (
        "fps=6,scale=960:-1:flags=lanczos,split[a][b];"
        "[a]palettegen=max_colors=96:stats_mode=diff[p];"
        "[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle"
    )
    subprocess.run(
        [imageio_ffmpeg.get_ffmpeg_exe(), "-hide_banner", "-loglevel", "error",
         "-y", "-i", str(source), "-filter_complex", filters,
         "-loop", "0", str(target)],
        check=True,
    )
    print(f"Created {target.name}: {target.stat().st_size / 1024 / 1024:.2f} MiB")


if __name__ == "__main__":
    main()
