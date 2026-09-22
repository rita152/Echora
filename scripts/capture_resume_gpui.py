#!/usr/bin/env python3
"""Capture both themes of a saved resume matrix with this worktree's capture bundle."""
import argparse
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--manifest', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--binary', type=Path, default=None,
                    help='defaults to scripts/gpui_capture_binary.sh for this worktree')
parser.add_argument('--scroll-from-bottom', type=float, default=0)
args = parser.parse_args()
if args.binary is None:
    helper = Path(__file__).resolve().parent / 'gpui_capture_binary.sh'
    args.binary = Path(subprocess.check_output([str(helper)], text=True).strip())
args.output.mkdir(parents=True, exist_ok=True)
for sample in json.loads(args.manifest.read_text()):
    for theme in ['light', 'dark']:
        state = 'bottom' if args.scroll_from_bottom == 0 else f'scroll-{args.scroll_from_bottom:g}'
        prefix = args.output / f"{sample['slug']}-{theme}-{state}"
        with prefix.with_suffix('.log').open('w') as log:
            subprocess.run([str(args.binary.resolve()), f'--theme={theme}',
                '--window-width=1440', '--window-height=900',
                f"--resume-thread={sample['id']}", f'--resume-scroll-from-bottom={args.scroll_from_bottom}',
                f'--screenshot={prefix.with_suffix(".png").resolve()}'],
                stdout=log, stderr=log, check=True, timeout=90)
        print(prefix, flush=True)
